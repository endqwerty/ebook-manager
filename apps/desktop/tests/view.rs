//! The reader's web view against hostile chapters, on a real WebKitGTK.
//! Each test is a statement of docs/design/epub-reading.md, "What would show
//! this is wrong" and "Behaviour". Run under a private display and bus:
//! see apps/desktop/README.md.

#[allow(dead_code)]
#[path = "../src/reader/view.rs"]
mod view;

use libtest_mimic::{Arguments, Failed, Trial};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::net::TcpListener;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use view::{Config, Document, Event, Landing, Picture, Protect, Source, View, Zone};
use webkit6::prelude::*;
use webkit6::{gio, glib, gtk};

type Outcome = Result<(), Failed>;

macro_rules! check {
    ($cond:expr, $($arg:tt)*) => {
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !$cond {
            return Err(format!($($arg)*).into());
        }
    };
}

/// Counts connections to a local port, as a stand-in for the network.
struct Listener {
    port: u16,
    hits: Arc<AtomicU32>,
}

impl Listener {
    fn start() -> Listener {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        let hits = Arc::new(AtomicU32::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for stream in socket.incoming() {
                counter.fetch_add(1, Ordering::SeqCst);
                if let Ok(mut stream) = stream {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                    let _ = stream.read(&mut [0u8; 64]);
                }
            }
        });
        Listener { port, hits }
    }
    fn hits(&self) -> u32 {
        self.hits.load(Ordering::SeqCst)
    }
}

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x56, 0xc7, 0x2f, 0x0d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[derive(Default)]
struct Chapters {
    markup: HashMap<u32, String>,
    right_to_left: bool,
    pictures_served: std::cell::Cell<u32>,
}

impl Source for Chapters {
    fn chapter(&self, index: u32, done: Box<dyn FnOnce(Result<Document, String>)>) {
        let found = self.markup.get(&index).cloned();
        let right_to_left = self.right_to_left;
        glib::idle_add_local_once(move || {
            done(
                found
                    .map(|markup| Document {
                        markup,
                        language: Some("en".into()),
                        right_to_left,
                    })
                    .ok_or_else(|| "no such chapter".into()),
            )
        });
    }
    fn picture(&self, id: u32, done: Box<dyn FnOnce(Option<Picture>)>) {
        self.pictures_served.set(self.pictures_served.get() + 1);
        glib::idle_add_local_once(move || {
            done((id == 1).then(|| Picture {
                media_type: "image/png",
                bytes: PNG.to_vec(),
            }))
        });
    }
}

/// A view in a window, the events it has sent, and ways to wait on both.
struct Stage {
    view: View,
    window: gtk::Window,
    events: Rc<RefCell<Vec<Event>>>,
}

fn spin(mut done: impl FnMut() -> bool, limit: Duration) -> bool {
    let context = glib::MainContext::default();
    let end = Instant::now() + limit;
    loop {
        while context.iteration(false) {}
        if done() {
            return true;
        }
        if Instant::now() > end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn pause(time: Duration) {
    spin(|| false, time);
}

impl Stage {
    fn new(source: Rc<Chapters>, change: impl FnOnce(&mut Config)) -> Result<Stage, Failed> {
        Stage::new_with(source, change)
    }

    fn new_with(source: Rc<dyn Source>, change: impl FnOnce(&mut Config)) -> Result<Stage, Failed> {
        let dir = std::env::temp_dir().join(format!("ebook-view-test-{}", std::process::id()));
        let mut config = Config::new(dir.join("filters"));
        change(&mut config);
        let events = Rc::new(RefCell::new(Vec::new()));
        let built = Rc::new(RefCell::new(None));
        let sink = events.clone();
        let slot = built.clone();
        View::open(
            config,
            source,
            move |e| sink.borrow_mut().push(e),
            move |v| {
                slot.replace(Some(v));
            },
        );
        check!(
            spin(|| built.borrow().is_some(), Duration::from_secs(20)),
            "the view was never built"
        );
        let view = built.take().unwrap()?;
        let window = gtk::Window::builder()
            .default_width(900)
            .default_height(700)
            .child(view.widget())
            .build();
        window.present();
        Ok(Stage {
            view,
            window,
            events,
        })
    }

    fn wait(&self, what: &str, found: impl Fn(&Event) -> bool) -> Result<Event, Failed> {
        let mut hit = None;
        spin(
            || {
                hit = self.events.borrow().iter().find(|e| found(e)).cloned();
                hit.is_some()
            },
            Duration::from_secs(90),
        );
        hit.ok_or_else(|| format!("no {what}; events: {:?}", self.events.borrow()).into())
    }

    /// Load a chapter and wait until it shows its landing place.
    fn open(&self, chapter: u32, landing: Landing) -> Result<(u32, u32, u32, u32), Failed> {
        self.events.borrow_mut().clear();
        self.view.load(chapter, landing);
        let Event::Ready { length, .. } = self.wait("ready", |e| {
            matches!(e, Event::Ready { .. } | Event::Failed(_))
        })?
        else {
            return Err(format!("the chapter failed: {:?}", self.events.borrow()).into());
        };
        let Event::Place {
            offset,
            page,
            pages,
            ..
        } = self.wait("place", |e| matches!(e, Event::Place { .. }))?
        else {
            unreachable!()
        };
        Ok((length, offset, page, pages))
    }

    fn last_place(&self) -> Option<(u32, u32, u32)> {
        self.events.borrow().iter().rev().find_map(|e| match e {
            Event::Place {
                offset,
                page,
                pages,
                ..
            } => Some((*offset, *page, *pages)),
            _ => None,
        })
    }

    /// Evaluate in the app's script world and return the result as text.
    fn app(&self, body: &str) -> Result<String, Failed> {
        let out = Rc::new(RefCell::new(None));
        let slot = out.clone();
        self.view
            .call(body, glib::VariantDict::new(None), move |v| {
                slot.replace(Some(v.map(|v| v.to_str().to_string())));
            });
        check!(
            spin(|| out.borrow().is_some(), Duration::from_secs(20)),
            "no answer to {body}"
        );
        out.take()
            .unwrap()
            .ok_or_else(|| format!("{body} failed").into())
    }

    /// Evaluate in the page's own world, where a book's script would run.
    fn page(&self, script: &str) -> Result<String, Failed> {
        let out = Rc::new(RefCell::new(None));
        let slot = out.clone();
        self.view.widget().evaluate_javascript(
            script,
            None,
            None,
            gio::Cancellable::NONE,
            move |v| {
                slot.replace(Some(
                    v.map(|v| v.to_str().to_string()).map_err(|e| e.to_string()),
                ));
            },
        );
        check!(
            spin(|| out.borrow().is_some(), Duration::from_secs(20)),
            "no answer to {script}"
        );
        out.take().unwrap().map_err(Into::into)
    }

    fn turn(&self, by: i32) -> Result<bool, Failed> {
        let out = Rc::new(RefCell::new(None));
        let slot = out.clone();
        self.view.turn(by, false, move |moved| {
            slot.replace(Some(moved));
        });
        check!(
            spin(|| out.borrow().is_some(), Duration::from_secs(20)),
            "no answer to a turn"
        );
        Ok(out.take().unwrap())
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        self.window.destroy();
        pause(Duration::from_millis(50));
    }
}

/// Everything a book could try, pointed at `remote`. A script that runs
/// marks the document, where the app's world can see it.
fn hostile(remote: &str) -> String {
    let mark = "document.documentElement.setAttribute('data-ran','1')";
    format!(
        r#"<script>{mark}</script>
<script src="{remote}/s.js"></script>
<meta http-equiv="refresh" content="1;url={remote}/refresh"/>
<link rel="stylesheet" href="{remote}/a.css"/>
<link rel="prefetch" href="{remote}/prefetch"/>
<style>@import url("{remote}/import.css"); p {{ background: url("{remote}/bg.png"); font-family: x; }}
@font-face {{ font-family: x; src: url("{remote}/f.woff"); }}</style>
<p onclick="{mark}">Hello <em>world</em> text.</p>
<img src="{remote}/i.png" onerror="{mark}" alt="remote"/>
<img id="local" src="/r/1" alt="local"/>
<iframe src="{remote}/frame"></iframe>
<a id="js" href="javascript:{mark}">js</a>
<form id="f" action="{remote}/form"><input name="a"/></form>
<svg xmlns="http://www.w3.org/2000/svg" onload="{mark}"><image href="{remote}/svg.png"/></svg>
<video src="{remote}/v.mp4" autoplay="autoplay"></video>
<object data="{remote}/o"></object>
<body onload="{mark}"></body>"#
    )
}

fn one(markup: String) -> Rc<Chapters> {
    Rc::new(Chapters {
        markup: HashMap::from([(0, markup)]),
        ..Chapters::default()
    })
}

fn the_app_script_runs_and_the_book_script_does_not() -> Outcome {
    let listener = Listener::start();
    let stage = Stage::new(
        one(hostile(&format!("http://127.0.0.1:{}", listener.port))),
        |_| {},
    )?;
    let (length, _, _, pages) = stage.open(0, Landing::Offset(0))?;
    check!(
        length > 10 && pages >= 1,
        "the app's script measured nothing: {length} {pages}"
    );
    check!(
        stage.app("return document.documentElement.getAttribute('data-ran')")? == "null",
        "a book script ran"
    );
    check!(
        stage.app("return __reader.length()")? == length.to_string(),
        "length changed"
    );
    Ok(())
}

fn the_view_alone_blocks_scripts_requests_and_navigation() -> Outcome {
    let listener = Listener::start();
    let remote = format!("http://127.0.0.1:{}", listener.port);
    {
        let source = one(hostile(&remote));
        let stage = Stage::new(source.clone(), |_| {})?;
        stage.open(0, Landing::Offset(0))?;
        // Long enough for the meta refresh, the frame and the media to try.
        pause(Duration::from_secs(3));
        check!(
            listener.hits() == 0,
            "{} connections left the view",
            listener.hits()
        );
        check!(
            stage.app("return document.documentElement.getAttribute('data-ran')")? == "null",
            "a book script ran"
        );
        let uri = stage.view.widget().uri().unwrap_or_default();
        check!(
            uri.starts_with("ebook://") && uri.contains("/c/0"),
            "the view left the chapter: {uri}"
        );
        check!(
            stage.app("return document.getElementById('local').naturalWidth")? == "1",
            "the book's own picture did not load"
        );
        check!(
            source.pictures_served.get() >= 1,
            "no picture was asked for"
        );
        check!(stage.page("document.getElementById('js').click(); document.documentElement.getAttribute('data-ran')")? == "null", "a javascript: link ran");
    }
    // The same chapter with every protection off must be caught by both
    // detectors, or the checks above prove nothing.
    let open = Protect {
        no_markup_scripts: false,
        navigation: false,
        content_policy: false,
        filter: false,
        proxy: false,
    };
    let stage = Stage::new(one(hostile(&remote)), |c| c.protect = open)?;
    stage.events.borrow_mut().clear();
    stage.view.load(0, Landing::Offset(0));
    check!(
        spin(|| listener.hits() > 0, Duration::from_secs(20)),
        "the listener hears nothing even unprotected"
    );
    Ok(())
}

fn each_network_layer_blocks_alone() -> Outcome {
    for layer in ["content policy", "filter", "proxy"] {
        let listener = Listener::start();
        let remote = format!("http://127.0.0.1:{}", listener.port);
        // The book's scripts run here, so they can ask for addresses too.
        let fetching = format!(
            "{}<script>fetch('{remote}/fetch'); new Image().src='{remote}/img';</script>",
            hostile(&remote)
        );
        let protect = Protect {
            no_markup_scripts: false,
            navigation: true,
            content_policy: layer == "content policy",
            filter: layer == "filter",
            proxy: layer == "proxy",
        };
        let stage = Stage::new(one(fetching), |c| c.protect = protect)?;
        stage.open(0, Landing::Offset(0))?;
        pause(Duration::from_secs(3));
        check!(
            listener.hits() == 0,
            "with only the {layer}, {} connections left",
            listener.hits()
        );
    }
    Ok(())
}

fn the_bridge_accepts_only_its_own_world_and_valid_messages() -> Outcome {
    let stage = Stage::new(one("<p>One two three.</p>".into()), |_| {})?;
    stage.open(0, Landing::Offset(0))?;
    let seen = stage.page("typeof (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.reader) + ' ' + typeof window.__reader")?;
    check!(
        seen == "undefined undefined",
        "the page's world sees the bridge: {seen}"
    );
    stage.events.borrow_mut().clear();
    let send = |message: &str| {
        stage.app(&format!(
            "window.webkit.messageHandlers.reader.postMessage({message}); return 1"
        ))
    };
    let load = "Number(new URLSearchParams(location.search).get('l'))";
    send("{kind: 'place', load: 9999, offset: 1, page: 0, pages: 1}")?;
    send(&format!(
        "{{kind: 'place', load: {load}, offset: -1, page: 0, pages: 1}}"
    ))?;
    send(&format!(
        "{{kind: 'place', load: {load}, offset: 1.5, page: 0, pages: 1}}"
    ))?;
    send(&format!(
        "{{kind: 'place', load: {load}, offset: 1, page: 5, pages: 2}}"
    ))?;
    send(&format!(
        "{{kind: 'place', load: {load}, offset: 1e12, page: 0, pages: 1}}"
    ))?;
    send(&format!(
        "{{kind: 'place', load: {load}, offset: '1', page: 0, pages: 1}}"
    ))?;
    send(&format!(
        "{{kind: 'jump', load: {load}, href: 'https://example.org/'}}"
    ))?;
    send(&format!(
        "{{kind: 'jump', load: {load}, href: '/c/1/../../etc'}}"
    ))?;
    send(&format!("{{kind: 'jump', load: {load}, href: '/r/1'}}"))?;
    send(&format!(
        "{{kind: 'external', load: {load}, address: 'file:///etc/passwd'}}"
    ))?;
    send(&format!(
        "{{kind: 'external', load: {load}, address: 'javascript:alert(1)'}}"
    ))?;
    send(&format!(
        "{{kind: 'external', load: {load}, address: 'https://example.org/a b'}}"
    ))?;
    send(&format!("{{kind: 'tap', load: {load}, zone: 'elsewhere'}}"))?;
    send(&format!("{{kind: 'unknown', load: {load}}}"))?;
    send("'text'")?;
    send("null")?;
    pause(Duration::from_millis(300));
    check!(
        stage.events.borrow().is_empty(),
        "a bad message got through: {:?}",
        stage.events.borrow()
    );
    send(&format!(
        "{{kind: 'place', load: {load}, offset: 3, page: 0, pages: 1}}"
    ))?;
    send(&format!(
        "{{kind: 'jump', load: {load}, href: '/c/4#a%20b'}}"
    ))?;
    send(&format!(
        "{{kind: 'external', load: {load}, address: 'https://example.org/x'}}"
    ))?;
    pause(Duration::from_millis(300));
    let events = stage.events.borrow().clone();
    check!(
        events
            == [
                Event::Place {
                    chapter: 0,
                    offset: 3,
                    page: 0,
                    pages: 1
                },
                Event::Jump {
                    chapter: 4,
                    anchor: Some("a b".into())
                },
                Event::External("https://example.org/x".into()),
            ],
        "good messages were not passed on as sent: {events:?}"
    );
    Ok(())
}

fn long_chapter() -> String {
    let mut markup = String::from("<h1>Tide tables</h1>");
    for n in 0..120 {
        markup.push_str(&format!(
            "<p id=\"p{n}\">Marker {n}. \u{1F600} {}</p>",
            "The lantern shines along the northern shore. ".repeat(6)
        ));
    }
    markup
}

fn a_place_survives_reflow() -> Outcome {
    let stage = Stage::new(one(long_chapter()), |_| {})?;
    let (length, _, _, pages) = stage.open(0, Landing::Offset(0))?;
    check!(pages > 5, "the chapter is one page: {pages}");
    // A place in the middle of a paragraph, after characters outside the
    // basic plane, found by its text.
    let target: u32 = stage.app("const n = document.getElementById('p70').firstChild; let o = 0; const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT); while (w.nextNode() && w.currentNode !== n) o += w.currentNode.data.length; return o + 120")?.parse().unwrap();
    check!(target < length, "target past the end");
    let visible = |stage: &Stage, t: u32| -> Result<bool, Failed> {
        Ok(stage.app(&format!("const t = {t}; const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT); let o = 0; while (w.nextNode()) {{ const l = w.currentNode.data.length; if (o + l > t) {{ const r = document.createRange(); r.setStart(w.currentNode, t - o); r.setEnd(w.currentNode, t - o + 1); const b = r.getBoundingClientRect(); return b.left >= 0 && b.right <= innerWidth && b.top >= 0 && b.bottom <= innerHeight; }} o += l; }} return false"))? == "true")
    };
    let (_, saved, page, _) = stage.open(0, Landing::Offset(target))?;
    check!(
        visible(&stage, target)?,
        "the place is not on the page it opened at"
    );
    // The place the reader saves is the first character of that page. It is
    // inside the paragraph, not at its top.
    check!(
        saved <= target && target - saved < 3000,
        "the page starts at {saved}, far from {target}"
    );
    check!(
        stage.app(&format!("return __reader.charAt({saved} - 1) !== ''"))? == "true",
        "the page starts at the chapter's top"
    );
    // A larger font, then a narrower window: the same character stays shown.
    stage.events.borrow_mut().clear();
    stage.view.set_style(&[("--size", "1.6")]);
    stage.wait("place after a font change", |e| {
        matches!(e, Event::Place { .. })
    })?;
    let (_, new_page, new_pages) = stage.last_place().unwrap();
    check!(
        new_pages > pages && new_page > page,
        "the larger font did not reflow: {new_page}/{new_pages}"
    );
    check!(
        visible(&stage, saved)?,
        "the place was lost when the font grew"
    );
    stage.events.borrow_mut().clear();
    stage.window.set_default_size(520, 640);
    stage.wait("place after a resize", |e| matches!(e, Event::Place { .. }))?;
    pause(Duration::from_millis(300));
    check!(
        stage.app("return innerWidth")?.parse::<f64>().unwrap() < 700.0,
        "the window did not narrow"
    );
    check!(
        visible(&stage, saved)?,
        "the place was lost when the window narrowed"
    );
    // Reopening at the saved offset in the new layout shows it too.
    stage.open(0, Landing::Offset(saved))?;
    check!(visible(&stage, saved)?, "the place was lost on reopening");
    Ok(())
}

fn turns_cross_chapters() -> Outcome {
    let source = Rc::new(Chapters {
        markup: HashMap::from([
            (0, long_chapter()),
            (1, "<p id=\"only\">Short.</p>".to_owned()),
        ]),
        ..Chapters::default()
    });
    let stage = Stage::new(source, |_| {})?;
    let (_, _, page, pages) = stage.open(0, Landing::End)?;
    check!(
        page == pages - 1 && pages > 1,
        "End did not land on the last page: {page}/{pages}"
    );
    check!(!stage.turn(1)?, "a turn forward moved on the last page");
    check!(stage.turn(-1)?, "a turn back did not move on the last page");
    check!(
        stage.last_place().unwrap().1 == pages - 2,
        "a turn back went elsewhere"
    );
    let (_, offset, page, pages) = stage.open(1, Landing::Offset(0))?;
    check!(
        (offset, page, pages) == (0, 0, 1),
        "the next chapter did not open at its start"
    );
    check!(
        !stage.turn(-1)? && !stage.turn(1)?,
        "a one-page chapter turned"
    );
    let (_, _, page, _) = stage.open(0, Landing::Anchor("p70".into()))?;
    check!(page > 0, "the anchor was not found");
    check!(stage.app("const b = document.getElementById('p70').getBoundingClientRect(); return b.left >= 0 && b.left < innerWidth")? == "true", "the anchor's page is not showing");
    let (_, offset, page, _) = stage.open(0, Landing::Anchor("missing".into()))?;
    check!(
        (offset, page) == (0, 0),
        "a missing anchor did not open the chapter's start"
    );
    Ok(())
}

fn right_to_left_chapters_turn_and_keep_their_place() -> Outcome {
    let source = Rc::new(Chapters {
        markup: HashMap::from([(0, long_chapter())]),
        right_to_left: true,
        ..Chapters::default()
    });
    let stage = Stage::new(source, |_| {})?;
    let (_, _, page, pages) = stage.open(0, Landing::Offset(0))?;
    check!(page == 0 && pages > 5, "{page}/{pages}");
    check!(stage.turn(1)? && stage.turn(1)?, "no turn forward");
    let (offset, page, _) = stage.last_place().unwrap();
    check!(page == 2 && offset > 0, "page {page} offset {offset}");
    let (_, again, page, _) = stage.open(0, Landing::Offset(offset))?;
    check!(
        (again, page) == (offset, 2),
        "reopened at {again} on page {page}"
    );
    Ok(())
}

fn page_colours_match_the_design() -> Outcome {
    let stage = Stage::new(one("<p>Text <a href=\"/c/0\">link</a></p>".into()), |_| {})?;
    stage.open(0, Landing::Offset(0))?;
    let colours = |stage: &Stage| {
        stage.app("const s = getComputedStyle(document.documentElement); return s.backgroundColor + ' ' + s.color + ' ' + getComputedStyle(document.querySelector('a')).color")
    };
    // Paper: #f4ecd8 with #3b2f21.
    check!(
        colours(&stage)? == "rgb(244, 236, 216) rgb(59, 47, 33) rgb(59, 47, 33)",
        "Paper is {}",
        colours(&stage)?
    );
    stage.events.borrow_mut().clear();
    stage.view.set_style(&[
        ("--page", "#2b2b2e"),
        ("--ink", "#d6d6d8"),
        ("--secondary", "#929294"),
    ]);
    stage.wait("place", |e| matches!(e, Event::Place { .. }))?;
    // Grey: #2b2b2e with #d6d6d8.
    check!(
        colours(&stage)? == "rgb(43, 43, 46) rgb(214, 214, 216) rgb(214, 214, 216)",
        "Grey is {}",
        colours(&stage)?
    );
    Ok(())
}

fn links_and_taps_reach_the_shell_and_open_nothing() -> Outcome {
    let listener = Listener::start();
    let remote = format!("http://127.0.0.1:{}/x", listener.port);
    let markup = format!(
        "<p><a id=\"in\" href=\"/c/3#n1\">in</a> <a id=\"out\" data-external=\"{remote}\">out</a> <a id=\"bad\" data-external=\"file:///etc/passwd\">bad</a> <a id=\"none\">none</a></p>"
    );
    let stage = Stage::new(one(markup), |_| {})?;
    stage.open(0, Landing::Offset(0))?;
    stage.events.borrow_mut().clear();
    for id in ["in", "out", "bad", "none"] {
        stage.app(&format!(
            "document.getElementById('{id}').click(); return 1"
        ))?;
    }
    stage.app("document.getElementById('out').dispatchEvent(new KeyboardEvent('keydown', {key: 'Enter', bubbles: true})); return 1")?;
    pause(Duration::from_millis(500));
    let events = stage.events.borrow().clone();
    check!(
        events
            == [
                Event::Jump {
                    chapter: 3,
                    anchor: Some("n1".into())
                },
                Event::External(remote.clone()),
                Event::External(remote.clone()),
            ],
        "{events:?}"
    );
    check!(
        listener.hits() == 0,
        "an external link was opened without asking"
    );
    let uri = stage.view.widget().uri().unwrap_or_default();
    check!(uri.contains("/c/0"), "a link navigated the view: {uri}");
    check!(
        stage.app(
            "const a = document.getElementById('out'); return a.getAttribute('role') + a.tabIndex"
        )? == "link0",
        "an external link cannot be reached from the keyboard"
    );
    stage.events.borrow_mut().clear();
    for (x, zone) in [(0.1, Zone::Back), (0.5, Zone::Middle), (0.9, Zone::Forward)] {
        stage.app(&format!("document.documentElement.dispatchEvent(new MouseEvent('click', {{clientX: innerWidth * {x}, clientY: 300, bubbles: true}})); return 1"))?;
        stage.wait("tap", |e| *e == Event::Tap(zone))?;
    }
    stage.events.borrow_mut().clear();
    stage.app("document.documentElement.dispatchEvent(new MouseEvent('click', {clientX: innerWidth * 0.3, clientY: 300, bubbles: true})); return 1")?;
    pause(Duration::from_millis(200));
    check!(
        stage.events.borrow().is_empty(),
        "a click between the zones did something"
    );
    Ok(())
}

fn web_processes() -> Vec<u32> {
    let me = std::process::id();
    let parent = |pid: u32| -> Option<u32> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(") ")?.1.split(' ').nth(1)?.parse().ok()
    };
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc").unwrap().flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        if !name.starts_with("WebKitWebProces") {
            continue;
        }
        let mut at = pid;
        for _ in 0..12 {
            match parent(at) {
                Some(p) if p == me => {
                    found.push(pid);
                    break;
                }
                Some(p) if p > 1 => at = p,
                _ => break,
            }
        }
    }
    found
}

/// How many core dumps the machine has recorded, where it keeps a record.
fn core_dumps() -> Option<usize> {
    let out = std::process::Command::new("coredumpctl")
        .args(["list", "--no-pager", "-q"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).lines().count())
}

fn the_web_process_has_no_network_and_is_ended_past_its_limits() -> Outcome {
    {
        let stage = Stage::new(one("<p>Quiet.</p>".into()), |c| {
            c.watchdog = Duration::from_secs(2)
        })?;
        stage.open(0, Landing::Offset(0))?;
        let processes = web_processes();
        // Under nextest this is the one view's process. Run with other
        // tests in one process, earlier views' processes may still be ending.
        check!(
            !processes.is_empty(),
            "no web process of this test was found"
        );
        let mine = std::fs::read_link("/proc/self/ns/net").map_err(|e| e.to_string())?;
        for process in processes {
            let Ok(theirs) = std::fs::read_link(format!("/proc/{process}/ns/net")) else {
                check!(
                    !std::path::Path::new(&format!("/proc/{process}")).exists(),
                    "cannot read the network namespace of web process {process}"
                );
                continue;
            };
            check!(
                mine != theirs,
                "web process {process} shares the app's network: {theirs:?}"
            );
        }
        // A call that never answers is ended by the watchdog.
        stage.events.borrow_mut().clear();
        stage
            .view
            .call("for(;;){}", glib::VariantDict::new(None), |_| {});
        let failed = stage.wait("failure", |e| matches!(e, Event::Failed(_)))?;
        check!(
            failed == Event::Failed("This chapter took too long to display.".into()),
            "{failed:?}"
        );
        stage.open(0, Landing::Offset(0))?;
    }
    Ok(())
}

/// A book through the real sandboxed helper, as the reader serves it.
struct Helper(ebook_reader::Book);

impl Source for Helper {
    fn chapter(&self, index: u32, done: Box<dyn FnOnce(Result<Document, String>)>) {
        let read = self.0.read_content(index);
        glib::idle_add_local_once(move || {
            done(
                read.map(|c| Document {
                    markup: c.markup,
                    language: c.language,
                    right_to_left: c.right_to_left,
                })
                .map_err(|e| e.to_string()),
            )
        });
    }
    fn picture(&self, id: u32, done: Box<dyn FnOnce(Option<Picture>)>) {
        let read = self.0.read_resource(ebook_reader::ResourceId(id)).ok();
        glib::idle_add_local_once(move || {
            done(read.map(|r| Picture {
                media_type: r.kind.media_type(),
                bytes: r.bytes,
            }))
        });
    }
}

/// A small EPUB whose chapters have what makes counting hard: nested inline
/// elements, entities, characters outside the basic plane, pictures that
/// load and that do not, tables, preformatted text and hidden text.
fn fixture_book(dir: &std::path::Path) -> std::path::PathBuf {
    use std::io::Write;
    let chapters = [
        "<h1>One &amp; <em>only</em></h1><p>Plain <b>bold <i>both</i></b> text.\r\nSecond line \u{1F600}\u{1F4DA} after.</p><p>caf\u{e9} na\u{ef}ve \u{4e2d}\u{6587} &lt;tag&gt;</p>",
        "<p>Before <img src=\"dot.png\" alt=\"a dot\"/> between <img src=\"missing.png\" alt=\"Lost picture\"/> after.</p><table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td></td></tr></table><pre>  two\n    lines\t</pre>",
        "<div><p hidden=\"hidden\">gone</p><script>gone()</script><ul><li>one</li><li>two<ol><li>deep</li></ol></li></ul><blockquote><p>Quoted <a href=\"c0.xhtml#x\">link</a> and <a href=\"https://example.org/\">out</a>.</p></blockquote></div>",
    ];
    let mut manifest = String::from(r#"<item id="dot" href="dot.png" media-type="image/png"/>"#);
    let mut spine = String::new();
    for i in 0..chapters.len() {
        manifest.push_str(&format!(
            r#"<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>"#
        ));
        spine.push_str(&format!(r#"<itemref idref="c{i}"/>"#));
    }
    let path = dir.join("book.epub");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let packed = zip::write::SimpleFileOptions::default();
    archive.start_file("mimetype", stored).unwrap();
    archive.write_all(b"application/epub+zip").unwrap();
    archive
        .start_file("META-INF/container.xml", packed)
        .unwrap();
    archive.write_all(br#"<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    archive.start_file("book.opf", packed).unwrap();
    archive.write_all(format!(r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">view-test</dc:identifier><dc:title>View test</dc:title><dc:language>en</dc:language></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#).as_bytes()).unwrap();
    archive.start_file("dot.png", packed).unwrap();
    archive.write_all(PNG).unwrap();
    for (i, body) in chapters.iter().enumerate() {
        archive.start_file(format!("c{i}.xhtml"), packed).unwrap();
        archive.write_all(format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>c</title></head><body>\n{body}\n</body></html>").as_bytes()).unwrap();
    }
    archive.finish().unwrap();
    path
}

/// The text of markup in the helper's form, by dropping tags and reading the
/// three entities, as UTF-16.
fn text_of(markup: &str) -> Vec<u16> {
    let mut text = String::new();
    let mut rest = markup;
    while let Some(c) = rest.chars().next() {
        if c == '<' {
            rest = &rest[rest.find('>').unwrap() + 1..];
        } else if let Some((entity, plain)) = [("&amp;", '&'), ("&lt;", '<'), ("&gt;", '>')]
            .iter()
            .find(|(e, _)| rest.starts_with(e))
        {
            text.push(*plain);
            rest = &rest[entity.len()..];
        } else {
            text.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    text.encode_utf16().collect()
}

/// Check 4 of the design: for every chapter of a book read through the real
/// helper, the page counts the text the helper counted, and each offset is
/// the same UTF-16 unit on both sides.
fn page_and_helper_agree_on_offsets() -> Outcome {
    let dir = std::env::temp_dir().join(format!("ebook-view-book-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let helper = std::env::current_exe()
        .ok()
        // The helper is beside the build's programs, some folders above this one.
        .and_then(|exe| {
            exe.ancestors()
                .map(|dir| dir.join("ebook-reader-helper"))
                .find(|p| p.is_file())
        })
        .ok_or("the helper is not built: cargo build -p ebook-reader --bin ebook-reader-helper")?;
    let reader = ebook_reader::Reader::new(ebook_reader::ReaderConfig {
        helper_path: Some(helper),
        ..Default::default()
    })
    .map_err(|e| e.to_string())?;
    let book = reader
        .open(&fixture_book(&dir))
        .map_err(|e| e.to_string())?;
    let chapters = book.chapters().len() as u32;
    let stage = Stage::new_with(Rc::new(Helper(book.clone())), |_| {})?;
    for chapter in 0..chapters {
        let content = book.read_content(chapter).map_err(|e| e.to_string())?;
        let text = text_of(&content.markup);
        check!(
            text.len() as u32 == content.length,
            "chapter {chapter}: the helper's length is not its text's"
        );
        let (length, _, _, _) = stage.open(chapter, Landing::Offset(0))?;
        check!(
            length == content.length,
            "chapter {chapter}: the page counts {length}, the helper {}",
            content.length
        );
        let units = stage.app("let s = ''; for (let i = 0; i < __reader.length(); i++) s += __reader.charAt(i).charCodeAt(0) + ','; return s")?;
        let page: Vec<u16> = units
            .split(',')
            .filter(|u| !u.is_empty())
            .map(|u| u.parse().unwrap())
            .collect();
        check!(
            page == text,
            "chapter {chapter}: the page's text differs from the helper's at unit {:?}",
            page.iter().zip(&text).position(|(a, b)| a != b)
        );
        if chapter == 1 {
            check!(
                stage.app("return document.querySelector('img').naturalWidth")? == "1",
                "the book's picture did not load from the helper"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Two views open at once, one reloaded and closed while the other stays.
/// A second web context, or compiling the content filter again, made
/// WebKitGTK 2.54 abort a web process here. The check in `main` sees it.
fn two_views_share_one_process_without_a_crash() -> Outcome {
    let first = Stage::new(one(long_chapter()), |_| {})?;
    first.open(0, Landing::Offset(0))?;
    let second = Stage::new(one(long_chapter()), |_| {})?;
    second.open(0, Landing::Offset(0))?;
    first.open(0, Landing::Offset(500))?;
    drop(first);
    pause(Duration::from_secs(1));
    let (_, _, _, pages) = second.open(0, Landing::End)?;
    check!(pages > 1, "the second view stopped working");
    Ok(())
}

fn main() {
    let mut arguments = Arguments::from_args();
    // GTK belongs to one thread.
    arguments.test_threads = Some(1);
    if arguments.list {
        // Listing needs the names only, and no display.
        let names = TESTS
            .iter()
            .map(|(name, _)| Trial::test(*name, || Ok(())))
            .collect();
        libtest_mimic::run(&arguments, names).exit();
    }
    gtk::init().expect("a display: run under apps/desktop/tests/private-session.sh");
    let dumps = core_dumps();
    let trials = TESTS
        .iter()
        .map(|(name, test)| Trial::test(*name, *test))
        .collect();
    let conclusion = libtest_mimic::run(&arguments, trials);
    // Users must never see a crash, so no test here may leave a core dump,
    // whatever it asserted. A dump is recorded some seconds after the crash.
    if let Some(before) = dumps {
        std::thread::sleep(Duration::from_secs(10));
        let after = core_dumps().unwrap_or(before);
        if after > before {
            eprintln!(
                "error: {} core dumps were recorded during these tests",
                after - before
            );
            std::process::exit(101);
        }
    }
    conclusion.exit();
}

type Test = (&'static str, fn() -> Outcome);

const TESTS: &[Test] = &[
    (
        "the_app_script_runs_and_the_book_script_does_not",
        the_app_script_runs_and_the_book_script_does_not,
    ),
    (
        "the_view_alone_blocks_scripts_requests_and_navigation",
        the_view_alone_blocks_scripts_requests_and_navigation,
    ),
    (
        "each_network_layer_blocks_alone",
        each_network_layer_blocks_alone,
    ),
    (
        "the_bridge_accepts_only_its_own_world_and_valid_messages",
        the_bridge_accepts_only_its_own_world_and_valid_messages,
    ),
    ("a_place_survives_reflow", a_place_survives_reflow),
    ("turns_cross_chapters", turns_cross_chapters),
    (
        "right_to_left_chapters_turn_and_keep_their_place",
        right_to_left_chapters_turn_and_keep_their_place,
    ),
    (
        "page_colours_match_the_design",
        page_colours_match_the_design,
    ),
    (
        "links_and_taps_reach_the_shell_and_open_nothing",
        links_and_taps_reach_the_shell_and_open_nothing,
    ),
    (
        "the_web_process_has_no_network_and_is_ended_past_its_limits",
        the_web_process_has_no_network_and_is_ended_past_its_limits,
    ),
    (
        "two_views_share_one_process_without_a_crash",
        two_views_share_one_process_without_a_crash,
    ),
    (
        "page_and_helper_agree_on_offsets",
        page_and_helper_agree_on_offsets,
    ),
];

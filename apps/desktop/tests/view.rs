//! The reader's web view against hostile chapters, on a real WebKitGTK.
//! Each test is a statement of docs/design/epub-reading.md, "What would show
//! this is wrong" and "Behaviour". Run under a private display and bus:
//! see apps/desktop/README.md.

#[allow(dead_code)]
#[path = "../src/reader/view.rs"]
mod view;
// The reader's place-keeping, which the tests at the end drive with what a
// real page says, as the reader's window does.
#[allow(dead_code)]
#[path = "../src/reader/place.rs"]
mod place;
// The text settings panel, which the panel's tests build alone.
#[allow(dead_code, unused_imports)]
#[path = "../src/reader/settings.rs"]
mod settings;
#[allow(dead_code)]
#[path = "../src/reader/text_panel.rs"]
mod text_panel;

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

thread_local! {
    /// The width the next test window opens at.
    static WIDTH: std::cell::Cell<i32> = const { std::cell::Cell::new(900) };
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
            .default_width(WIDTH.with(|w| w.get()))
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
        let Event::Ready { load } = self.wait("ready", |e| {
            matches!(e, Event::Ready { .. } | Event::Failed(_))
        })?
        else {
            return Err(format!("the chapter failed: {:?}", self.events.borrow()).into());
        };
        let length = load.length;
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

    /// Turn, and give the place the turn told the shell before it answered,
    /// or `None` at the chapter's edge.
    fn turned(&self, by: i32, animate: bool) -> Result<Option<(u32, u32)>, Failed> {
        self.events.borrow_mut().clear();
        let answered = Rc::new(RefCell::new(None));
        let slot = answered.clone();
        let seen = self.events.clone();
        self.view.turn(by, animate, move |moved| {
            slot.replace(Some((moved, seen.borrow().last().cloned())));
        });
        check!(
            spin(|| answered.borrow().is_some(), Duration::from_secs(20)),
            "no answer to a turn"
        );
        match answered.take().unwrap() {
            (false, _) => Ok(None),
            (
                true,
                Some(Event::Place {
                    offset,
                    page,
                    moved: true,
                    ..
                }),
            ) => Ok(Some((offset, page))),
            (true, other) => Err(format!("a turn moved and told {other:?}").into()),
        }
    }

    /// How far the pages are scrolled.
    fn scrolled(&self) -> Result<String, Failed> {
        self.app("return Math.round(document.documentElement.scrollLeft)")
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        self.view.shut();
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
        "{{kind: 'place', load: {load}, offset: -1, page: 0, pages: 1, move: 1, moved: true}}"
    ))?;
    send(&format!(
        "{{kind: 'place', load: {load}, offset: 1.5, page: 0, pages: 1, move: 1, moved: true}}"
    ))?;
    send(&format!(
        "{{kind: 'place', load: {load}, offset: 1, page: 5, pages: 2, move: 1, moved: true}}"
    ))?;
    send(&format!(
        "{{kind: 'place', load: {load}, offset: 1e12, page: 0, pages: 1, move: 1, moved: true}}"
    ))?;
    // Past the end of the chapter's text, which is 14 units long.
    send(&format!(
        "{{kind: 'place', load: {load}, offset: 15, page: 0, pages: 1, move: 1, moved: true}}"
    ))?;
    send(&format!(
        "{{kind: 'place', load: {load}, offset: '1', page: 0, pages: 1, move: 1, moved: true}}"
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
        "{{kind: 'place', load: {load}, offset: 3, page: 0, pages: 1, move: 7, moved: true}}"
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
                    load: view::Load {
                        number: 1,
                        chapter: 0,
                        length: 14,
                    },
                    offset: 3,
                    page: 0,
                    pages: 1,
                    count: 7,
                    moved: true,
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

/// Either layer that stops a book's script does so with the other off, and
/// with both off the script runs, so the two checks can fail.
fn each_script_layer_blocks_alone() -> Outcome {
    let ran = |markup_off: bool, policy: bool| -> Result<String, Failed> {
        let protect = Protect {
            no_markup_scripts: markup_off,
            content_policy: policy,
            navigation: true,
            filter: false,
            proxy: true,
        };
        let stage = Stage::new(one(hostile("http://127.0.0.1:9")), |c| c.protect = protect)?;
        stage.open(0, Landing::Start)?;
        stage.app("return document.documentElement.getAttribute('data-ran')")
    };
    check!(
        ran(true, false)? == "null",
        "a script ran with markup scripts off"
    );
    check!(
        ran(false, true)? == "null",
        "a script ran under the content policy"
    );
    check!(
        ran(false, false)? == "1",
        "no script ran even unprotected, so the checks prove nothing"
    );
    Ok(())
}

/// The scheme gives a view its own book and nothing of another view's.
fn a_token_serves_only_its_own_view() -> Outcome {
    let first = Stage::new(one("<p>First.</p>".into()), |_| {})?;
    first.open(0, Landing::Start)?;
    let second = Stage::new(one("<p>Second.</p>".into()), |_| {})?;
    second.open(0, Landing::Start)?;
    let origin = |stage: &Stage| -> String {
        let uri = stage.view.widget().uri().unwrap_or_default().to_string();
        uri.split("/c/").next().unwrap_or_default().to_owned()
    };
    let width = |stage: &Stage, address: &str| {
        stage.app(&format!("return await new Promise(done => {{ const i = new Image(); i.onload = () => done(i.naturalWidth); i.onerror = () => done(0); i.src = '{address}/r/1'; }})"))
    };
    check!(origin(&first) != origin(&second), "two views share a token");
    check!(
        width(&first, &origin(&first))? == "1",
        "a view cannot load its own book's picture"
    );
    check!(
        width(&first, &origin(&second))? == "0",
        "a view loaded another view's picture"
    );
    check!(
        width(&first, "ebook://unknown")? == "0",
        "an unknown token was served"
    );
    // A closed view's token is forgotten.
    let gone = origin(&second);
    drop(second);
    pause(Duration::from_millis(500));
    check!(
        width(&first, &gone)? == "0",
        "a closed view's token is still served"
    );
    Ok(())
}

/// A chapter the source cannot give fails when the source says so, not at
/// the time limit, and the next chapter opens.
fn a_chapter_that_cannot_be_read_fails_at_once() -> Outcome {
    let stage = Stage::new(one("<p>Here.</p>".into()), |_| {})?;
    stage.events.borrow_mut().clear();
    let started = Instant::now();
    stage.view.load(7, Landing::Start);
    let failed = stage.wait("failure", |e| matches!(e, Event::Failed(_)))?;
    check!(
        failed == Event::Failed("no such chapter".into()),
        "{failed:?}"
    );
    check!(
        started.elapsed() < Duration::from_secs(5),
        "the failure waited for the time limit"
    );
    stage.open(0, Landing::Start)?;
    Ok(())
}

/// Pages with no text on them: a chapter opens on its first page even when
/// that page is only a picture, and its end is its last page.
fn pages_without_text_keep_their_place() -> Outcome {
    let markup = "<div style=\"height: 90vh\"></div><p id=\"t\">Text on the second page.</p><div style=\"height: 250vh\"></div>";
    let stage = Stage::new(one(markup.into()), |_| {})?;
    let (length, offset, page, pages) = stage.open(0, Landing::Start)?;
    check!(pages >= 4, "the chapter has {pages} pages");
    check!(
        (page, offset) == (0, 0),
        "Start opened page {page} at {offset}"
    );
    check!(stage.turn(1)?, "no turn from the first page");
    check!(
        stage.last_place().unwrap().1 == 1,
        "a turn skipped the text page"
    );
    let (_, offset, page, _) = stage.open(0, Landing::End)?;
    check!(page == pages - 1, "End opened page {page} of {pages}");
    check!(
        offset == length,
        "a page after all the text reports {offset}, not the length {length}"
    );
    let (_, _, page, _) = stage.open(0, Landing::Offset(offset))?;
    check!(
        page == pages - 1,
        "the end reopened on page {page} of {pages}"
    );
    // A reflow keeps a text-less first or last page showing.
    let reflowed = |stage: &Stage| -> Result<(u32, u32), Failed> {
        stage.events.borrow_mut().clear();
        stage.view.set_style(&[("--size", "1.4")]);
        stage.wait("place after a reflow", |e| matches!(e, Event::Place { .. }))?;
        let (_, page, pages) = stage.last_place().unwrap();
        stage.view.set_style(&[("--size", "1")]);
        pause(Duration::from_millis(300));
        Ok((page, pages))
    };
    let (page, pages) = reflowed(&stage)?;
    check!(
        page == pages - 1,
        "a reflow left the last page for page {page} of {pages}"
    );
    stage.open(0, Landing::Start)?;
    let (page, _) = reflowed(&stage)?;
    check!(page == 0, "a reflow left the first page for page {page}");
    // A text-less page in the middle stays while the page count does: here
    // a change of colour, which moves nothing.
    stage.open(0, Landing::End)?;
    check!(stage.turn(-1)?, "no turn back from the last page");
    let (_, middle, pages) = stage.last_place().unwrap();
    check!(
        middle > 1 && middle < pages - 1,
        "page {middle} of {pages} is not in the blank run"
    );
    stage.events.borrow_mut().clear();
    stage.view.set_style(&[("--secondary", "#929294")]);
    stage.wait("place after a restyle", |e| {
        matches!(e, Event::Place { .. })
    })?;
    let (_, page, _) = stage.last_place().unwrap();
    check!(
        page == middle,
        "a text-less page in the middle moved from {middle} to {page}"
    );
    Ok(())
}

/// A chapter read that answers late, for a load since replaced, does not
/// fail the load that replaced it.
fn a_late_error_does_not_fail_a_newer_load() -> Outcome {
    struct Slow {
        first: std::cell::Cell<bool>,
        asked: Rc<std::cell::Cell<u32>>,
    }
    impl Source for Slow {
        fn chapter(&self, _: u32, done: Box<dyn FnOnce(Result<Document, String>)>) {
            self.asked.set(self.asked.get() + 1);
            if self.first.replace(false) {
                // The first read fails, but only after the second was asked for.
                glib::timeout_add_local_once(Duration::from_millis(400), move || {
                    done(Err("too late to matter".into()))
                });
            } else {
                glib::timeout_add_local_once(Duration::from_millis(900), move || {
                    done(Ok(Document {
                        markup: "<p>Second try.</p>".into(),
                        language: None,
                        right_to_left: false,
                    }))
                });
            }
        }
        fn picture(&self, _: u32, done: Box<dyn FnOnce(Option<Picture>)>) {
            done(None)
        }
    }
    let asked = Rc::new(std::cell::Cell::new(0));
    let source = Rc::new(Slow {
        first: std::cell::Cell::new(true),
        asked: asked.clone(),
    });
    let stage = Stage::new_with(source, |_| {})?;
    stage.view.load(0, Landing::Start);
    // The second load starts once the first has reached the source.
    check!(
        spin(|| asked.get() == 1, Duration::from_secs(20)),
        "the first load never asked for its chapter"
    );
    stage.events.borrow_mut().clear();
    stage.view.load(0, Landing::Start);
    let first = stage.wait("an answer", |e| {
        matches!(e, Event::Ready { .. } | Event::Failed(_))
    })?;
    check!(
        matches!(first, Event::Ready { .. }),
        "the late error failed the newer load: {first:?}"
    );
    Ok(())
}

/// A colour set while a chapter is still loading is the colour it opens in.
fn a_style_set_during_a_load_applies() -> Outcome {
    let stage = Stage::new(one("<p>Text.</p>".into()), |_| {})?;
    stage.open(0, Landing::Start)?;
    stage.events.borrow_mut().clear();
    stage.view.load(0, Landing::Start);
    // The document has been asked for, and may already be written.
    pause(Duration::from_millis(30));
    stage.view.set_style(&[("--page", "#2b2b2e")]);
    stage.wait("place", |e| matches!(e, Event::Place { .. }))?;
    let colour = stage.app("return getComputedStyle(document.documentElement).backgroundColor")?;
    check!(
        colour == "rgb(43, 43, 46)",
        "the chapter opened in {colour}"
    );
    Ok(())
}

/// A turn made in one chapter and answered after the next was asked for is
/// still told to the shell, as that chapter's place, before the next
/// chapter's own place and in an order that says which is newer.
fn a_turn_answered_after_a_chapter_change_is_still_told() -> Outcome {
    let source = Rc::new(Chapters {
        markup: HashMap::from([(0, long_chapter()), (1, "<p>Short.</p>".to_owned())]),
        ..Chapters::default()
    });
    let stage = Stage::new(source, |_| {})?;
    stage.open(0, Landing::Start)?;
    stage.events.borrow_mut().clear();
    // The turn is asked for, and the next chapter in the same breath, so
    // the turn's answer can only arrive after the load has moved on.
    stage.view.turn(1, false, |_| {});
    stage.view.load(1, Landing::Start);
    stage.wait(
        "the next chapter's place",
        |e| matches!(e, Event::Place { load, .. } if load.chapter == 1),
    )?;
    let places: Vec<(u32, u32, bool, (u32, u32))> = stage
        .events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            Event::Place {
                load,
                page,
                moved,
                count,
                ..
            } => Some((load.chapter, *page, *moved, (load.number, *count))),
            _ => None,
        })
        .collect();
    let turn = places.iter().find(|p| p.0 == 0);
    let landing = places.iter().find(|p| p.0 == 1);
    let (Some(turn), Some(landing)) = (turn, landing) else {
        return Err(format!("a place is missing: {places:?}").into());
    };
    check!(turn.1 == 1 && turn.2, "the turn's place is {turn:?}");
    check!(
        landing.2 && landing.3 > turn.3,
        "the landing {landing:?} is not after the turn {turn:?}"
    );
    Ok(())
}

/// Every move tells the shell its place in the step that reports the move,
/// and a reflow changes the page and not the place to save.
fn a_move_carries_its_place_and_a_reflow_does_not_move_it() -> Outcome {
    let stage = Stage::new(one(long_chapter()), |_| {})?;
    let (_, landed, page, _) = stage.open(0, Landing::Offset(9000))?;
    // A turn: the place has reached the shell by the time the turn answers.
    stage.events.borrow_mut().clear();
    let answered = Rc::new(RefCell::new(None));
    let slot = answered.clone();
    let seen = stage.events.clone();
    stage.view.turn(1, false, move |moved| {
        slot.replace(Some((moved, seen.borrow().last().cloned())));
    });
    check!(
        spin(|| answered.borrow().is_some(), Duration::from_secs(20)),
        "no answer to a turn"
    );
    let (moved, place) = answered.take().unwrap();
    let Some(Event::Place {
        offset: turned,
        page: now,
        ..
    }) = place
    else {
        return Err(format!("the turn answered {moved} with no place before it: {place:?}").into());
    };
    check!(
        moved && now == page + 1 && turned > landed,
        "turned to page {now} at {turned}"
    );
    // A turn at the edge moves nothing and tells nothing.
    stage.open(0, Landing::End)?;
    stage.events.borrow_mut().clear();
    check!(!stage.turn(1)?, "a turn past the end moved");
    check!(
        stage.events.borrow().is_empty(),
        "an edge sent {:?}",
        stage.events.borrow()
    );
    // A reflow, from a new size of type or of window, reports new pages and
    // the same place to save.
    let (_, held, page, pages) = stage.open(0, Landing::Offset(turned))?;
    stage.events.borrow_mut().clear();
    stage.view.set_style(&[("--size", "1.6")]);
    stage.wait("place after a font change", |e| {
        matches!(e, Event::Place { .. })
    })?;
    stage.window.set_default_size(560, 640);
    pause(Duration::from_millis(800));
    let places: Vec<(u32, u32, u32)> = stage
        .events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            Event::Place {
                offset,
                page,
                pages,
                ..
            } => Some((*offset, *page, *pages)),
            _ => None,
        })
        .collect();
    check!(
        places.iter().all(|p| p.0 == held),
        "a reflow changed the place to save from {held}: {places:?}"
    );
    let reports: Vec<(bool, (u32, u32))> = stage
        .events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            Event::Place {
                load, moved, count, ..
            } => Some((*moved, (load.number, *count))),
            _ => None,
        })
        .collect();
    check!(reports.len() >= 2, "the reflows reported {reports:?}");
    check!(
        reports.iter().all(|r| !r.0 && r.1 == reports[0].1),
        "a reflow was reported as a move: {reports:?}"
    );
    let last = places.last().copied().unwrap_or((held, page, pages));
    check!(
        last.2 > pages && last.1 > page,
        "the reflow did not repaginate: {last:?}, was {page} of {pages}"
    );
    Ok(())
}

/// Set properties and wait for the page to have gone back to its place.
fn restyle(stage: &Stage, properties: &[(&str, &str)]) -> Result<(u32, u32, u32), Failed> {
    stage.events.borrow_mut().clear();
    stage.view.set_style(properties);
    stage.wait("place after a setting", |e| {
        matches!(e, Event::Place { .. })
    })?;
    Ok(stage.last_place().unwrap())
}

/// Each text setting changes what the page computes for its text.
fn text_settings_reach_the_page() -> Outcome {
    let stage = Stage::new(one(long_chapter()), |_| {})?;
    stage.open(0, Landing::Start)?;
    let computed = |stage: &Stage, property: &str| {
        stage.app(&format!(
            "return getComputedStyle(document.querySelector('p')).{property}"
        ))
    };
    let root: f64 = stage
        .app("return parseFloat(getComputedStyle(document.documentElement).fontSize)")?
        .parse()
        .unwrap();
    restyle(&stage, &[("--size", "1.5")])?;
    let larger: f64 = computed(&stage, "fontSize")?
        .trim_end_matches("px")
        .parse()
        .unwrap();
    check!(
        (larger / root - 1.5).abs() < 0.01,
        "size 1.5 gave {larger} from {root}"
    );
    restyle(&stage, &[("--size", "1"), ("--weight", "600")])?;
    check!(computed(&stage, "fontWeight")? == "600", "weight");
    restyle(&stage, &[("--line", "2")])?;
    let line: f64 = computed(&stage, "lineHeight")?
        .trim_end_matches("px")
        .parse()
        .unwrap();
    check!(
        (line / root - 2.0).abs() < 0.05,
        "line height 2 gave {line} at {root}"
    );
    restyle(&stage, &[("--letter", "0.1em"), ("--word", "0.2em")])?;
    let letter: f64 = computed(&stage, "letterSpacing")?
        .trim_end_matches("px")
        .parse()
        .unwrap();
    let word: f64 = computed(&stage, "wordSpacing")?
        .trim_end_matches("px")
        .parse()
        .unwrap();
    check!(
        (letter / root - 0.1).abs() < 0.01 && (word / root - 0.2).abs() < 0.01,
        "spacing {letter} {word}"
    );
    restyle(&stage, &[("--align", "justify"), ("--hyphens", "auto")])?;
    check!(computed(&stage, "textAlign")? == "justify", "justification");
    check!(computed(&stage, "hyphens")? == "auto", "hyphenation");
    restyle(&stage, &[("--font", "'DejaVu Sans', sans-serif")])?;
    check!(
        computed(&stage, "fontFamily")?.contains("DejaVu Sans"),
        "font"
    );
    // Margins: the side margin scales, and the line never passes the measure.
    let left = |stage: &Stage| -> Result<f64, Failed> {
        Ok(stage
            .app("return document.querySelector('p').getBoundingClientRect().left")?
            .parse()
            .unwrap())
    };
    stage.window.set_default_size(700, 700);
    pause(Duration::from_millis(400));
    restyle(&stage, &[("--margin", "1")])?;
    let medium = left(&stage)?;
    restyle(&stage, &[("--margin", "1.5")])?;
    let wide = left(&stage)?;
    check!(
        (medium - 48.0).abs() < 1.0 && (wide - 72.0).abs() < 1.0,
        "margins {medium} {wide}"
    );
    Ok(())
}

/// Two columns show when asked for and the window fits them, with the
/// margins' width between them, and turning still moves a whole page.
fn two_columns_when_they_fit() -> Outcome {
    // The private display is 1,280 wide, so the text is set small enough for
    // two columns of 60 characters to fit a 1,200 window.
    WIDTH.with(|w| w.set(1200));
    let stage = Stage::new(one(long_chapter()), |_| {});
    WIDTH.with(|w| w.set(900));
    let stage = stage?;
    stage.view.set_style(&[("--size", "0.8")]);
    let (_, _, _, one_column) = stage.open(0, Landing::Start)?;
    check!(
        spin(
            || stage.app("return innerWidth").is_ok_and(|w| w == "1200"),
            Duration::from_secs(10)
        ),
        "the window is {} wide, not 1200 (gtk window {}x{}, view {}, ratio {})",
        stage.app("return innerWidth")?,
        stage.window.width(),
        stage.window.height(),
        stage.view.widget().width(),
        stage.app("return devicePixelRatio + ' ' + screen.width")?
    );
    let (_, _, two_column) = restyle(&stage, &[("--columns", "2")])?;
    check!(
        stage.app("return document.documentElement.classList.contains('two')")? == "true",
        "two columns were not used in a wide window"
    );
    check!(
        two_column < one_column,
        "two columns gave {two_column} pages, one gave {one_column}"
    );
    // The page holds two columns: paragraphs at two left edges, both on
    // screen. Each column is at most the measure, 34 em, and the pair is
    // centred, with the margins' width between them.
    let columns = |stage: &Stage| -> Result<(Vec<f64>, f64, f64), Failed> {
        let found = stage.app("const xs = new Set(); let widest = 0; for (const p of document.querySelectorAll('p')) { const r = p.getBoundingClientRect(); if (r.left >= 0 && r.right <= innerWidth) { xs.add(Math.round(r.left)); widest = Math.max(widest, r.width); } } return [...xs].sort((a, b) => a - b).join(',') + ' ' + widest + ' ' + parseFloat(getComputedStyle(document.body).fontSize)")?;
        let mut parts = found.split(' ');
        let edges = parts
            .next()
            .unwrap_or("")
            .split(',')
            .filter_map(|e| e.parse().ok())
            .collect();
        let widest = parts.next().and_then(|w| w.parse().ok()).unwrap_or(0.0);
        let em = parts.next().and_then(|e| e.parse().ok()).unwrap_or(16.0);
        Ok((edges, widest, em))
    };
    let expected = |em: f64| {
        let column = f64::min(34.0 * em, (1200.0 - 4.0 * 48.0) / 2.0);
        let side = (1200.0 - 2.0 * column) / 4.0;
        (column, side, side + column + 2.0 * side)
    };
    let (edges, _, em) = columns(&stage)?;
    let (_, first, second) = expected(em);
    check!(
        edges.len() == 2 && (edges[0] - first).abs() < 2.0 && (edges[1] - second).abs() < 2.0,
        "columns start at {edges:?}, not {first} and {second}"
    );
    // Smaller text in the same window: the columns stop at the measure and
    // the pair stays centred.
    restyle(&stage, &[("--size", "0.6")])?;
    let (edges, widest, em) = columns(&stage)?;
    let (column, first, second) = expected(em);
    check!(
        column < (1200.0 - 4.0 * 48.0) / 2.0 && widest <= column + 1.0,
        "at size 0.6 a column is {widest} wide, the measure {column}"
    );
    check!(
        edges.len() == 2 && (edges[0] - first).abs() < 2.0 && (edges[1] - second).abs() < 2.0,
        "at size 0.6 the columns start at {edges:?}, not {first} and {second}"
    );
    restyle(&stage, &[("--size", "0.8")])?;
    check!(stage.turn(1)?, "no turn");
    check!(
        stage.app("return Math.round(document.documentElement.scrollLeft)")? == "1200",
        "a turn did not move one page"
    );
    // A window too narrow for two columns of 60 characters shows one.
    stage.events.borrow_mut().clear();
    stage.window.set_default_size(800, 800);
    stage.wait("place after a resize", |e| matches!(e, Event::Place { .. }))?;
    pause(Duration::from_millis(300));
    check!(
        stage.app("return document.documentElement.classList.contains('two')")? == "false",
        "two columns in a narrow window"
    );
    Ok(())
}

/// A turn answers with the place it leads to when the page slides or fades
/// there, though the text has not moved when the answer is made. Where the
/// animation ends is not reported again. Turns animate in pages, which is
/// all the reader shows.
fn an_animated_turn_answers_with_the_place_it_leads_to() -> Outcome {
    let stage = Stage::new(one(long_chapter()), |_| {})?;
    for turn in ["slide", "fade"] {
        stage.view.set_style(&[("--turn", turn)]);
        // The turn animates: right after it the text is still where it was.
        stage.open(0, Landing::Start)?;
        let probe = stage.app("const r = document.documentElement; const at = () => Math.round(r.scrollLeft); const before = at(); const answer = __reader.turn(1, true); return [before, at(), answer !== ''].join(' ')")?;
        let probe: Vec<&str> = probe.split(' ').collect();
        check!(
            probe[0] == probe[1] && probe[2] == "true",
            "{turn}: the turn did not animate: {probe:?}"
        );
        // The same turn without animation, from the same place.
        stage.open(0, Landing::Start)?;
        let Some((plain, plain_page)) = stage.turned(1, false)? else {
            return Err(format!("{turn}: no turn").into());
        };
        let landed = stage.scrolled()?;
        // The animated turn tells the same place, the first character of
        // the page it leads to, and ends there.
        let (_, start, _, _) = stage.open(0, Landing::Start)?;
        let Some((offset, page)) = stage.turned(1, true)? else {
            return Err(format!("{turn}: no animated turn").into());
        };
        check!(
            offset == plain && page == plain_page && offset > start,
            "{turn}: the animated turn told {offset} on page {page}, a plain one {plain} on page {plain_page}, from {start}"
        );
        pause(Duration::from_millis(1200));
        check!(
            stage.scrolled()? == landed,
            "{turn}: the animation ended at {}, the turn at {landed}",
            stage.scrolled()?
        );
        let places = stage
            .events
            .borrow()
            .iter()
            .filter(|e| matches!(e, Event::Place { .. }))
            .count();
        check!(
            places == 1,
            "{turn}: the turn was told {places} times: {:?}",
            stage.events.borrow()
        );
    }
    Ok(())
}

/// Two turns made in quick succession, the second before the first slide
/// or fade ends, land two pages on and report two places in order.
fn two_quick_turns_land_two_pages_on() -> Outcome {
    let stage = Stage::new(one(long_chapter()), |_| {})?;
    for turn in ["slide", "fade"] {
        stage.view.set_style(&[("--turn", turn)]);
        // Where two plain turns lead.
        stage.open(0, Landing::Start)?;
        let (Some(first), Some(second)) = (stage.turned(1, false)?, stage.turned(1, false)?) else {
            return Err(format!("{turn}: no plain turns").into());
        };
        let landed = stage.scrolled()?;
        // In one task, so the second turn is surely made before the first
        // slide has moved the text at all.
        stage.open(0, Landing::Start)?;
        let probe = stage.app("const r = document.documentElement; const at = () => Math.round(r.scrollLeft); const before = at(); const a = __reader.turn(1, true); const mid = at(); const b = __reader.turn(1, true); return [before, mid, a.split(',')[0], b.split(',')[0]].join(' ')")?;
        let probe: Vec<&str> = probe.split(' ').collect();
        check!(
            probe[0] == probe[1]
                && probe[2] == first.0.to_string()
                && probe[3] == second.0.to_string(),
            "{turn}: two turns in one task answered {probe:?}, plain turns {first:?} {second:?}"
        );
        pause(Duration::from_millis(1200));
        check!(
            stage.scrolled()? == landed,
            "{turn}: two turns in one task ended at {}, not {landed}",
            stage.scrolled()?
        );
        // Through the view, as the reader's keys turn: two calls with no
        // wait between them.
        stage.open(0, Landing::Start)?;
        stage.events.borrow_mut().clear();
        let answers = Rc::new(RefCell::new(Vec::new()));
        for _ in 0..2 {
            let answers = answers.clone();
            stage
                .view
                .turn(1, true, move |moved| answers.borrow_mut().push(moved));
        }
        check!(
            spin(|| answers.borrow().len() == 2, Duration::from_secs(20)),
            "{turn}: no answer to two turns"
        );
        check!(
            *answers.borrow() == [true, true],
            "{turn}: two turns answered {:?}",
            answers.borrow()
        );
        pause(Duration::from_millis(1200));
        let places: Vec<(u32, u32, u32, bool)> = stage
            .events
            .borrow()
            .iter()
            .filter_map(|e| match e {
                Event::Place {
                    offset,
                    page,
                    count,
                    moved,
                    ..
                } => Some((*offset, *page, *count, *moved)),
                _ => None,
            })
            .collect();
        check!(
            places.len() == 2
                && (places[0].0, places[0].1) == first
                && (places[1].0, places[1].1) == second
                && places[0].2 < places[1].2
                && places.iter().all(|p| p.3),
            "{turn}: two turns told {places:?}, plain turns {first:?} {second:?}"
        );
        check!(
            stage.scrolled()? == landed,
            "{turn}: two turns ended at {}, not {landed}",
            stage.scrolled()?
        );
    }
    Ok(())
}

/// A turn made after the layout changed and before the place is shown in
/// it counts from the held place's page in the new layout, not from where
/// an unfinished slide or fade was going in the old one.
fn a_turn_after_a_reflow_counts_from_the_held_place() -> Outcome {
    let stage = Stage::new(one(long_chapter()), |_| {})?;
    for turn in ["slide", "fade"] {
        stage.view.set_style(&[("--turn", turn), ("--size", "1")]);
        stage.open(0, Landing::Start)?;
        let answer = stage.app("__reader.turn(1, true); __reader.style({'--size': '0.8'}); return __reader.turn(1, true)")?;
        let quick: u32 = answer.split(',').next().unwrap_or("").parse().unwrap_or(0);
        pause(Duration::from_millis(600));
        // The same, one step at a time.
        stage.open(0, Landing::Start)?;
        let Some((first, _)) = stage.turned(1, false)? else {
            return Err("no turn".into());
        };
        restyle(&stage, &[("--size", "0.8")])?;
        let Some((expected, _)) = stage.turned(1, false)? else {
            return Err("no turn after the reflow".into());
        };
        check!(
            quick == expected,
            "{turn}: a turn before the reflow was shown went to {quick}, after it to {expected} (the first turn held {first})"
        );
    }
    Ok(())
}

/// An animation that something ends early, here a change of layout,
/// leaves the page at the place the turn told, so what is shown and what
/// was told never part. The turn's place is told once, as a move, and the
/// reflow's report is not a move.
fn an_animation_ended_by_a_reflow_leaves_the_page_at_its_place() -> Outcome {
    let stage = Stage::new(one(long_chapter()), |_| {})?;
    let visible = "const t = __OFFSET__; const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT); let o = 0; while (w.nextNode()) { const l = w.currentNode.data.length; if (o + l > t) { const r = document.createRange(); r.setStart(w.currentNode, t - o); r.setEnd(w.currentNode, t - o + 1); const b = r.getBoundingClientRect(); return b.left >= 0 && b.right <= innerWidth; } o += l; } return false";
    for turn in ["slide", "fade"] {
        stage.view.set_style(&[("--turn", turn), ("--size", "1")]);
        stage.open(0, Landing::Start)?;
        stage.events.borrow_mut().clear();
        // The change of size is asked for straight after the turn, while
        // the slide or the fade is still under way.
        let answered = Rc::new(RefCell::new(None));
        let slot = answered.clone();
        stage.view.turn(1, true, move |moved| {
            slot.replace(Some(moved));
        });
        stage.view.set_style(&[("--size", "0.8")]);
        check!(
            spin(|| answered.borrow().is_some(), Duration::from_secs(20)),
            "{turn}: no answer to the turn"
        );
        stage.wait("place after the reflow", |e| {
            matches!(e, Event::Place { moved: false, .. })
        })?;
        pause(Duration::from_millis(600));
        let reports: Vec<(u32, bool)> = stage
            .events
            .borrow()
            .iter()
            .filter_map(|e| match e {
                Event::Place { offset, moved, .. } => Some((*offset, *moved)),
                _ => None,
            })
            .collect();
        let moves: Vec<u32> = reports.iter().filter(|r| r.1).map(|r| r.0).collect();
        check!(
            moves.len() == 1,
            "{turn}: the turn was told as a move {} times: {reports:?}",
            moves.len()
        );
        let turned = moves[0];
        check!(
            reports.iter().filter(|r| !r.1).all(|r| r.0 == turned) && !reports.last().unwrap().1,
            "{turn}: after the turn to {turned} the reflow told {reports:?}"
        );
        check!(
            stage.app(&visible.replace("__OFFSET__", &turned.to_string()))? == "true",
            "{turn}: the place told, {turned}, is not on the page showing"
        );
    }
    Ok(())
}

/// When the system asks for less motion, the reader sets `--motion` to
/// "none", and a turn asked to animate moves the page at once, whatever the
/// page turn setting. WebKitGTK 2.54 did not
/// answer `prefers-reduced-motion` from GTK's animation setting when this
/// was written, so the reader passes it on itself.
fn less_motion_turns_at_once() -> Outcome {
    let stage = Stage::new(one(long_chapter()), |_| {})?;
    for turn in ["slide", "fade"] {
        stage
            .view
            .set_style(&[("--turn", turn), ("--motion", "none")]);
        stage.open(0, Landing::Start)?;
        let probe = stage.app("const r = document.documentElement; const at = () => Math.round(r.scrollLeft); const before = at(); __reader.turn(1, true); return [before, at()].join(' ')")?;
        let probe: Vec<&str> = probe.split(' ').collect();
        check!(
            probe[0] != probe[1],
            "{turn}: with less motion the text had not moved after the turn: {probe:?}"
        );
    }
    Ok(())
}

/// Every widget under `root`, in order.
fn widgets(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut all = vec![root.clone()];
    let mut child = root.first_child();
    while let Some(c) = child {
        all.extend(widgets(&c));
        child = c.next_sibling();
    }
    all
}

/// A text settings panel in a window, opened, on its own settings.
struct Panel {
    window: gtk::Window,
    popover: gtk::Popover,
    settings: Rc<RefCell<settings::Settings>>,
    /// The settings as the last change left them, where the reader saves.
    saved: Rc<RefCell<Option<settings::Settings>>>,
}

impl Panel {
    fn new(saved: settings::Settings) -> Panel {
        let settings = Rc::new(RefCell::new(saved));
        let saved = Rc::new(RefCell::new(None));
        let changed = {
            let (settings, saved) = (Rc::downgrade(&settings), saved.clone());
            Rc::new(move || {
                if let Some(settings) = settings.upgrade() {
                    saved.replace(Some(settings.borrow().clone()));
                }
            })
        };
        let popover = text_panel::build(settings.clone(), Vec::new(), changed);
        let button = gtk::MenuButton::builder().label("Aa").build();
        button.set_popover(Some(&popover));
        let window = gtk::Window::builder()
            .default_width(600)
            .default_height(700)
            .child(&button)
            .build();
        window.present();
        pause(Duration::from_millis(200));
        popover.popup();
        pause(Duration::from_millis(300));
        Panel {
            window,
            popover,
            settings,
            saved,
        }
    }

    fn all(&self) -> Vec<gtk::Widget> {
        widgets(self.popover.upcast_ref())
    }

    /// A button by its tooltip, which is its name in words, or its label.
    fn button(&self, name: &str) -> gtk::Button {
        self.all()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Button>().ok())
            .find(|b| {
                b.tooltip_text().as_deref() == Some(name) || b.label().as_deref() == Some(name)
            })
            .unwrap_or_else(|| panic!("no button {name}"))
    }

    fn swatch(&self, name: &str) -> gtk::ToggleButton {
        self.all()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::ToggleButton>().ok())
            .find(|b| b.tooltip_text().as_deref() == Some(name))
            .unwrap_or_else(|| panic!("no swatch {name}"))
    }

    /// The switch on the row named `label`.
    fn switch(&self, label: &str) -> gtk::Switch {
        self.all()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .find(|l| l.text() == label)
            .and_then(|l| l.parent())
            .and_then(|row| row.last_child())
            .and_then(|w| w.downcast::<gtk::Switch>().ok())
            .unwrap_or_else(|| panic!("no switch {label}"))
    }

    /// The control on the row named `label`.
    fn control<T: IsA<gtk::Widget>>(&self, label: &str) -> T {
        self.all()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .find(|l| l.text() == label)
            .and_then(|l| l.parent())
            .and_then(|row| row.last_child())
            .and_then(|w| w.downcast::<T>().ok())
            .unwrap_or_else(|| panic!("no control {label}"))
    }

    /// Whether every control shows the settings, and if not, which does not.
    fn consistent(&self) -> Result<(), String> {
        use settings::{Colour, LETTERS, LINES, Margin, SIZES, Turn, WORDS};
        let s = self.settings.borrow().clone();
        let mut wrong = Vec::new();
        if self
            .switch("Page colour follows light and dark")
            .is_active()
            != (s.colour == Colour::System)
        {
            wrong.push("follow".to_owned());
        }
        for (name, colour) in [
            ("White", Colour::White),
            ("Paper", Colour::Paper),
            ("Grey", Colour::Grey),
            ("Black", Colour::Black),
            ("A colour of your own", Colour::Custom),
        ] {
            if self.swatch(name).is_active() != (s.colour == colour) {
                wrong.push(name.to_owned());
            }
        }
        let selected = |label: &str| self.control::<gtk::DropDown>(label).selected();
        if selected("Font") != 0 || s.face.is_some() {
            wrong.push("font".to_owned());
        }
        if selected("Weight") as usize != s.weight {
            wrong.push("weight".to_owned());
        }
        let margin = match s.margin {
            Margin::Narrow => 0,
            Margin::Medium => 1,
            Margin::Wide => 2,
        };
        if selected("Margins") != margin {
            wrong.push("margins".to_owned());
        }
        let turn = match s.turn {
            Turn::Slide => 0,
            Turn::Fade => 1,
            Turn::None => 2,
        };
        if selected("Page turn") != turn {
            wrong.push("turn".to_owned());
        }
        for (label, on) in [
            ("Justified", s.justify),
            ("Hyphenation", s.hyphenate),
            ("Two columns", s.two_columns),
        ] {
            if self.switch(label).is_active() != on {
                wrong.push(label.to_owned());
            }
        }
        for (less, more, at, last) in [
            ("Smaller text", "Larger text", s.size, SIZES.len() - 1),
            ("Tighter lines", "Looser lines", s.line, LINES.len() - 1),
            (
                "Closer letters",
                "Wider letters",
                s.letter,
                LETTERS.len() - 1,
            ),
            ("Closer words", "Wider words", s.word, WORDS.len() - 1),
        ] {
            if self.button(less).is_sensitive() != (at > 0)
                || self.button(more).is_sensitive() != (at < last)
            {
                wrong.push(less.to_owned());
            }
        }
        let rgba = |label: &str| self.control::<gtk::ColorDialogButton>(label).rgba();
        let as_rgb = |c: gtk::gdk::RGBA| {
            let part = |v: f32| (v * 255.0).round() as u8;
            settings::Rgb(part(c.red()), part(c.green()), part(c.blue()))
        };
        if as_rgb(rgba("Your page colour")) != s.custom.0
            || as_rgb(rgba("Your text colour")) != s.custom.1
        {
            wrong.push("custom colours".to_owned());
        }
        if wrong.is_empty() {
            Ok(())
        } else {
            Err(format!("{wrong:?} do not show {s:?}"))
        }
    }

    fn level(&self) -> String {
        self.popover
            .child()
            .and_then(|c| c.downcast::<gtk::Stack>().ok())
            .and_then(|s| s.visible_child_name())
            .map(|n| n.to_string())
            .unwrap_or_default()
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        self.popover.popdown();
        self.window.destroy();
        pause(Duration::from_millis(100));
    }
}

/// The panel's controls always show the settings, and showing them never
/// moves the panel to another level: choosing a colour of one's own turns
/// "follows light and dark" off where it opens, and opening the panel on a
/// saved colour of one's own stays on the first level.
fn the_panel_shows_the_settings_and_showing_them_moves_nothing() -> Outcome {
    let panel = Panel::new(settings::Settings::default());
    // What the settings apply to is said on both levels.
    let scopes = panel
        .all()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Label>().ok())
        .filter(|l| l.text() == "For every book")
        .count();
    check!(scopes == 2, "the scope is said {scopes} times");
    check!(
        panel
            .switch("Page colour follows light and dark")
            .is_active(),
        "the defaults do not follow light and dark"
    );
    panel.swatch("A colour of your own").set_active(true);
    pause(Duration::from_millis(300));
    check!(panel.level() == "more", "+ opened {}", panel.level());
    check!(
        panel.settings.borrow().colour == settings::Colour::Custom,
        "+ chose {:?}",
        panel.settings.borrow().colour
    );
    check!(
        !panel
            .switch("Page colour follows light and dark")
            .is_active(),
        "after +, the panel says the page colour follows light and dark"
    );
    drop(panel);
    let saved = settings::Settings {
        colour: settings::Colour::Custom,
        ..settings::Settings::default()
    };
    let panel = Panel::new(saved);
    check!(
        panel.level() == "first",
        "opening the panel on a colour of one's own went to {}",
        panel.level()
    );
    check!(
        panel.swatch("A colour of your own").is_active(),
        "the saved colour of one's own is not shown"
    );
    Ok(())
}

/// Reset shows the defaults in every control, the steppers included, with
/// the second level open.
fn reset_shows_the_defaults_in_every_control() -> Outcome {
    let panel = Panel::new(settings::Settings::default());
    panel.button("More").emit_clicked();
    let tighter = panel.button("Tighter lines");
    while tighter.is_sensitive() {
        tighter.emit_clicked();
    }
    check!(
        panel.settings.borrow().line == 0,
        "line spacing is {}",
        panel.settings.borrow().line
    );
    panel.button("Reset").emit_clicked();
    pause(Duration::from_millis(100));
    check!(
        panel.settings.borrow().line == settings::Settings::default().line,
        "Reset left line spacing at {}",
        panel.settings.borrow().line
    );
    check!(
        tighter.is_sensitive(),
        "after Reset, Tighter lines is still disabled"
    );
    Ok(())
}

/// One change to each control, as the reader would make it, from the
/// defaults.
type Action = (&'static str, fn(&Panel), fn(&mut settings::Settings));

fn actions() -> Vec<Action> {
    use settings::{Colour, Margin, Rgb, Turn};
    fn swatch(panel: &Panel, name: &str) {
        panel.swatch(name).set_active(true);
    }
    fn colour(panel: &Panel, label: &str, rgb: (f32, f32, f32)) {
        panel
            .control::<gtk::ColorDialogButton>(label)
            .set_rgba(&gtk::gdk::RGBA::new(rgb.0, rgb.1, rgb.2, 1.0));
    }
    vec![
        (
            "White",
            |p| swatch(p, "White"),
            |s| s.colour = Colour::White,
        ),
        (
            "Paper",
            |p| swatch(p, "Paper"),
            |s| s.colour = Colour::Paper,
        ),
        ("Grey", |p| swatch(p, "Grey"), |s| s.colour = Colour::Grey),
        (
            "Black",
            |p| swatch(p, "Black"),
            |s| s.colour = Colour::Black,
        ),
        (
            "+",
            |p| swatch(p, "A colour of your own"),
            |s| s.colour = Colour::Custom,
        ),
        (
            "follow off",
            |p| {
                p.switch("Page colour follows light and dark")
                    .set_active(false)
            },
            |s| s.colour = Colour::Paper,
        ),
        (
            "Larger text",
            |p| p.button("Larger text").emit_clicked(),
            |s| s.size += 1,
        ),
        (
            "Smaller text",
            |p| p.button("Smaller text").emit_clicked(),
            |s| s.size -= 1,
        ),
        (
            "Weight",
            |p| p.control::<gtk::DropDown>("Weight").set_selected(3),
            |s| s.weight = 3,
        ),
        (
            "Tighter lines",
            |p| p.button("Tighter lines").emit_clicked(),
            |s| s.line -= 1,
        ),
        (
            "Looser lines",
            |p| p.button("Looser lines").emit_clicked(),
            |s| s.line += 1,
        ),
        (
            "Wider letters",
            |p| p.button("Wider letters").emit_clicked(),
            |s| s.letter += 1,
        ),
        (
            "Wider words",
            |p| p.button("Wider words").emit_clicked(),
            |s| s.word += 1,
        ),
        (
            "Margins",
            |p| p.control::<gtk::DropDown>("Margins").set_selected(2),
            |s| s.margin = Margin::Wide,
        ),
        (
            "Justified",
            |p| p.switch("Justified").set_active(true),
            |s| {
                s.justify = true;
                s.hyphenate = true;
            },
        ),
        (
            "Hyphenation",
            |p| p.switch("Hyphenation").set_active(true),
            |s| s.hyphenate = true,
        ),
        (
            "Two columns",
            |p| p.switch("Two columns").set_active(true),
            |s| s.two_columns = true,
        ),
        (
            "Page turn",
            |p| p.control::<gtk::DropDown>("Page turn").set_selected(1),
            |s| s.turn = Turn::Fade,
        ),
        (
            "Your page colour",
            |p| colour(p, "Your page colour", (0.2, 0.4, 0.6)),
            |s| {
                s.custom.0 = Rgb(51, 102, 153);
                s.colour = Colour::Custom;
            },
        ),
        (
            "Your text colour",
            |p| colour(p, "Your text colour", (1.0, 1.0, 0.8)),
            |s| {
                s.custom.1 = Rgb(255, 255, 204);
                s.colour = Colour::Custom;
            },
        ),
    ]
}

/// Changing any one control from the defaults changes exactly its setting,
/// and every control then shows the settings.
fn each_control_changes_only_its_setting() -> Outcome {
    for (name, act, expect) in actions() {
        let panel = Panel::new(settings::Settings::default());
        act(&panel);
        pause(Duration::from_millis(50));
        let mut expected = settings::Settings::default();
        expect(&mut expected);
        check!(
            *panel.settings.borrow() == expected,
            "{name} made {:?}, not {expected:?}",
            panel.settings.borrow()
        );
        panel
            .consistent()
            .map_err(|e| format!("after {name}: {e}"))?;
    }
    Ok(())
}

/// Reset from every control changed, with a colour of one's own and with
/// following light and dark turned back on after it, ends with the
/// defaults saved and shown.
fn reset_from_every_change_restores_the_defaults() -> Outcome {
    for follow in [false, true] {
        let panel = Panel::new(settings::Settings::default());
        panel.button("More").emit_clicked();
        for (name, act, _) in actions() {
            if ![
                "White",
                "Paper",
                "Grey",
                "Black",
                "+",
                "follow off",
                "Smaller text",
                "Looser lines",
            ]
            .contains(&name)
            {
                act(&panel);
            }
        }
        if follow {
            panel
                .switch("Page colour follows light and dark")
                .set_active(true);
        }
        check!(
            *panel.settings.borrow() != settings::Settings::default(),
            "nothing changed"
        );
        panel
            .consistent()
            .map_err(|e| format!("before Reset: {e}"))?;
        panel.button("Reset").emit_clicked();
        pause(Duration::from_millis(200));
        check!(
            *panel.settings.borrow() == settings::Settings::default()
                && panel.saved.borrow().as_ref() == Some(&settings::Settings::default()),
            "follow {follow}: Reset left {:?}, saved {:?}",
            panel.settings.borrow(),
            panel.saved.borrow()
        );
        panel
            .consistent()
            .map_err(|e| format!("follow {follow}, after Reset: {e}"))?;
    }
    Ok(())
}

/// A saved font the system no longer has is the default everywhere: the
/// panel shows Default, and the next change saves no font.
fn a_saved_font_the_system_lacks_is_the_default() -> Outcome {
    let saved = settings::Settings {
        face: Some("Gone Serif".into()),
        ..settings::Settings::default()
    };
    // The panel is offered no faces, as on a system without that one.
    let panel = Panel::new(saved);
    check!(
        panel.control::<gtk::DropDown>("Font").selected() == 0,
        "the panel does not show Default"
    );
    panel.button("Larger text").emit_clicked();
    pause(Duration::from_millis(50));
    let written = panel.saved.borrow().clone();
    check!(
        written
            .as_ref()
            .is_some_and(|s| s.face.is_none() && s.size == 3),
        "the next change saved {written:?}"
    );
    panel.consistent()?;
    Ok(())
}

/// The panel is freed with the window that holds it: nothing in it holds
/// itself. The popover, the stack of levels, a stepper's button and the
/// switch whose handler shows the settings again are each watched.
fn the_panel_is_freed_with_its_window() -> Outcome {
    let watched: Vec<(&str, glib::WeakRef<gtk::Widget>)> = {
        let panel = Panel::new(settings::Settings::default());
        // Use the controls that hold others, so their handlers have run.
        panel.button("More").emit_clicked();
        panel.button("Back").emit_clicked();
        panel.swatch("Grey").set_active(true);
        panel
            .switch("Page colour follows light and dark")
            .set_active(true);
        panel.button("Larger text").emit_clicked();
        let stack = panel.popover.child().unwrap();
        vec![
            (
                "popover",
                panel.popover.upcast_ref::<gtk::Widget>().downgrade(),
            ),
            ("stack", stack.downgrade()),
            (
                "Larger text",
                panel
                    .button("Larger text")
                    .upcast::<gtk::Widget>()
                    .downgrade(),
            ),
            (
                "follow switch",
                panel
                    .switch("Page colour follows light and dark")
                    .upcast::<gtk::Widget>()
                    .downgrade(),
            ),
            (
                "Grey",
                panel.swatch("Grey").upcast::<gtk::Widget>().downgrade(),
            ),
        ]
    };
    spin(
        || watched.iter().all(|(_, w)| w.upgrade().is_none()),
        Duration::from_secs(5),
    );
    let kept: Vec<&str> = watched
        .iter()
        .filter(|(_, w)| w.upgrade().is_some())
        .map(|(name, _)| *name)
        .collect();
    check!(kept.is_empty(), "outlived the window: {kept:?}");
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

/// The reader's place-keeping fed by a real view, as the reader's window
/// feeds it: every load asked for, and every event in the order it came.
struct Shell {
    keeper: place::Keeper,
    fed: usize,
    /// Every place the keeper said to show.
    shown: Vec<place::Place>,
    /// Everything the keeper was told, in order.
    told: Vec<place::Input>,
}

impl Shell {
    fn new() -> Shell {
        Shell {
            keeper: place::Keeper::default(),
            fed: 0,
            shown: Vec::new(),
            told: Vec::new(),
        }
    }

    fn load(&mut self, stage: &Stage, chapter: u32, landing: Landing) -> u32 {
        let number = stage.view.load(chapter, landing);
        self.step(place::Input::Asked { number });
        number
    }

    fn step(&mut self, input: place::Input) {
        self.told.push(input);
        for effect in self.keeper.step(input) {
            if let place::Effect::Show(place) = effect {
                self.shown.push(place);
            }
        }
    }

    /// Give the keeper every event that has arrived since the last call.
    fn take(&mut self, stage: &Stage) {
        let events: Vec<Event> = stage.events.borrow()[self.fed..].to_vec();
        for event in &events {
            self.feed(event);
        }
    }

    fn feed(&mut self, event: &Event) {
        self.fed += 1;
        let record = |load: &view::Load| place::Load {
            number: load.number,
            chapter: load.chapter,
            length: load.length,
        };
        match event {
            Event::Ready { load } => self.step(place::Input::Ready { load: record(load) }),
            Event::Place {
                load,
                offset,
                page,
                pages,
                count,
                moved,
            } => self.step(place::Input::Report(place::Report {
                load: record(load),
                offset: *offset,
                page: *page,
                pages: *pages,
                count: *count,
                moved: *moved,
            })),
            Event::Failed(_) => self.step(place::Input::Failed),
            _ => {}
        }
    }

    /// Wait until load `number` has told a place, and take what arrived.
    fn landed(&mut self, stage: &Stage, number: u32) -> Outcome {
        stage.wait(
            "a load's place",
            |e| matches!(e, Event::Place { load, .. } if load.number == number),
        )?;
        self.take(stage);
        Ok(())
    }
}

/// Have the page say it is at a place, as load `number`.
fn say(stage: &Stage, number: u32, offset: u32, page: u32, count: u32, moved: bool) -> Outcome {
    stage.app(&format!(
        "window.webkit.messageHandlers.reader.postMessage({{kind: 'place', load: {number}, offset: {offset}, page: {page}, pages: 99, move: {count}, moved: {moved}}}); return 1"
    ))?;
    stage.wait("the place the page said", |e| {
        matches!(e, Event::Place { load, count: c, moved: m, .. } if load.number == number && *c == count && *m == moved)
    })?;
    Ok(())
}

/// Turn in A, choose B, which cannot be read, choose C, and only then does
/// a move made in A answer. The answer is told with A's own record, two
/// loads later. It is the place held until C lands, and it is never shown.
fn an_answer_two_chapters_late_is_held_and_not_shown() -> Outcome {
    let source = Rc::new(Chapters {
        markup: HashMap::from([(0, long_chapter()), (2, "<p>Shorter.</p>".to_owned())]),
        ..Chapters::default()
    });
    let stage = Stage::new(source, |_| {})?;
    let mut shell = Shell::new();
    let a = shell.load(&stage, 0, Landing::Start);
    shell.landed(&stage, a)?;
    // A real turn, answered after B was asked for.
    stage.view.turn(1, false, |_| {});
    shell.load(&stage, 1, Landing::Start);
    stage.wait("the turn's place", |e| {
        matches!(e, Event::Place { load, page: 1, count: 2, moved: true, .. } if load.number == a && load.chapter == 0)
    })?;
    stage.wait("B failing", |e| matches!(e, Event::Failed(_)))?;
    shell.take(&stage);
    let turn = shell.keeper.held();
    check!(
        turn.map(|p| (p.chapter, p.page)) == Some((0, 1)),
        "after the turn {turn:?} is held"
    );
    check!(
        shell.keeper.displayed().is_none(),
        "{:?} is displayed while nothing shows",
        shell.keeper.displayed()
    );
    // C is chosen, and then another move A made is answered: two loads late.
    let c = shell.load(&stage, 2, Landing::Start);
    say(&stage, a, 4321, 3, 3, true)?;
    shell.landed(&stage, c)?;
    // Go through what the keeper was told once more, one step at a time,
    // to see the place held after each.
    let mut again = place::Keeper::default();
    let mut moves: Vec<((u32, u32), (u32, u32))> = Vec::new();
    let mut asked = None;
    let mut held_the_late_answer = false;
    for input in &shell.told {
        match input {
            place::Input::Asked { number } => asked = Some(*number),
            place::Input::Report(r) if r.moved => {
                moves.push(((r.load.number, r.count), (r.load.chapter, r.offset)));
            }
            _ => {}
        }
        if let place::Input::Report(r) = input
            && r.load.number == a
            && r.count == 3
        {
            check!(
                r.load.chapter == 0 && r.load.length > 4321 && asked == Some(c),
                "the late answer came with the record {:?} while load {asked:?} was showing",
                r.load
            );
        }
        again.step(*input);
        let held = again.held().map(|p| (p.chapter, p.offset));
        let newest = moves.iter().max_by_key(|(order, _)| *order).map(|m| m.1);
        check!(
            held == newest,
            "after {input:?}, {held:?} is held and the newest move is {newest:?}"
        );
        held_the_late_answer |= held == Some((0, 4321));
        check!(
            again
                .displayed()
                .is_none_or(|(_, load)| Some(load) == asked),
            "after {input:?}, {:?} is displayed while load {asked:?} shows",
            again.displayed()
        );
    }
    check!(
        held_the_late_answer,
        "the late answer was never the place held: {:?}",
        shell.told
    );
    check!(
        shell.keeper.held().map(|p| p.chapter) == Some(2),
        "C landed last, and {:?} is held",
        shell.keeper.held()
    );
    check!(
        shell.keeper.displayed().map(|(p, load)| (p.chapter, load)) == Some((2, c)),
        "{:?} is displayed",
        shell.keeper.displayed()
    );
    check!(
        !shell.shown.iter().any(|p| p.offset == 4321),
        "the late answer was shown: {:?}",
        shell.shown
    );
    Ok(())
}

/// A chapter loaded a second time shows only what its second load says. A
/// place from the first load, of the same chapter number, is told with the
/// first load's record and is neither shown nor held.
fn a_chapter_loaded_again_shows_only_its_new_load() -> Outcome {
    let stage = Stage::new(one(long_chapter()), |_| {})?;
    let mut shell = Shell::new();
    let first = shell.load(&stage, 0, Landing::Start);
    shell.landed(&stage, first)?;
    let second = shell.load(&stage, 0, Landing::Offset(9000));
    shell.landed(&stage, second)?;
    let Some((landing, _)) = shell.keeper.displayed() else {
        return Err("nothing is displayed after the second load landed".into());
    };
    check!(landing.offset > 0, "the second load landed at {landing:?}");
    // A late move and a late reflow from the first load.
    say(&stage, first, 77, 1, 40, true)?;
    say(&stage, first, 77, 2, 40, false)?;
    shell.take(&stage);
    check!(
        shell.keeper.displayed() == Some((landing, second)),
        "the first load's place is displayed: {:?}",
        shell.keeper.displayed()
    );
    check!(
        shell.keeper.held() == Some(landing),
        "the first load's place is held: {:?}",
        shell.keeper.held()
    );
    // The second load still moves what is shown and held.
    check!(stage.turn(-1)?, "the second load did not turn");
    shell.take(&stage);
    let now = shell.keeper.displayed().map(|(p, load)| (p.page, load));
    check!(
        now == Some((landing.page - 1, second)),
        "after a turn {now:?} is displayed, from page {}",
        landing.page
    );
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
        // Under load a record has appeared as late as 19 seconds after
        // its crash.
        std::thread::sleep(Duration::from_secs(30));
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
    (
        "each_script_layer_blocks_alone",
        each_script_layer_blocks_alone,
    ),
    (
        "a_token_serves_only_its_own_view",
        a_token_serves_only_its_own_view,
    ),
    (
        "a_chapter_that_cannot_be_read_fails_at_once",
        a_chapter_that_cannot_be_read_fails_at_once,
    ),
    (
        "pages_without_text_keep_their_place",
        pages_without_text_keep_their_place,
    ),
    ("text_settings_reach_the_page", text_settings_reach_the_page),
    ("two_columns_when_they_fit", two_columns_when_they_fit),
    (
        "an_animated_turn_answers_with_the_place_it_leads_to",
        an_animated_turn_answers_with_the_place_it_leads_to,
    ),
    (
        "two_quick_turns_land_two_pages_on",
        two_quick_turns_land_two_pages_on,
    ),
    (
        "a_turn_after_a_reflow_counts_from_the_held_place",
        a_turn_after_a_reflow_counts_from_the_held_place,
    ),
    (
        "the_panel_shows_the_settings_and_showing_them_moves_nothing",
        the_panel_shows_the_settings_and_showing_them_moves_nothing,
    ),
    (
        "reset_shows_the_defaults_in_every_control",
        reset_shows_the_defaults_in_every_control,
    ),
    (
        "the_panel_is_freed_with_its_window",
        the_panel_is_freed_with_its_window,
    ),
    (
        "each_control_changes_only_its_setting",
        each_control_changes_only_its_setting,
    ),
    (
        "reset_from_every_change_restores_the_defaults",
        reset_from_every_change_restores_the_defaults,
    ),
    (
        "a_saved_font_the_system_lacks_is_the_default",
        a_saved_font_the_system_lacks_is_the_default,
    ),
    ("less_motion_turns_at_once", less_motion_turns_at_once),
    (
        "an_animation_ended_by_a_reflow_leaves_the_page_at_its_place",
        an_animation_ended_by_a_reflow_leaves_the_page_at_its_place,
    ),
    (
        "a_move_carries_its_place_and_a_reflow_does_not_move_it",
        a_move_carries_its_place_and_a_reflow_does_not_move_it,
    ),
    (
        "a_turn_answered_after_a_chapter_change_is_still_told",
        a_turn_answered_after_a_chapter_change_is_still_told,
    ),
    (
        "an_answer_two_chapters_late_is_held_and_not_shown",
        an_answer_two_chapters_late_is_held_and_not_shown,
    ),
    (
        "a_chapter_loaded_again_shows_only_its_new_load",
        a_chapter_loaded_again_shows_only_its_new_load,
    ),
    (
        "a_late_error_does_not_fail_a_newer_load",
        a_late_error_does_not_fail_a_newer_load,
    ),
    (
        "a_style_set_during_a_load_applies",
        a_style_set_during_a_load_applies,
    ),
];

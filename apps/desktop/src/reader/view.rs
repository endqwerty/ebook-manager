//! The web view that draws a chapter, and everything that keeps book content
//! inside it. See docs/design/epub-reading.md, "The trust boundary".
//!
//! This file uses nothing from the rest of the app, so the view tests can
//! include it directly.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use webkit6::prelude::*;
use webkit6::{gio, glib, javascriptcore, soup};

const SCHEME: &str = "ebook";
const WORLD: &str = "ebook-reader";
const HANDLER: &str = "reader";
const STYLE: &str = include_str!("reader.css");
const SCRIPT: &str = include_str!("reader.js");
const POLICY: &str = "default-src 'none'; img-src ebook:; style-src ebook: 'unsafe-inline'; form-action 'none'; base-uri 'none'";
/// Blocks every load whose address does not start with `ebook:`.
const FILTER: &str = r#"[{"trigger":{"url-filter":".*"},"action":{"type":"block"}},{"trigger":{"url-filter":"^ebook:"},"action":{"type":"ignore-previous-rules"}}]"#;
const FILTER_NAME: &str = "ebook-only-1";
/// A port nothing should listen on, so a request that passed every other
/// layer still goes nowhere.
const DEAD_PROXY: &str = "http://127.0.0.1:9";
const MAX_ADDRESS: usize = 2048;
const MAX_ANCHOR: usize = 256;

/// The layers that keep book content in. Everything is on outside tests.
#[derive(Clone, Copy, Debug)]
pub struct Protect {
    /// The engine removes script elements and handler attributes.
    pub no_markup_scripts: bool,
    /// Only the chapter load the shell asked for may navigate.
    pub navigation: bool,
    pub content_policy: bool,
    pub filter: bool,
    pub proxy: bool,
}

impl Protect {
    pub const ALL: Protect = Protect {
        no_markup_scripts: true,
        navigation: true,
        content_policy: true,
        filter: true,
        proxy: true,
    };
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Where the compiled content filter is kept.
    pub filter_dir: PathBuf,
    /// WebKit ends the web process when it holds more than this.
    pub memory_mib: u32,
    /// How long a chapter load or a call into the script may take.
    pub watchdog: Duration,
    pub protect: Protect,
}

impl Config {
    pub fn new(filter_dir: PathBuf) -> Config {
        Config {
            filter_dir,
            memory_mib: 1024,
            watchdog: Duration::from_secs(15),
            protect: Protect::ALL,
        }
    }
}

/// A chapter for the view: the children of `body`, already sanitised.
pub struct Document {
    pub markup: String,
    pub language: Option<String>,
    pub right_to_left: bool,
}

pub struct Picture {
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

/// Where chapters and pictures come from. Both answer later, on the main
/// thread, so the work can run elsewhere.
pub trait Source {
    fn chapter(&self, index: u32, done: Box<dyn FnOnce(Result<Document, String>)>);
    fn picture(&self, id: u32, done: Box<dyn FnOnce(Option<Picture>)>);
}

/// Where a chapter opens.
#[derive(Clone, Debug)]
pub enum Landing {
    /// Its first page, whatever is on it.
    Start,
    Offset(u32),
    Anchor(String),
    /// Its last page.
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Back,
    Middle,
    Forward,
}

/// What the view tells the shell. Everything here came from the page and
/// has been checked for kind and range, and for nothing else.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A load is laid out and about to show its landing place.
    Ready {
        load: Load,
    },
    /// Where a page is, with the record of the load that said so. That load
    /// may not be the one showing: a page can answer after the next chapter
    /// was asked for, or the one after that. `moved` is false for a reflow,
    /// which changes the page numbers and not the place: its offset is the
    /// one the last move reported. `count` is the page's count of its moves
    /// in that load, so the load's number and then `count` put every move
    /// the view has made in the order it was made.
    Place {
        load: Load,
        offset: u32,
        page: u32,
        pages: u32,
        count: u32,
        moved: bool,
    },
    /// A link to a place in the book was chosen.
    Jump {
        chapter: u32,
        anchor: Option<String>,
    },
    /// A link out of the book was chosen: `http`, `https` or `mailto`.
    /// Nothing has been opened.
    External(String),
    Tap(Zone),
    /// The chapter cannot be shown. The next load starts afresh.
    Failed(String),
}

/// One chapter load that became ready: its number, which grows with every
/// `View::load`, the chapter it showed and the length of that chapter's
/// text. Every place the view reports carries the record of its own load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Load {
    pub number: u32,
    pub chapter: u32,
    pub length: u32,
}

/// The load the view was asked for last.
struct Asked {
    number: u32,
    chapter: u32,
    landing: Landing,
    /// Its record, once the page has said it is ready.
    ready: Option<Load>,
}

struct Inner {
    view: webkit6::WebView,
    token: String,
    config: Config,
    load: RefCell<Option<Asked>>,
    /// The record of every load that became ready, by its number. A place
    /// is taken for the load whose number it carries, however many chapters
    /// have been asked for since. One small record per chapter the reader
    /// opened, so it is never trimmed.
    records: RefCell<std::collections::HashMap<u32, Load>>,
    counter: Cell<u32>,
    /// The one address `decide-policy` will let navigate, until it does.
    allowed: RefCell<Option<String>>,
    /// The custom properties every chapter opens with.
    style: RefCell<Vec<(String, String)>>,
    source: Rc<dyn Source>,
    /// Ends a chapter load that never becomes ready.
    watchdog: Rc<RefCell<Option<glib::SourceId>>>,
    on_event: Box<dyn Fn(Event)>,
}

#[derive(Clone)]
pub struct View(Rc<Inner>);

fn random_token() -> Result<String, String> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("no random source: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn escape_attribute(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '"' => "&quot;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

/// A custom property name the page accepts: `--` and lower-case letters.
fn property_name(name: &str) -> bool {
    name.strip_prefix("--").is_some_and(|rest| {
        !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
    })
}

/// A custom property value with nothing that could end the declaration.
fn property_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " #.,%()-'".contains(c))
}

/// The fixed document around a chapter's markup. `style` is the app's own
/// custom properties, so a chapter opens in the reader's colours and size.
pub fn wrap(document: &Document, style: &[(String, String)]) -> String {
    let style: String = style
        .iter()
        .filter(|(name, value)| property_name(name) && property_value(value))
        .map(|(name, value)| format!("{name}:{value};"))
        .collect();
    let language = document
        .language
        .as_deref()
        .map(|l| format!(r#" lang="{}""#, escape_attribute(l)))
        .unwrap_or_default();
    let direction = if document.right_to_left { "rtl" } else { "ltr" };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"{language} dir=\"{direction}\" style=\"{}\"><head><title>Chapter</title></head><body>{}</body></html>",
        escape_attribute(&style),
        document.markup
    )
}

/// `/c/<n>` or `/r/<n>` with a decimal number and nothing else.
fn numbered(path: &str, prefix: &str) -> Option<u32> {
    let digits = path.strip_prefix(prefix)?;
    if digits.is_empty() || digits.len() > 10 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The chapter and anchor of a link the sanitiser wrote: `/c/<n>#<anchor>`.
pub fn parse_jump(href: &str) -> Option<(u32, Option<String>)> {
    if href.len() > MAX_ADDRESS {
        return None;
    }
    let (path, fragment) = match href.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (href, None),
    };
    let chapter = numbered(path, "/c/")?;
    let anchor = match fragment {
        Some(f) => {
            let decoded = glib::Uri::unescape_string(f, None)?.to_string();
            if decoded.chars().count() > MAX_ANCHOR || decoded.chars().any(char::is_control) {
                return None;
            }
            Some(decoded).filter(|a| !a.is_empty())
        }
        None => None,
    };
    Some((chapter, anchor))
}

/// An address the reader may be offered to open outside the book.
pub fn external_address(address: &str) -> Option<String> {
    let lower = address.to_ascii_lowercase();
    let allowed = ["http://", "https://", "mailto:"]
        .iter()
        .any(|p| lower.starts_with(p));
    let tidy = !address.chars().any(|c| c.is_control() || c.is_whitespace());
    (allowed && tidy && address.len() <= MAX_ADDRESS).then(|| address.to_owned())
}

fn respond(
    request: &webkit6::URISchemeRequest,
    status: u32,
    media_type: &str,
    bytes: Vec<u8>,
    policy: bool,
) {
    let length = bytes.len() as i64;
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
    let response = webkit6::URISchemeResponse::new(&stream, length);
    response.set_content_type(media_type);
    response.set_status(status, None);
    let headers = soup::MessageHeaders::new(soup::MessageHeadersType::Response);
    headers.append("X-Content-Type-Options", "nosniff");
    headers.append("Cache-Control", "no-store");
    if policy {
        headers.append("Content-Security-Policy", POLICY);
    }
    response.set_http_headers(headers);
    request.finish_with_response(&response);
}

impl View {
    /// Build a view. `done` gets it once the content filter is in place, or
    /// the reason no book can be opened.
    pub fn open(
        config: Config,
        source: Rc<dyn Source>,
        on_event: impl Fn(Event) + 'static,
        done: impl FnOnce(Result<View, String>) + 'static,
    ) {
        let manager = webkit6::UserContentManager::new();
        if !config.protect.filter {
            done(View::build(config, source, manager, Box::new(on_event)));
            return;
        }
        // Compiled once for the process. Saving it again would rewrite the
        // file under a view that is using it.
        thread_local! {
            static COMPILED: RefCell<Option<webkit6::UserContentFilter>> = const { RefCell::new(None) };
        }
        if let Some(filter) = COMPILED.with(|c| c.borrow().clone()) {
            manager.add_filter(&filter);
            done(View::build(config, source, manager, Box::new(on_event)));
            return;
        }
        let Some(dir) = config.filter_dir.to_str().map(str::to_owned) else {
            done(Err("the content filter folder has no usable name".into()));
            return;
        };
        let store = webkit6::UserContentFilterStore::new(&dir);
        store.save(
            FILTER_NAME,
            &glib::Bytes::from_static(FILTER.as_bytes()),
            gio::Cancellable::NONE,
            move |saved| {
                done(match saved {
                    Ok(filter) => {
                        COMPILED.with(|c| c.replace(Some(filter.clone())));
                        manager.add_filter(&filter);
                        View::build(config, source, manager, Box::new(on_event))
                    }
                    Err(e) => Err(format!("the content filter could not be made: {e}")),
                })
            },
        );
    }

    fn build(
        config: Config,
        source: Rc<dyn Source>,
        manager: webkit6::UserContentManager,
        on_event: Box<dyn Fn(Event)>,
    ) -> Result<View, String> {
        let token = random_token()?;
        let protect = config.protect;

        let context = shared_context(config.memory_mib);

        let session = webkit6::NetworkSession::new_ephemeral();
        if protect.proxy {
            let proxy = webkit6::NetworkProxySettings::new(Some(DEAD_PROXY), &[]);
            session.set_proxy_settings(webkit6::NetworkProxyMode::Custom, Some(&proxy));
        }
        session.connect_download_started(|_, download| download.cancel());

        manager.add_style_sheet(&webkit6::UserStyleSheet::new(
            STYLE,
            webkit6::UserContentInjectedFrames::TopFrame,
            webkit6::UserStyleLevel::User,
            &[],
            &[],
        ));
        manager.add_script(&webkit6::UserScript::for_world(
            SCRIPT,
            webkit6::UserContentInjectedFrames::TopFrame,
            webkit6::UserScriptInjectionTime::End,
            WORLD,
            &[],
            &[],
        ));
        if !manager.register_script_message_handler(HANDLER, Some(WORLD)) {
            return Err("the reader's message handler could not be registered".into());
        }

        let settings = webkit6::Settings::new();
        settings.set_enable_javascript(true);
        settings.set_enable_javascript_markup(!protect.no_markup_scripts);
        settings.set_enable_webgl(false);
        settings.set_enable_webrtc(false);
        settings.set_enable_media(false);
        settings.set_enable_media_stream(false);
        settings.set_enable_mediasource(false);
        settings.set_enable_encrypted_media(false);
        settings.set_enable_webaudio(false);
        settings.set_enable_html5_local_storage(false);
        settings.set_enable_html5_database(false);
        settings.set_enable_page_cache(false);
        settings.set_enable_dns_prefetching(false);
        settings.set_allow_file_access_from_file_urls(false);
        settings.set_allow_universal_access_from_file_urls(false);
        settings.set_javascript_can_open_windows_automatically(false);
        settings.set_javascript_can_access_clipboard(false);
        settings.set_enable_back_forward_navigation_gestures(false);
        settings.set_enable_developer_extras(cfg!(debug_assertions));

        let view = webkit6::WebView::builder()
            .web_context(&context)
            .network_session(&session)
            .user_content_manager(&manager)
            .settings(&settings)
            .hexpand(true)
            .vexpand(true)
            .build();

        let inner = Rc::new(Inner {
            view: view.clone(),
            token,
            config,
            load: RefCell::new(None),
            records: RefCell::default(),
            counter: Cell::new(0),
            allowed: RefCell::new(None),
            style: RefCell::new(Vec::new()),
            source,
            watchdog: Rc::new(RefCell::new(None)),
            on_event,
        });
        // From here the scheme serves this view its book, and no other view.
        BOOKS.with(|b| {
            b.borrow_mut()
                .insert(inner.token.clone(), Rc::downgrade(&inner))
        });

        {
            let weak = Rc::downgrade(&inner);
            manager.connect_script_message_received(Some(HANDLER), move |_, value| {
                if let Some(inner) = weak.upgrade() {
                    View(inner).message(value);
                }
            });
        }
        {
            let weak = Rc::downgrade(&inner);
            view.connect_decide_policy(move |_, decision, kind| {
                let Some(inner) = weak.upgrade() else {
                    decision.ignore();
                    return true;
                };
                if !inner.config.protect.navigation {
                    return false;
                }
                let allow = match kind {
                    webkit6::PolicyDecisionType::NavigationAction => decision
                        .downcast_ref::<webkit6::NavigationPolicyDecision>()
                        .and_then(|d| d.navigation_action())
                        .and_then(|action| action.request())
                        .and_then(|r| r.uri())
                        .is_some_and(|uri| {
                            // Spent on use: nothing navigates twice on one permission.
                            let asked = inner.allowed.borrow().as_deref() == Some(uri.as_str());
                            if asked {
                                inner.allowed.replace(None);
                            }
                            asked
                        }),
                    webkit6::PolicyDecisionType::Response => decision
                        .downcast_ref::<webkit6::ResponsePolicyDecision>()
                        .and_then(|d| d.request())
                        .and_then(|r| r.uri())
                        .is_some_and(|uri| uri.starts_with("ebook:")),
                    _ => false,
                };
                if allow {
                    decision.use_();
                } else {
                    decision.ignore();
                }
                true
            });
        }
        view.connect_permission_request(|_, request| {
            request.deny();
            true
        });
        view.connect_create(|_, _| None);
        {
            let weak = Rc::downgrade(&inner);
            view.connect_web_process_terminated(move |_, reason| {
                if let Some(inner) = weak.upgrade() {
                    let view = View(inner);
                    disarm(&view.0.watchdog);
                    view.0.load.replace(None);
                    let why = match reason {
                        webkit6::WebProcessTerminationReason::ExceededMemoryLimit => {
                            "This chapter needs more memory than the reader allows."
                        }
                        webkit6::WebProcessTerminationReason::TerminatedByApi => {
                            "This chapter took too long to display."
                        }
                        _ => "The part of the app that draws the page stopped.",
                    };
                    (view.0.on_event)(Event::Failed(why.into()));
                }
            });
        }
        Ok(View(inner))
    }

    /// End the view's web process. Call it before the view's window is
    /// destroyed, so WebKit never runs that process's own shutdown. See
    /// `Drop for Inner`.
    pub fn shut(&self) {
        disarm(&self.0.watchdog);
        self.0.load.replace(None);
        self.0.view.terminate_web_process();
    }

    pub fn widget(&self) -> &webkit6::WebView {
        &self.0.view
    }

    /// Show a chapter. `Event::Ready` follows, or `Event::Failed`. The
    /// answer is this load's number, which every event about it carries.
    pub fn load(&self, chapter: u32, landing: Landing) -> u32 {
        let number = self.0.counter.get().wrapping_add(1);
        self.0.counter.set(number);
        self.0.load.replace(Some(Asked {
            number,
            chapter,
            landing,
            ready: None,
        }));
        let address = format!("{SCHEME}://{}/c/{chapter}?l={number}", self.0.token);
        self.0.allowed.replace(Some(address.clone()));
        self.arm(&self.0.watchdog);
        self.0.view.load_uri(&address);
        number
    }

    /// Turn `by` pages. `done` hears whether the page moved; at either edge
    /// of the chapter it did not. A turn that moved has sent `Event::Place`
    /// with the new place before `done` is called: the page's answer to the
    /// turn carries it, so the two cannot arrive apart.
    pub fn turn(&self, by: i32, animate: bool, done: impl FnOnce(bool) + 'static) {
        // The answer belongs to the load that is turned, whatever has been
        // asked for by the time it arrives.
        let Some(load) = self.0.load.borrow().as_ref().and_then(|l| l.ready) else {
            return done(false);
        };
        let arguments = glib::VariantDict::new(None);
        arguments.insert("by", by);
        // A boolean is not accepted as an argument, so it goes as a number.
        arguments.insert("smooth", i32::from(animate));
        let weak = Rc::downgrade(&self.0);
        self.call(
            "return __reader.turn(by, smooth === 1)",
            arguments,
            move |value| {
                // The answer is "" at a chapter's edge, or the new place as
                // "offset,page,pages,move". It is checked like any message.
                let place = value.filter(|v| v.is_string()).and_then(|v| {
                    let text = v.to_str();
                    let mut parts = text.split(',').map(|p| p.parse::<u32>().ok());
                    let place = (
                        parts.next()??,
                        parts.next()??,
                        parts.next()??,
                        parts.next()??,
                    );
                    parts.next().is_none().then_some(place)
                });
                let moved = match (place, weak.upgrade()) {
                    (Some((offset, page, pages, count)), Some(inner)) => {
                        View(inner).place(load, offset, page, pages, count, true)
                    }
                    _ => false,
                };
                done(moved)
            },
        );
    }

    /// Tell the shell a place a page reported, with the record of the load
    /// it reported it for, if it is inside that load's chapter.
    fn place(
        &self,
        load: Load,
        offset: u32,
        page: u32,
        pages: u32,
        count: u32,
        moved: bool,
    ) -> bool {
        if pages == 0 || page >= pages || offset > load.length {
            return false;
        }
        (self.0.on_event)(Event::Place {
            load,
            offset,
            page,
            pages,
            count,
            moved,
        });
        true
    }

    /// Set CSS custom properties on the page, keeping the reading place.
    /// They also apply to every chapter opened afterwards. Names are `--`
    /// and lower-case letters; anything else is ignored.
    pub fn set_style(&self, properties: &[(&str, &str)]) {
        let all = glib::VariantDict::new(None);
        {
            let mut kept = self.0.style.borrow_mut();
            for (name, value) in properties {
                if !property_name(name) || !property_value(value) {
                    continue;
                }
                kept.retain(|(n, _)| n != name);
                kept.push(((*name).to_owned(), (*value).to_owned()));
                all.insert(name, value);
            }
        }
        // A chapter still loading takes them from its own document.
        if !self
            .0
            .load
            .borrow()
            .as_ref()
            .is_some_and(|l| l.ready.is_some())
        {
            return;
        }
        let arguments = glib::VariantDict::new(None);
        arguments.insert_value("properties", &all.end());
        self.call("return __reader.style(properties)", arguments, |_| {});
    }

    /// Run a function body in the app's script world and hand `done` its
    /// result, or `None` when it failed. Each call has its own time limit,
    /// so one call answering never excuses another that has not.
    pub fn call(
        &self,
        body: &str,
        arguments: glib::VariantDict,
        done: impl FnOnce(Option<javascriptcore::Value>) + 'static,
    ) {
        let limit = Rc::new(RefCell::new(None));
        self.arm(&limit);
        self.0.view.call_async_javascript_function(
            body,
            Some(&arguments.end()),
            Some(WORLD),
            None,
            gio::Cancellable::NONE,
            move |result| {
                disarm(&limit);
                done(result.ok())
            },
        );
    }

    /// End the web process if `slot` is not disarmed in time.
    fn arm(&self, slot: &Rc<RefCell<Option<glib::SourceId>>>) {
        disarm(slot);
        let weak = Rc::downgrade(&self.0);
        let fired = slot.clone();
        let timer = glib::timeout_add_local_once(self.0.config.watchdog, move || {
            // This timer has fired and is gone, so it is forgotten, not removed.
            fired.replace(None);
            if let Some(inner) = weak.upgrade() {
                // `web-process-terminated` reports it to the shell.
                inner.view.terminate_web_process();
            }
        });
        slot.replace(Some(timer));
    }

    /// The chapter cannot be shown. Say so now rather than at the time limit.
    fn fail(&self, why: String) {
        disarm(&self.0.watchdog);
        self.0.load.replace(None);
        (self.0.on_event)(Event::Failed(why));
    }

    /// One message from the page. It is untrusted: only a known kind with
    /// values in range does anything. A place is taken for the load whose
    /// number it carries, if that load became ready. Every other kind acts
    /// on what is showing, so it needs the number of the load asked for last.
    fn message(&self, value: &javascriptcore::Value) {
        if !value.is_object() {
            return;
        }
        let number = |name: &str| -> Option<u32> {
            let v = value.object_get_property(name)?;
            let n = v.to_double();
            (v.is_number()
                && n.is_finite()
                && n >= 0.0
                && n <= f64::from(u32::MAX)
                && n.fract() == 0.0)
                .then_some(n as u32)
        };
        let text = |name: &str| -> Option<String> {
            let v = value.object_get_property(name)?;
            if !v.is_string() {
                return None;
            }
            let s = v.to_str().to_string();
            (s.len() <= MAX_ADDRESS).then_some(s)
        };
        let Some(kind) = text("kind") else {
            return;
        };
        if kind == "place" {
            let moved = value
                .object_get_property("moved")
                .is_some_and(|m| m.is_boolean() && m.to_boolean());
            let load = number("load").and_then(|n| self.0.records.borrow().get(&n).copied());
            if let (Some(load), Some(offset), Some(page), Some(pages), Some(count)) = (
                load,
                number("offset"),
                number("page"),
                number("pages"),
                number("move"),
            ) {
                self.place(load, offset, page, pages, count, moved);
            }
            return;
        }
        let (chapter, current, ready) = match &*self.0.load.borrow() {
            Some(load) => (load.chapter, load.number, load.ready.is_some()),
            None => return,
        };
        if number("load") != Some(current) {
            return;
        }
        match kind.as_str() {
            "ready" if !ready => {
                disarm(&self.0.watchdog);
                let body = value
                    .object_get_property("body")
                    .is_some_and(|b| b.is_boolean() && b.to_boolean());
                let Some(length) = number("length").filter(|_| body) else {
                    self.0.load.replace(None);
                    (self.0.on_event)(Event::Failed("This chapter could not be displayed.".into()));
                    return;
                };
                let load = Load {
                    number: current,
                    chapter,
                    length,
                };
                let landing = match self.0.load.borrow_mut().as_mut() {
                    Some(asked) => {
                        asked.ready = Some(load);
                        asked.landing.clone()
                    }
                    None => return,
                };
                self.0.records.borrow_mut().insert(current, load);
                (self.0.on_event)(Event::Ready { load });
                let arguments = glib::VariantDict::new(None);
                // The document was written with the properties of the moment
                // it was asked for. Any set since then apply before landing.
                let properties = glib::VariantDict::new(None);
                for (name, value) in self.0.style.borrow().iter() {
                    properties.insert(name, value.as_str());
                }
                arguments.insert_value("properties", &properties.end());
                let body = match landing {
                    Landing::Start => "__reader.assign(properties); return __reader.go({page: 0})",
                    Landing::Offset(offset) => {
                        arguments.insert("offset", offset.min(length));
                        "__reader.assign(properties); return __reader.go({offset})"
                    }
                    Landing::Anchor(anchor) => {
                        arguments.insert("anchor", anchor.as_str());
                        "__reader.assign(properties); return __reader.go({anchor})"
                    }
                    Landing::End => "__reader.assign(properties); return __reader.go({end: true})",
                };
                self.call(body, arguments, |_| {});
            }
            "jump" if ready => {
                if let Some((chapter, anchor)) = text("href").as_deref().and_then(parse_jump) {
                    (self.0.on_event)(Event::Jump { chapter, anchor });
                }
            }
            "external" if ready => {
                if let Some(address) = text("address").as_deref().and_then(external_address) {
                    (self.0.on_event)(Event::External(address));
                }
            }
            "tap" if ready => {
                let zone = match text("zone").as_deref() {
                    Some("back") => Zone::Back,
                    Some("middle") => Zone::Middle,
                    Some("forward") => Zone::Forward,
                    _ => return,
                };
                (self.0.on_event)(Event::Tap(zone));
            }
            _ => {}
        }
    }
}

thread_local! {
    /// The view each token in an `ebook:` address belongs to.
    static BOOKS: RefCell<std::collections::HashMap<String, std::rc::Weak<Inner>>> = RefCell::new(Default::default());
    static CONTEXT: RefCell<Option<webkit6::WebContext>> = const { RefCell::new(None) };
}

/// One web context for every reader view in the process. A second context
/// with its own memory limit made WebKitGTK 2.54 abort a web process, so
/// the limit is the first view's and the scheme is registered once.
fn shared_context(memory_mib: u32) -> webkit6::WebContext {
    CONTEXT.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| {
                let mut pressure = webkit6::MemoryPressureSettings::new();
                pressure.set_memory_limit(memory_mib);
                pressure.set_conservative_threshold(0.4);
                pressure.set_strict_threshold(0.6);
                pressure.set_kill_threshold(1.0);
                pressure.set_poll_interval(1.0);
                let context = webkit6::WebContext::builder()
                    .memory_pressure_settings(&pressure)
                    .build();
                context.register_uri_scheme(SCHEME, serve);
                // A chapter may load its own scheme and no other, whatever
                // its markup and the layers above say.
                if let Some(security) = context.security_manager() {
                    security.register_uri_scheme_as_no_access(SCHEME);
                }
                context
            })
            .clone()
    })
}

fn disarm(slot: &Rc<RefCell<Option<glib::SourceId>>>) {
    if let Some(timer) = slot.take() {
        timer.remove();
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // End the web process now instead of letting it shut itself down.
        // WebKitGTK 2.54 sometimes aborts in that shutdown, in a timer's
        // destructor, and an abort is a crash the reader would see. A
        // running process is ended with a kill signal, which leaves no core
        // dump, and one still starting is cancelled. Nothing in it is needed.
        self.view.terminate_web_process();
        disarm(&self.watchdog);
        BOOKS.with(|b| b.borrow_mut().remove(&self.token));
    }
}

fn serve(request: &webkit6::URISchemeRequest) {
    let refuse =
        |request: &webkit6::URISchemeRequest| respond(request, 404, "text/plain", Vec::new(), true);
    let uri = request.uri().unwrap_or_default();
    let Ok(parsed) = glib::Uri::parse(&uri, glib::UriFlags::NONE) else {
        return refuse(request);
    };
    if parsed.scheme() != SCHEME {
        return refuse(request);
    }
    // A token names its book only to the view it was made for. Any other
    // host, and the right host from another view, is nothing.
    let Some(inner) = parsed
        .host()
        .and_then(|host| BOOKS.with(|b| b.borrow().get(host.as_str()).and_then(|w| w.upgrade())))
        .filter(|inner| request.web_view().as_ref() == Some(&inner.view))
    else {
        return refuse(request);
    };
    let policy = inner.config.protect.content_policy;
    let source = inner.source.clone();
    let path = parsed.path();
    // The load number the shell put in the chapter's address.
    let asked: Option<u32> = parsed
        .query()
        .and_then(|q| q.strip_prefix("l=").and_then(|n| n.parse().ok()));
    let request = request.clone();
    if let Some(chapter) = numbered(&path, "/c/") {
        source.chapter(
            chapter,
            Box::new(move |document| match document {
                Ok(document) => respond(
                    &request,
                    200,
                    "application/xhtml+xml",
                    wrap(&document, &inner.style.borrow()).into_bytes(),
                    policy,
                ),
                Err(why) => {
                    respond(&request, 404, "text/plain", Vec::new(), policy);
                    // Only the load that asked can fail. An answer for an
                    // earlier load of the same chapter is too late to matter.
                    let loading = inner
                        .load
                        .borrow()
                        .as_ref()
                        .is_some_and(|l| l.ready.is_none() && Some(l.number) == asked);
                    if loading {
                        View(inner).fail(why);
                    }
                }
            }),
        );
    } else if let Some(id) = numbered(&path, "/r/") {
        source.picture(
            id,
            Box::new(move |picture| match picture {
                Some(picture) => respond(&request, 200, picture.media_type, picture.bytes, policy),
                None => respond(&request, 404, "text/plain", Vec::new(), policy),
            }),
        );
    } else {
        refuse(&request)
    }
}

#[path = "../book.rs"]
mod book;
#[path = "../epub.rs"]
mod epub;
#[path = "../fatal.rs"]
mod fatal;
#[path = "../isolation.rs"]
mod isolation;
#[path = "../mobi.rs"]
mod mobi;
use ebook_convert::{Error, Metadata, STATUS_LIMIT};
use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
use std::io::{self, Write};

fn convert() -> Result<(Vec<String>, Vec<u8>), Error> {
    for (res, cap) in [
        (Resource::As, 768 << 20),
        (Resource::Cpu, 55),
        (Resource::Nproc, 1),
        (Resource::Core, 0),
        (Resource::Nofile, 32),
    ] {
        let old = getrlimit(res);
        let bound = [old.current, old.maximum]
            .into_iter()
            .flatten()
            .fold(cap, u64::min);
        setrlimit(
            res,
            Rlimit {
                current: Some(bound),
                maximum: Some(bound),
            },
        )
        .map_err(|_| Error::Sandbox("cannot restrict converter".into()))?;
    }
    isolation::verify().map_err(Error::Sandbox)?;
    let input = std::fs::read("/job/source")?;
    let parsed = mobi::decode(&input).map_err(|e| match e {
        book::DecodeError::Protected => Error::Protected,
        book::DecodeError::Unsupported(s) => Error::Unsupported(s),
        book::DecodeError::Invalid(s) => Error::Invalid(s),
        book::DecodeError::Limit => Error::Limit,
    })?;
    drop(input);
    let metadata: Metadata = serde_json::from_slice(&std::fs::read("/job/metadata")?)
        .map_err(|_| Error::Invalid("invalid metadata".into()))?;
    let mut cover = None;
    for (path, kind) in [
        ("/job/cover.png", book::ImageKind::Png),
        ("/job/cover.jpg", book::ImageKind::Jpeg),
    ] {
        if std::path::Path::new(path).exists() {
            cover = Some(book::Image {
                kind,
                bytes: std::fs::read(path)?,
            });
        }
    }
    epub::write(parsed, metadata, cover)
}
fn main() -> io::Result<()> {
    fatal::exit_on_fatal_signals();
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("fatal-probe") {
        std::process::exit(fatal::fatal_probe(args.next().as_deref()));
    }
    let (result, bytes) = match convert() {
        Ok((losses, bytes)) => (Ok(losses), bytes),
        Err(e) => (Err(e), Vec::new()),
    };
    let mut header = serde_json::to_vec(&result)?;
    if header.len() > STATUS_LIMIT {
        header = serde_json::to_vec(&Err::<Vec<String>, Error>(Error::Limit))?;
    }
    let mut out = io::stdout().lock();
    out.write_all(&(header.len() as u32).to_be_bytes())?;
    out.write_all(&header)?;
    out.write_all(&bytes)
}

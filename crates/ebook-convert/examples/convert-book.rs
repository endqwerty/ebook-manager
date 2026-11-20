//! Run the same job a catalogue caller would start, printing its report.
use ebook_convert::{start, Config, Metadata, Request};
use std::path::PathBuf;
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [source, destination, converter, reader] = args.as_slice() else {
        return Err("usage: convert-book SOURCE DESTINATION CONVERTER_HELPER READER_HELPER".into());
    };
    let request = Request {
        source: PathBuf::from(source),
        destination: PathBuf::from(destination),
        identity: "standalone".into(),
        metadata: Metadata::default(),
        cover: None,
    };
    let config = Config {
        helper: PathBuf::from(converter),
        reader: ebook_reader::ReaderConfig {
            helper_path: Some(PathBuf::from(reader)),
            ..Default::default()
        },
    };
    let result = start(request, config).wait().map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result.report).map_err(|e| e.to_string())?
    );
    Ok(())
}

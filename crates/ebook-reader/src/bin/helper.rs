//! Sandboxed helper. Started by the host library inside bubblewrap; refuses to
//! read a book when it finds itself outside a sandbox.
#![forbid(unsafe_code)]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(ebook_reader::engine::run::main(&args));
}

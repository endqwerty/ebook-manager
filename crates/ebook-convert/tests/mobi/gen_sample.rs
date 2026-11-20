//! Writes the checked-in sample:
//!   rustc --edition 2021 tests/mobi/gen_sample.rs -o <dir>/gen-sample && <dir>/gen-sample tests/mobi/sample.mobi
#[path = "fixture.rs"]
mod fixture;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: gen-sample <output.mobi>");
    std::fs::write(&path, fixture::MobiFixture::sample().build()).expect("write sample");
}

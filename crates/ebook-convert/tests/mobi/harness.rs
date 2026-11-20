//! Standalone test harness for the MOBI decoder until the crate is assembled:
//!   rustc --edition 2021 --test tests/mobi/harness.rs -o <dir>/mobi-harness && <dir>/mobi-harness
//! The real-sample trial is ignored by default; see tests/mobi/README.md.

#[path = "../../src/book.rs"]
mod book;
#[path = "fixture.rs"]
mod fixture;
#[path = "../../src/mobi.rs"]
mod mobi;

use book::{Block, DecodeError, ImageKind, ParsedBook};
use fixture::{palmdoc_compress, MobiFixture, GIF, JPEG, PNG, SAMPLE_FILLER};

fn decode(f: &MobiFixture) -> Result<ParsedBook, DecodeError> {
    mobi::decode(&f.build())
}

fn ok(f: &MobiFixture) -> ParsedBook {
    decode(f).expect("fixture decodes")
}

fn paragraphs(book: &ParsedBook) -> Vec<String> {
    book.sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .filter_map(|b| match b {
            Block::Paragraph(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn page(markup_body: &str) -> String {
    format!("<html><body>{markup_body}</body></html>")
}

fn invalid(result: Result<ParsedBook, DecodeError>) -> String {
    match result {
        Err(DecodeError::Invalid(reason)) => reason,
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn checked_in_sample_matches_generator() {
    let on_disk = include_bytes!("sample.mobi");
    assert!(
        on_disk[..] == MobiFixture::sample().build()[..],
        "tests/mobi/sample.mobi is stale; regenerate it with tests/mobi/gen_sample.rs"
    );
}

#[test]
fn sample_reading_order_text_images_and_cover() {
    let book = mobi::decode(include_bytes!("sample.mobi")).unwrap();
    assert_eq!(book.title, "The Lantern Keeper");
    assert_eq!(book.authors, ["Ada Marlow", "Ben Ortiz"]);
    assert_eq!(book.language.as_deref(), Some("en"));

    let titles: Vec<_> = book.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Chapter One: The Lamp",
            "Chapter Two: The Storm",
            "Chapter Three: Morning"
        ]
    );

    let one = &book.sections[0].blocks;
    assert!(matches!(&one[0], Block::Heading(t) if t == "Chapter One: The Lamp"));
    assert!(
        matches!(&one[1], Block::Paragraph(t) if t == "The lamp at Gull Point had burned for ninety years.")
    );
    assert!(matches!(&one[2], Block::Paragraph(t)
        if t == "Its keeper, Marta Ruiz & her cat \u{2014} kept the log in caf\u{e9} ink: \u{201c}All\u{a0}clear.\u{201d}"));
    assert!(matches!(one[3], Block::Image(0)));
    assert_eq!(one.len(), 4);

    let two = section_paragraphs(&book.sections[1]);
    assert_eq!(two[0], "Wind rose from the west.");
    assert_eq!(two[1], "Waves broke on the rocks.");
    for i in 1..=60 {
        assert_eq!(two[1 + i], format!("{i}. {SAMPLE_FILLER}"));
    }
    assert_eq!(
        &two[62..],
        ["See chapter one for the lamp.", "Night", "Ships"]
    );
    assert!(!book.sections[1]
        .blocks
        .iter()
        .any(|b| matches!(b, Block::Image(_))));

    let three = &book.sections[2].blocks;
    assert!(matches!(&three[0], Block::Heading(t) if t == "Chapter Three: Morning"));
    assert!(matches!(&three[1], Block::Paragraph(t) if t == "Gulls: 41"));
    assert!(matches!(three[2], Block::Image(1)));
    assert!(matches!(&three[3], Block::Paragraph(t) if t == "The end. \u{1F30A}"));

    assert_eq!(book.images.len(), 2);
    assert!(matches!(book.images[0].kind, ImageKind::Jpeg));
    assert_eq!(book.images[0].bytes, JPEG);
    assert!(matches!(book.images[1].kind, ImageKind::Png));
    assert_eq!(book.images[1].bytes, PNG);
    assert_eq!(book.cover, Some(1));

    let mut losses = book.losses.clone();
    losses.sort();
    assert_eq!(
        losses,
        [
            "GIF images were dropped; only JPEG and PNG images are kept.",
            "Heading levels were changed; every heading is written at the same level.",
            "Inline formatting such as bold, italic and font sizes was removed.",
            "Links were removed; their text was kept.",
            "Lists were flattened into paragraphs.",
            "Tables were flattened into paragraphs.",
            "The book's guide (such as its start position and contents entry) was not carried over.",
        ]
    );
}

fn section_paragraphs(section: &book::Section) -> Vec<String> {
    section
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn err(result: Result<ParsedBook, DecodeError>) -> DecodeError {
    match result {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    }
}

#[test]
fn sample_spans_several_text_records() {
    let f = MobiFixture::sample();
    assert!(f.encoded_text().len() > 2 * 4096);
}

fn straddling_text(prefix_len: usize) -> String {
    let mut body = String::from("<html><body><p>");
    let fill = prefix_len - body.len();
    body.push_str(&"a".repeat(fill));
    body.push_str("\u{e9}\u{2014}\u{1F30A}x</p>");
    body.push_str(&"<p>after</p>".repeat(400));
    body.push_str("</body></html>");
    body
}

#[test]
fn utf8_characters_split_across_records_survive_both_compressions() {
    for split in 4088..=4095 {
        for compression in [1, 2] {
            let markup = straddling_text(split);
            let f = MobiFixture {
                markup,
                compression,
                extra_flags: 0x3,
                ..Default::default()
            };
            let book = ok(&f);
            let first = &paragraphs(&book)[0];
            assert!(
                first.ends_with("a\u{e9}\u{2014}\u{1F30A}x"),
                "split {split} compression {compression}"
            );
            assert!(other_losses(&book).is_empty(), "{:?}", book.losses);
        }
    }
}

#[test]
fn records_without_trailing_entries() {
    let f = MobiFixture {
        markup: straddling_text(4095),
        extra_flags: 0,
        encoding: 65001,
        ..Default::default()
    };
    let book = ok(&f);
    assert!(paragraphs(&book)[0].ends_with("\u{e9}\u{2014}\u{1F30A}x"));
}

#[test]
fn cp1252_uncompressed_text_and_metadata() {
    let f = MobiFixture {
        encoding: 1252,
        compression: 1,
        extra_flags: 0,
        title: "Caf\u{e9} \u{2014} Notes".into(),
        authors: vec!["Ren\u{e9}e Dupr\u{e9}".into()],
        markup: page("<p>\u{201c}Caf\u{e9}\u{201d} costs \u{20ac}3 &#169;</p>"),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(book.title, "Caf\u{e9} \u{2014} Notes");
    assert_eq!(book.authors, ["Ren\u{e9}e Dupr\u{e9}"]);
    assert_eq!(
        paragraphs(&book),
        ["\u{201c}Caf\u{e9}\u{201d} costs \u{20ac}3 \u{a9}"]
    );
}

#[test]
fn language_falls_back_to_english_locale_then_none() {
    let f = MobiFixture {
        language: None,
        locale: 1033,
        ..Default::default()
    };
    assert_eq!(ok(&f).language.as_deref(), Some("en"));
    let f = MobiFixture {
        language: None,
        locale: 1036,
        ..Default::default()
    };
    assert_eq!(ok(&f).language, None);
    let f = MobiFixture {
        language: Some("fr".into()),
        locale: 1033,
        ..Default::default()
    };
    assert_eq!(ok(&f).language.as_deref(), Some("fr"));
}

#[test]
fn title_falls_back_to_database_name() {
    let f = MobiFixture {
        title: String::new(),
        pdb_name: "Short_Name".into(),
        ..Default::default()
    };
    assert_eq!(ok(&f).title, "Short_Name");
}

#[test]
fn file_without_exth_has_no_authors_or_cover() {
    let f = MobiFixture {
        authors: vec![],
        language: None,
        cover: None,
        images: vec![JPEG.to_vec()],
        ..Default::default()
    };
    let book = ok(&f);
    assert!(book.authors.is_empty());
    assert_eq!(book.cover, None);
    assert!(book.images.is_empty());
}

#[test]
fn cover_only_image_is_kept_and_gif_cover_is_reported() {
    let f = MobiFixture {
        images: vec![GIF.to_vec(), JPEG.to_vec()],
        cover: Some(1),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(book.cover, Some(0));
    assert!(matches!(book.images[0].kind, ImageKind::Jpeg));

    let f = MobiFixture {
        images: vec![GIF.to_vec()],
        cover: Some(0),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(book.cover, None);
    assert!(book.losses.iter().any(|l| l.contains("GIF")));
}

#[test]
fn missing_or_unindexed_images_are_reported() {
    let f = MobiFixture {
        images: vec![JPEG.to_vec()],
        markup: page("<p>a<img recindex=\"00009\">b<img src=\"x.jpg\">c<img recindex=\"0\"></p>"),
        cover: Some(40),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(paragraphs(&book), ["abc"]);
    assert!(book
        .losses
        .iter()
        .any(|l| l.contains("missing image records")));
    assert!(book.losses.iter().any(|l| l.contains("referenced by name")));
    assert_eq!(book.cover, None);
}

#[test]
fn same_image_referenced_twice_is_stored_once() {
    let f = MobiFixture {
        images: vec![PNG.to_vec()],
        markup: page("<img recindex=1><p>x</p><img recindex='1'>"),
        cover: Some(0),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(book.images.len(), 1);
    assert_eq!(book.cover, Some(0));
}

#[test]
fn sections_without_headings_get_numbered_titles() {
    let f = MobiFixture {
        markup: page("<p>one</p><mbp:pagebreak/><mbp:pagebreak/><p>two</p><h3>Late heading</h3>"),
        ..Default::default()
    };
    let book = ok(&f);
    let titles: Vec<_> = book.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Section 1", "Late heading"]);
}

#[test]
fn unknown_markup_comments_and_entities() {
    let f = MobiFixture {
        markup: page(
            "<p>A<!-- hidden <p>no</p> -->B &bogus; &#x41;&#65;&lt;tag&gt; <blink>C</blink></p>",
        ),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(paragraphs(&book), ["AB &bogus; AA<tag> C"]);
    assert!(book
        .losses
        .iter()
        .any(|l| l.contains("Unrecognised markup")));
    assert_eq!(unknown_reference_losses(&book), 1);
}

const UNKNOWN_REFERENCE: &str =
    "Unrecognised named character references such as &mdash; were kept as written.";

fn unknown_reference_losses(book: &ParsedBook) -> usize {
    book.losses
        .iter()
        .filter(|l| *l == UNKNOWN_REFERENCE)
        .count()
}

#[test]
fn unknown_named_references_are_kept_and_reported_once() {
    let f = MobiFixture {
        markup: page(
            "<p>a &mdash; b &eacute; c &copy; d &mdash;</p><mbp:pagebreak/><p>&eacute;t&eacute;</p>",
        ),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(
        paragraphs(&book),
        [
            "a &mdash; b &eacute; c &copy; d &mdash;",
            "&eacute;t&eacute;"
        ]
    );
    assert_eq!(unknown_reference_losses(&book), 1);
    assert_eq!(other_losses(&book).len(), 1, "{:?}", book.losses);
}

#[test]
fn ampersands_that_are_not_references_are_plain_text() {
    let long_name = "a".repeat(33);
    let f = MobiFixture {
        markup: page(&format!(
            "<p>AT&T &amp; R&D; & more &#; &#x; &{long_name}; &amp;amp;</p>"
        )),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(
        paragraphs(&book),
        [format!("AT&T & R&D; & more &#; &#x; &{long_name}; &amp;")]
    );
    assert_eq!(
        unknown_reference_losses(&book),
        1,
        "R&D; is a named reference"
    );
}

#[test]
fn numeric_references_outside_unicode_are_replaced_and_reported() {
    let f = MobiFixture {
        markup: page("<p>&#xD800;|&#99999999999;|&#x1F30A;|&#233;</p>"),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(paragraphs(&book), ["\u{FFFD}|\u{FFFD}|\u{1F30A}|\u{e9}"]);
    assert!(book
        .losses
        .iter()
        .any(|l| l == "Byte sequences that are not valid text were replaced."));
    assert_eq!(unknown_reference_losses(&book), 0);
}

#[test]
fn greater_than_inside_quoted_attributes_stays_in_the_tag() {
    let f = MobiFixture {
        images: vec![JPEG.to_vec()],
        markup: page(
            "<p title=\"a > b\">One</p>\
<p class = 'x>y'>Two<img alt=\"<1> & >2\" recindex=\"1\"></p>\
<a href=\"x>y\" filepos=0000000001>Three</a>\
<p data-note=it's>Four</p>",
        ),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(paragraphs(&book), ["One", "Two", "Three", "Four"]);
    let blocks = &book.sections[0].blocks;
    assert!(blocks.iter().any(|b| matches!(b, Block::Image(0))));
    assert!(!book.losses.iter().any(|l| l.contains("cut off")));
}

#[test]
/// A quote with no partner within 1024 bytes, or whose value would contain the
/// start of a tag, is stray, so the first `>` ends the tag and a loss is noted.
/// A partner within that window closes the value, as in HTML.
fn unclosed_quote_in_a_tag_ends_at_the_first_greater_than() {
    let words = "no quotes here ".repeat(80);
    let long = "z".repeat(2000);
    let f = MobiFixture {
        markup: page(&format!(
            "<p title=\"oops>Kept one</p><p>{words}</p><p title=\"{long}>Kept two</p><p>Kept three</p>"
        )),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(
        paragraphs(&book),
        ["Kept one", words.trim(), "Kept two", "Kept three"]
    );
    assert!(has(&book, LOSS_STRAY_QUOTE), "{:?}", book.losses);

    let f = MobiFixture {
        markup: page("<p title=\"oops>Recovered</p><p class=\"next\">Kept</p>"),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(paragraphs(&book), ["Recovered", "Kept"]);
    assert!(has(&book, LOSS_STRAY_QUOTE), "{:?}", book.losses);
}

#[test]
fn quoted_value_cut_off_at_the_end_keeps_earlier_text() {
    let f = MobiFixture {
        markup: "<p>kept</p><p title=\"never closed".into(),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(paragraphs(&book), ["kept"]);
    assert!(book.losses.iter().any(|l| l.contains("cut off")));
}

#[test]
fn markup_cut_off_mid_tag_keeps_earlier_text() {
    let f = MobiFixture {
        markup: "<p>kept</p><p>also kept<img recindex=".into(),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(paragraphs(&book), ["kept", "also kept"]);
    assert!(book.losses.iter().any(|l| l.contains("cut off")));
}

#[test]
fn empty_text_is_invalid() {
    let f = MobiFixture {
        markup: page("<p> </p>"),
        ..Default::default()
    };
    invalid(decode(&f));
}

#[test]
fn encryption_and_drm_are_refused_before_any_text_is_read() {
    let garbage = vec![vec![0x80, 0x00, 0x08]];
    for encryption in [1, 2] {
        let f = MobiFixture {
            encryption,
            raw_text_records: Some(garbage.clone()),
            ..Default::default()
        };
        assert_eq!(err(decode(&f)), DecodeError::Protected);
    }
    let f = MobiFixture {
        drm: true,
        raw_text_records: Some(garbage),
        ..Default::default()
    };
    assert_eq!(err(decode(&f)), DecodeError::Protected);
}

#[test]
fn unsupported_variants_name_their_reason() {
    let cases: Vec<(MobiFixture, &str)> = vec![
        (
            MobiFixture {
                compression: 17480,
                ..Default::default()
            },
            "HUFF/CDIC compression",
        ),
        (
            MobiFixture {
                compression: 3,
                ..Default::default()
            },
            "compression type 3",
        ),
        (
            MobiFixture {
                mobi_type: 248,
                ..Default::default()
            },
            "KF8 (AZW3) book",
        ),
        (
            MobiFixture {
                version: 8,
                ..Default::default()
            },
            "KF8 (AZW3) book",
        ),
        (
            MobiFixture {
                extra_flags: 0x1_0003,
                ..Default::default()
            },
            "extra data flags 0x10003 name trailing entries above 0x8000",
        ),
        (
            MobiFixture {
                encoding: 1200,
                ..Default::default()
            },
            "text encoding 1200",
        ),
        (
            MobiFixture {
                mobi_type: 257,
                ..Default::default()
            },
            "MOBI type 257",
        ),
    ];
    for (f, reason) in cases {
        assert_eq!(err(decode(&f)), DecodeError::Unsupported(reason.into()));
    }
    let mut bytes = MobiFixture::default().build();
    bytes[60..68].copy_from_slice(b"TEXtREAd");
    assert_eq!(
        err(mobi::decode(&bytes)),
        DecodeError::Unsupported("PalmDOC text without a MOBI header".into())
    );
    bytes[60..68].copy_from_slice(b"PNG MOBI");
    assert!(matches!(
        mobi::decode(&bytes),
        Err(DecodeError::Unsupported(_))
    ));
    bytes[60..68].copy_from_slice(b"BOOKXXXX");
    invalid(mobi::decode(&bytes));
}

#[test]
fn every_truncation_of_the_sample_fails_cleanly() {
    let bytes = MobiFixture::sample().build();
    let eof_record_start = bytes.len() - 4;
    for len in 0..bytes.len() {
        let result = mobi::decode(&bytes[..len]);
        if len < eof_record_start {
            assert!(
                matches!(result, Err(DecodeError::Invalid(_))),
                "prefix {len}: {result:?}"
            );
        }
    }
}

#[test]
fn byte_flips_never_panic() {
    let original = MobiFixture::sample().build();
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..3000 {
        let mut bytes = original.clone();
        for _ in 0..(1 + next() % 4) {
            let at = (next() % bytes.len() as u64) as usize;
            bytes[at] = next() as u8;
        }
        if let Ok(book) = mobi::decode(&bytes) {
            let text: usize = paragraphs(&book).iter().map(|p| p.len()).sum();
            assert!(text < 4 * 1024 * 1024);
        }
    }
}

#[test]
fn malformed_structures_are_invalid() {
    let back_reference_before_start = vec![b'a', 0x80, 0x10, 0x81];
    let literal_past_end = vec![0x05, b'a', 0x81];
    let pair_cut_short = vec![b'a', 0x80];
    let trailing_size_too_large = vec![b'a', b'b', 0x00, 0x8F];
    let unterminated_size = vec![0x01, 0x01, 0x01];
    for record in [
        back_reference_before_start,
        literal_past_end,
        trailing_size_too_large,
        unterminated_size,
    ] {
        let f = MobiFixture {
            extra_flags: 0x2,
            raw_text_records: Some(vec![record]),
            declared_text_length: Some(1),
            ..Default::default()
        };
        invalid(decode(&f));
    }
    let f = MobiFixture {
        extra_flags: 0,
        raw_text_records: Some(vec![pair_cut_short]),
        declared_text_length: Some(1),
        ..Default::default()
    };
    invalid(decode(&f));

    let f = MobiFixture {
        declared_text_length: Some(2),
        ..Default::default()
    };
    invalid(decode(&f));
    let f = MobiFixture {
        declared_text_length: Some(5000),
        ..Default::default()
    };
    invalid(decode(&f));

    let mut bytes = MobiFixture::default().build();
    bytes[78..82].copy_from_slice(&0xFFFF_0000u32.to_be_bytes());
    invalid(mobi::decode(&bytes));

    let mut bytes = MobiFixture::default().build();
    let second = 86;
    let first = u32::from_be_bytes(bytes[78..82].try_into().unwrap());
    bytes[second..second + 4].copy_from_slice(&(first - 1).to_be_bytes());
    invalid(mobi::decode(&bytes));

    let mut bytes = MobiFixture::default().build();
    let r0 = u32::from_be_bytes(bytes[78..82].try_into().unwrap()) as usize;
    bytes[r0 + 84..r0 + 88].copy_from_slice(&0x00FF_FFFFu32.to_be_bytes());
    invalid(mobi::decode(&bytes));

    let mut bytes = MobiFixture::default().build();
    let exth = r0 + 16 + 232;
    assert_eq!(&bytes[exth..exth + 4], b"EXTH");
    bytes[exth + 4..exth + 8].copy_from_slice(&0x7FFF_FFFFu32.to_be_bytes());
    invalid(mobi::decode(&bytes));

    let mut bytes = MobiFixture::default().build();
    bytes[exth + 16..exth + 20].copy_from_slice(&3u32.to_be_bytes());
    invalid(mobi::decode(&bytes));

    let mut bytes = MobiFixture::default().build();
    bytes[r0 + 20..r0 + 24].copy_from_slice(&0x10_0000u32.to_be_bytes());
    invalid(mobi::decode(&bytes));
}

#[test]
fn shorter_text_than_declared_is_a_loss() {
    let markup = page("<p>short</p>");
    let declared = markup.len() as u32 + 10;
    let book = ok(&MobiFixture {
        markup,
        declared_text_length: Some(declared),
        ..Default::default()
    });
    assert!(book
        .losses
        .iter()
        .any(|l| l.contains("shorter than its declared length")));
}

#[test]
fn oversized_input_and_declared_text_hit_the_limit() {
    let mut huge = MobiFixture::default().build();
    huge.resize(mobi::MAX_INPUT_BYTES + 1, 0);
    assert_eq!(err(mobi::decode(&huge)), DecodeError::Limit);

    let f = MobiFixture {
        declared_text_length: Some(mobi::MAX_TEXT_BYTES as u32 + 1),
        ..Default::default()
    };
    assert_eq!(err(decode(&f)), DecodeError::Limit);
}

#[test]
fn expanding_record_is_stopped_at_one_record_of_output() {
    let mut bomb = vec![b'a', b'b', b'c'];
    for _ in 0..20_000 {
        bomb.extend_from_slice(&[0x80, (1 << 3) | 7]);
    }
    let f = MobiFixture {
        extra_flags: 0,
        raw_text_records: Some(vec![bomb.clone()]),
        declared_text_length: Some(4096),
        ..Default::default()
    };
    invalid(decode(&f));

    let literal = vec![b'x'; 5000];
    let f = MobiFixture {
        compression: 1,
        extra_flags: 0,
        raw_text_records: Some(vec![literal]),
        declared_text_length: Some(4096),
        ..Default::default()
    };
    invalid(decode(&f));
}

#[test]
fn many_records_at_the_limit_decode_with_bounded_output() {
    let body: String = (0..2000)
        .map(|i| format!("<p>Paragraph {i} {SAMPLE_FILLER}</p>"))
        .collect();
    let f = MobiFixture {
        compression: 1,
        markup: page(&body),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(paragraphs(&book).len(), 2000);
}

#[test]
fn compressor_round_trips_through_decoder() {
    let text = page(&format!(
        "<p>{}</p>",
        "abcabcabc \u{e9}\u{e9} xyzzy ".repeat(100)
    ));
    assert!(palmdoc_compress(text.as_bytes()).len() < text.len());
    let book = ok(&MobiFixture {
        markup: text,
        ..Default::default()
    });
    assert!(paragraphs(&book)[0].starts_with("abcabcabc \u{e9}\u{e9} xyzzy abcabcabc"));
}

/// Decodes a real file named by MOBI_SAMPLE and prints a summary. Optional
/// expectations: MOBI_EXPECT_TITLE, MOBI_EXPECT_AUTHOR, MOBI_EXPECT_TEXT.
#[test]
#[ignore]
fn real_sample_trial() {
    let path = std::env::var("MOBI_SAMPLE").expect("set MOBI_SAMPLE to a .mobi file");
    let bytes = std::fs::read(&path).unwrap();
    let started = std::time::Instant::now();
    let book = mobi::decode(&bytes).unwrap_or_else(|e| panic!("{path}: {e:?}"));
    let elapsed = started.elapsed();
    let paras = paragraphs(&book);
    let headings = book
        .sections
        .iter()
        .flat_map(|s| &s.blocks)
        .filter(|b| matches!(b, Block::Heading(_)))
        .count();
    let image_blocks = book
        .sections
        .iter()
        .flat_map(|s| &s.blocks)
        .filter(|b| matches!(b, Block::Image(_)))
        .count();
    println!(
        "file: {path} ({} bytes, decoded in {elapsed:?})",
        bytes.len()
    );
    println!(
        "title: {:?}\nauthors: {:?}\nlanguage: {:?}",
        book.title, book.authors, book.language
    );
    println!(
        "sections: {}, paragraphs: {}, headings: {headings}, image blocks: {image_blocks}, images: {}, cover: {:?}",
        book.sections.len(),
        paras.len(),
        book.images.len(),
        book.cover
    );
    println!(
        "first section titles: {:?}",
        book.sections
            .iter()
            .take(4)
            .map(|s| &s.title)
            .collect::<Vec<_>>()
    );
    println!("losses: {:#?}", book.losses);
    if let Ok(title) = std::env::var("MOBI_EXPECT_TITLE")
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or(())
    {
        assert_eq!(book.title, title);
    }
    if let Ok(author) = std::env::var("MOBI_EXPECT_AUTHOR")
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or(())
    {
        assert!(book.authors.contains(&author), "{:?}", book.authors);
    }
    if let Ok(text) = std::env::var("MOBI_EXPECT_TEXT")
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or(())
    {
        let at = paras.iter().position(|p| p.contains(&text));
        println!("expected text found in paragraph {at:?}");
        assert!(at.is_some(), "text not found");
    }
}

// Regression checks for the review findings on the MOBI decoder.

const LOSS_UNCLOSED_SKIP: &str =
    "Text after an unclosed script, style or head element was dropped.";
const LOSS_STRAY_QUOTE: &str =
    "Tags with an unmatched quote were ended at their first >; nearby text may be out of place.";
const LOSS_BAD_METADATA: &str =
    "Title, author or language bytes that are not valid text were replaced.";
const LOSS_GUIDE: &str =
    "The book's guide (such as its start position and contents entry) was not carried over.";
const LOSS_HEADING_LEVELS: &str =
    "Heading levels were changed; every heading is written at the same level.";
const LOSS_PRE: &str = "Preformatted text lost its line breaks and spacing.";
const LOSS_LAYOUT: &str = "Layout such as alignment, indentation and spacing was removed.";
const LOSS_NUMBERED: &str =
    "Sections without a heading were given numbered titles such as \"Section 1\".";

fn has(book: &ParsedBook, loss: &str) -> bool {
    book.losses.iter().any(|l| l == loss)
}

/// Losses other than the numbered-title note, which any book without headings has.
fn other_losses(book: &ParsedBook) -> Vec<String> {
    book.losses
        .iter()
        .filter(|l| *l != LOSS_NUMBERED)
        .cloned()
        .collect()
}

#[test]
fn first_content_record_field_chooses_where_text_starts() {
    let f = MobiFixture {
        compression: 1,
        extra_flags: 0,
        raw_text_records: Some(vec![b"<p>NO</p>".to_vec(), b"<p>OK</p>".to_vec()]),
        declared_text_records: Some(1),
        declared_text_length: Some(9),
        first_content_record: Some(2),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(paragraphs(&book), ["OK"]);

    for first in [0u16, 2, 900] {
        let f = MobiFixture {
            first_content_record: Some(first),
            raw_text_records: Some(vec![b"<p>NO</p>".to_vec(), b"<p>OK</p>".to_vec()]),
            declared_text_records: Some(if first == 2 { 600 } else { 2 }),
            declared_text_length: Some(18),
            compression: 1,
            extra_flags: 0,
            ..Default::default()
        };
        invalid(decode(&f));
    }
}

#[test]
fn text_after_skipped_elements_survives() {
    for markup in [
        "<p>Before</p><script>if (1 < 2) {}</script><p>After</p>",
        "<p>Before</p><SCRIPT type=\"text/javascript\">a = '</p>' < b</SCRIPT ><p>After</p>",
        "<p>Before</p><style>p > a { x: \"<\" }</style><p>After</p>",
        "<p>Before</p><script>x = \"</scripts>\";</script><p>After</p>",
    ] {
        let book = ok(&MobiFixture {
            markup: page(markup),
            ..Default::default()
        });
        assert_eq!(paragraphs(&book), ["Before", "After"], "{markup}");
        assert!(!has(&book, LOSS_UNCLOSED_SKIP), "{markup}");
    }

    let book = ok(&MobiFixture {
        markup: "<html><head><title>T</title><script>if (a<b) {}</script></head><body><p>After</p></body></html>"
            .into(),
        ..Default::default()
    });
    assert_eq!(paragraphs(&book), ["After"]);
}

#[test]
fn unclosed_skipped_element_is_reported() {
    let book = ok(&MobiFixture {
        markup: page("<p>Before</p><script>if (1 < 2) {}<p>Lost</p>"),
        ..Default::default()
    });
    assert_eq!(paragraphs(&book), ["Before"]);
    assert!(has(&book, LOSS_UNCLOSED_SKIP), "{:?}", book.losses);

    let book = ok(&MobiFixture {
        markup: "<html><body><p>Before</p><head><p>Lost</p>".into(),
        ..Default::default()
    });
    assert_eq!(paragraphs(&book), ["Before"]);
    assert!(has(&book, LOSS_UNCLOSED_SKIP), "{:?}", book.losses);
}

const LOSS_SECOND_COPY: &str =
    "The file may also hold a newer-format (KF8) copy of the book, which was not read.";
const LOSS_UNREAD_RECORDS: &str =
    "Records after the declared text whose purpose is uncertain were not read; they may hold more of the book.";

#[test]
fn text_records_are_text_whatever_they_contain() {
    let prose = MobiFixture {
        compression: 1,
        extra_flags: 0,
        markup: "BOUNDARY is a word in prose.".into(),
        ..Default::default()
    };
    let book = ok(&prose);
    assert_eq!(paragraphs(&book), ["BOUNDARY is a word in prose."]);
    assert_eq!(other_losses(&book), Vec::<String>::new());
}

#[test]
fn possible_newer_copy_is_reported_and_older_text_converted() {
    for f in [
        MobiFixture {
            exth_kf8_boundary: true,
            ..Default::default()
        },
        MobiFixture {
            exth_kf8_boundary: true,
            boundary_record: true,
            images: vec![JPEG.to_vec()],
            ..Default::default()
        },
        MobiFixture {
            extra_exth: vec![(121, vec![0xFF; 4])],
            ..Default::default()
        },
        MobiFixture {
            extra_exth: vec![(121, vec![])],
            ..Default::default()
        },
    ] {
        let book = ok(&f);
        assert_eq!(paragraphs(&book), ["Hello."]);
        assert_eq!(other_losses(&book), [LOSS_SECOND_COPY]);
    }
}

#[test]
fn a_record_named_boundary_alone_says_nothing() {
    let book = ok(&MobiFixture {
        boundary_record: true,
        images: vec![JPEG.to_vec()],
        ..Default::default()
    });
    assert_eq!(paragraphs(&book), ["Hello."]);
    assert_eq!(other_losses(&book), Vec::<String>::new());
}

#[test]
fn records_between_text_and_first_non_book_are_left_out_and_reported() {
    let book = ok(&MobiFixture {
        records_after_text: vec![b"<p>More</p>".to_vec()],
        ..Default::default()
    });
    assert_eq!(paragraphs(&book), ["Hello."]);
    assert_eq!(other_losses(&book), [LOSS_UNREAD_RECORDS]);

    let text_end = MobiFixture::default().encoded_text().len().div_ceil(4096) as u32 + 1;
    let book = ok(&MobiFixture {
        first_non_book: Some(text_end + 1),
        ..Default::default()
    });
    assert_eq!(
        other_losses(&book),
        Vec::<String>::new(),
        "the 2-zero-byte record is expected there"
    );

    let book = ok(&MobiFixture {
        records_after_text: vec![vec![0], vec![0, 0, 0]],
        ..Default::default()
    });
    assert_eq!(
        other_losses(&book),
        Vec::<String>::new(),
        "zero-byte records hold no text"
    );

    let book = ok(&MobiFixture {
        first_non_book: Some(text_end - 1),
        ..Default::default()
    });
    assert_eq!(paragraphs(&book), ["Hello."], "the declared range wins");
}

/// The review's case: one declared text record, a second record of markup after
/// it, and no first non-book record. The text converts and the report says a
/// record was left out.
#[test]
fn uncertain_records_are_reported_when_the_first_non_book_field_cannot_bound_them() {
    let kept_then_more = |first_non_book: Option<u32>| MobiFixture {
        compression: 1,
        extra_flags: 0,
        raw_text_records: Some(vec![b"<p>Kept</p>".to_vec(), b"<p>More</p>".to_vec()]),
        declared_text_records: Some(1),
        declared_text_length: Some(11),
        first_non_book,
        ..Default::default()
    };
    // Field unset, before the end of the text, at record 0, or past the database.
    for field in [0xFFFF_FFFF, 1, 0, 60_000] {
        let book = ok(&kept_then_more(Some(field)));
        assert_eq!(paragraphs(&book), ["Kept"], "offset 80 = {field}");
        assert_eq!(
            other_losses(&book),
            [LOSS_UNREAD_RECORDS],
            "offset 80 = {field}"
        );
    }

    // Offset 108 says nothing about records before or after an image, so it
    // does not settle where the book ends: here it names the record holding
    // <p>More</p>.
    let book = ok(&MobiFixture {
        first_image: Some(2),
        ..kept_then_more(Some(0xFFFF_FFFF))
    });
    assert_eq!(paragraphs(&book), ["Kept"]);
    assert_eq!(other_losses(&book), [LOSS_UNREAD_RECORDS]);

    // A MOBI header too short to hold offset 80 is just as uncertain.
    let short = MobiFixture {
        authors: vec![],
        language: None,
        ..kept_then_more(None)
    };
    let mut bytes = short.build();
    let r0 = u32::from_be_bytes(bytes[78..82].try_into().unwrap()) as usize;
    bytes[r0 + 20..r0 + 24].copy_from_slice(&60u32.to_be_bytes());
    let book = mobi::decode(&bytes).expect("short header decodes");
    assert_eq!(paragraphs(&book), ["Kept"]);
    assert_eq!(other_losses(&book), [LOSS_UNREAD_RECORDS]);
}

/// Without a usable first non-book field nothing documented says which later
/// records belong to the book, so any record holding a non-zero byte is
/// reported, whatever its first bytes are and wherever offset 108 points.
#[test]
fn records_after_the_text_are_uncertain_without_the_first_non_book_field() {
    let more_after_a_jpeg = MobiFixture {
        first_non_book: Some(0xFFFF_FFFF),
        images: vec![JPEG.to_vec(), b"<p>More</p>".to_vec()],
        cover: Some(0),
        ..Default::default()
    };
    let flis_prefixed_text = MobiFixture {
        first_non_book: Some(0xFFFF_FFFF),
        records_after_text: vec![b"FLIS<p>More</p>".to_vec()],
        ..Default::default()
    };
    let ordinary_trailer = MobiFixture {
        first_non_book: Some(0xFFFF_FFFF),
        ..Default::default()
    };
    let before_the_text = MobiFixture {
        first_non_book: Some(1),
        images: vec![JPEG.to_vec(), PNG.to_vec()],
        cover: Some(1),
        ..Default::default()
    };
    for (name, f) in [
        ("JPEG then More", more_after_a_jpeg),
        ("FLIS-prefixed More", flis_prefixed_text),
        ("FLIS, FCIS and end of file only", ordinary_trailer),
        ("offset 80 before the text", before_the_text),
    ] {
        let book = ok(&f);
        assert_eq!(paragraphs(&book), ["Hello."], "{name}");
        assert_eq!(other_losses(&book), [LOSS_UNREAD_RECORDS], "{name}");
    }

    let only_zero_bytes = MobiFixture {
        compression: 1,
        extra_flags: 0,
        raw_text_records: Some(vec![b"<p>Kept</p>".to_vec()]),
        declared_text_length: Some(11),
        first_non_book: Some(0xFFFF_FFFF),
        ..Default::default()
    };
    // Keep record 0, the text and the 2-zero-byte record; empty the rest.
    let mut bytes = only_zero_bytes.build();
    let count = u16::from_be_bytes(bytes[76..78].try_into().unwrap()) as usize;
    let zero_record = u32::from_be_bytes(bytes[94..98].try_into().unwrap()) as usize;
    let end = zero_record + 2;
    for entry in 3..count {
        let at = 78 + 8 * entry;
        bytes[at..at + 4].copy_from_slice(&(end as u32).to_be_bytes());
    }
    bytes.truncate(end);
    let book = mobi::decode(&bytes).expect("zero-byte trailer decodes");
    assert_eq!(paragraphs(&book), ["Kept"]);
    assert_eq!(
        other_losses(&book),
        Vec::<String>::new(),
        "zero bytes hold no text"
    );
}

#[test]
fn trailing_entries_outside_the_documented_form_are_refused() {
    let f = MobiFixture {
        extra_flags: 0x8003,
        ..Default::default()
    };
    assert_eq!(paragraphs(&ok(&f)), ["Hello."]);

    let f = MobiFixture {
        extra_flags: 0x2,
        raw_text_records: Some(vec![vec![b'a', 0x00, 0x01, 0x01, 0x01, 0x01]]),
        declared_text_length: Some(1),
        ..Default::default()
    };
    assert_eq!(
        err(decode(&f)),
        DecodeError::Unsupported("trailing entry size longer than four bytes".into())
    );
}

#[test]
fn repaired_metadata_is_reported() {
    let f = MobiFixture {
        raw_title: Some(b"Bad \xFF".to_vec()),
        ..Default::default()
    };
    let book = ok(&f);
    assert_eq!(book.title, "Bad \u{FFFD}");
    assert!(has(&book, LOSS_BAD_METADATA), "{:?}", book.losses);

    let f = MobiFixture {
        authors: vec![],
        language: None,
        extra_exth: vec![(100, b"Ann \xC3".to_vec()), (524, b"e\xFFn".to_vec())],
        ..Default::default()
    };
    let book = ok(&f);
    assert!(has(&book, LOSS_BAD_METADATA), "{:?}", book.losses);

    let f = MobiFixture {
        encoding: 1252,
        raw_title: Some(b"Odd \x81".to_vec()),
        ..Default::default()
    };
    assert!(has(&ok(&f), LOSS_BAD_METADATA));

    let clean = ok(&MobiFixture::default());
    assert!(!has(&clean, LOSS_BAD_METADATA));
}

#[test]
fn unused_metadata_guide_and_heading_levels_are_reported() {
    let f = MobiFixture {
        extra_exth: vec![
            (101, b"Harbour Press".to_vec()),
            (103, b"A short blurb.".to_vec()),
            (101, b"Second Press".to_vec()),
            (204, 201u32.to_be_bytes().to_vec()),
        ],
        ..Default::default()
    };
    let book = ok(&f);
    assert!(
        book.losses
            .iter()
            .any(|l| l == "Metadata not carried over: publisher, description."),
        "{:?}",
        book.losses
    );

    let book = ok(&MobiFixture::default());
    assert!(!book.losses.iter().any(|l| l.starts_with("Metadata not")));

    let book = ok(&MobiFixture {
        markup: "<html><head><guide><reference type=\"toc\" filepos=0000000001 /></guide></head><body><h2>A</h2><p>x</p></body></html>".into(),
        ..Default::default()
    });
    assert!(has(&book, LOSS_GUIDE));
    assert!(!has(&book, LOSS_HEADING_LEVELS), "h2 is the level written");

    for heading in ["h1", "h3", "h6"] {
        let book = ok(&MobiFixture {
            markup: page(&format!("<{heading}>A</{heading}><p>x</p>")),
            ..Default::default()
        });
        assert!(has(&book, LOSS_HEADING_LEVELS), "{heading}");
    }
}

#[test]
fn lost_layout_is_reported() {
    let book = ok(&MobiFixture {
        markup: page("<pre>one\n    two</pre>"),
        ..Default::default()
    });
    assert_eq!(paragraphs(&book), ["one two"]);
    assert!(has(&book, LOSS_PRE));

    for markup in [
        "<p style=\"text-align:center\">Centred</p>",
        "<p align=\"center\">Centred</p>",
        "<center>Centred</center>",
        "<p width=\"2em\">Centred</p>",
        "<blockquote>Centred</blockquote>",
    ] {
        let book = ok(&MobiFixture {
            markup: page(markup),
            ..Default::default()
        });
        assert_eq!(paragraphs(&book), ["Centred"], "{markup}");
        assert!(has(&book, LOSS_LAYOUT), "{markup}: {:?}", book.losses);
    }

    let book = ok(&MobiFixture {
        markup: page("<p>a</p><hr/><p>b</p>"),
        ..Default::default()
    });
    assert!(has(&book, "Horizontal rules were removed."));

    let book = ok(&MobiFixture {
        images: vec![JPEG.to_vec()],
        markup: page("<p>Plain</p><img recindex=1 align=\"baseline\" width=\"10\">"),
        ..Default::default()
    });
    assert_eq!(other_losses(&book), Vec::<String>::new());
}

#[test]
fn numbered_section_titles_are_reported() {
    let book = ok(&MobiFixture {
        markup: page("<p>one</p><mbp:pagebreak/><h2>Two</h2><p>two</p>"),
        ..Default::default()
    });
    assert!(has(&book, LOSS_NUMBERED));
    let book = ok(&MobiFixture {
        markup: page("<h2>One</h2><p>one</p>"),
        ..Default::default()
    });
    assert!(!has(&book, LOSS_NUMBERED));
}

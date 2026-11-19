# MOBI decoder tests

Run `cargo test -p ebook-convert --test mobi` or build the standalone harness from `crates/ebook-convert`:

```sh
dir=$(mktemp -d)
rustc --edition 2021 --test tests/mobi/harness.rs -o "$dir/mobi-harness" && "$dir/mobi-harness"
```

- [fixture.rs](fixture.rs) builds MOBI 7 files. `MobiFixture` has public fields
  for metadata, encoding, compression, encryption, DRM, KF8 markers, trailing
  entry flags, markup, images and the cover, `raw_text_records` for
  hand-made broken records, records placed after the text, and overrides for
  the text record count, the first content record, the first non-book record,
  the full-name bytes and extra EXTH records. `boundary_record` adds a record
  holding `BOUNDARY`; no allowed source describes such a record, and the decoder
  must ignore it. It has no dependencies, so another test can include it with
  `#[path = ".../tests/mobi/fixture.rs"] mod fixture;`.
- [sample.mobi](sample.mobi) is `MobiFixture::sample().build()`, written by
  [gen_sample.rs](gen_sample.rs). `checked_in_sample_matches_generator` fails
  when the two drift. To regenerate:
  `rustc --edition 2021 tests/mobi/gen_sample.rs -o "$dir/gen-sample" && "$dir/gen-sample" tests/mobi/sample.mobi`.
- [images](images) holds 2×2 JPEG, PNG and GIF files made for these tests.

## What `sample.mobi` decodes to

UTF-8, PalmDOC-compressed, three text records with trailing entries.

- Title "The Lantern Keeper", authors "Ada Marlow" and "Ben Ortiz", language `en`.
- Sections "Chapter One: The Lamp", "Chapter Two: The Storm",
  "Chapter Three: Morning", each starting with its heading.
- `images[0]` is the JPEG, placed at the end of chapter one; `images[1]` is the
  PNG, placed in chapter three and used as the cover (`cover == Some(1)`).
- Chapter two references a GIF, which is dropped.
- Seven losses: inline formatting, links, tables, lists, GIF images, heading
  levels (`h1` and `h2` are both written as one level) and the guide.
- Text with entities and non-ASCII characters, for example
  `Its keeper, Marta Ruiz & her cat — kept the log in café ink: “All clear.”`
  (a no-break space between "All" and "clear").

## Real-book trial

`real_sample_trial` is ignored by default. It decodes the file in `MOBI_SAMPLE`,
prints a summary and checks the optional `MOBI_EXPECT_TITLE`,
`MOBI_EXPECT_AUTHOR` and `MOBI_EXPECT_TEXT`:

```sh
MOBI_SAMPLE=pg84-images.mobi MOBI_EXPECT_AUTHOR="Mary Wollstonecraft Shelley" \
  "$dir/mobi-harness" --ignored --nocapture real_sample_trial
```

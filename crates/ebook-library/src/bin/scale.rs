//! Five warm repetitions. Generation and answer checking are outside the timers.
use ebook_library::*;
use std::{collections::BTreeMap, time::Instant};
fn run(count: u32) -> Result<()> {
    let d = tempfile::tempdir()?;
    fixture::generate(d.path(), count)?;
    let mut order: Vec<_> = (0..count).collect();
    order.sort_by_key(|&i| fixture::bucket(i));
    let mut resort = order.clone();
    resort.sort_by_key(|&i| (i % 7, i % 257, fixture::bucket(i), i));
    let queries = [
        ("a", order.clone()),
        (
            "atlas",
            order
                .iter()
                .copied()
                .filter(|&i| fixture::bucket(i).is_multiple_of(4))
                .collect(),
        ),
        (
            "book 4094",
            order
                .iter()
                .copied()
                .filter(|&i| fixture::bucket(i) == 4094)
                .collect(),
        ),
        ("unfindable", vec![]),
    ];
    let mut samples: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for repeat in 0..6 {
        macro_rules! timed {
            ($name:expr,$body:expr) => {{
                let start = Instant::now();
                let result = $body;
                let ms = start.elapsed().as_secs_f64() * 1000.;
                if repeat > 0 {
                    samples.entry($name).or_default().push(ms);
                }
                result
            }};
        }
        let cancel = Cancel::default();
        let (lib, mut reader, v) = timed!("open", {
            let lib = Library::open(d.path())?;
            let mut reader = lib.browse_reader()?;
            let v = reader.open(&Default::default(), &cancel)?;
            (lib, reader, v)
        });
        let check = |v: &OpenView, expected: &[u32]| {
            assert_eq!(v.count as usize, expected.len());
            assert_eq!(
                v.first.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
                expected
                    .iter()
                    .take(128)
                    .copied()
                    .map(fixture::id)
                    .collect::<Vec<_>>()
            );
        };
        check(&v, &order);
        let strip = timed!("shelf", reader.shelf(v.token)?);
        assert!(strip.groups.is_empty());
        assert!(
            strip.thickness.iter().copied().eq(order
                .iter()
                .map(|i| (16 + (i % 480000 + 6000) / 12000).min(48) as u8))
        );
        let mut slowest = 0f64;
        for j in 0..30 {
            let start = if j == 29 {
                count - 128
            } else {
                ((j as u64 * 2654435761) % (count as u64 - 128)) as u32
            };
            let clock = Instant::now();
            let page = reader.window(v.token, start, 128)?;
            slowest = slowest.max(clock.elapsed().as_secs_f64() * 1000.);
            assert!(
                page.iter().map(|r| r.id.clone()).eq(order[start as usize..]
                    .iter()
                    .take(128)
                    .copied()
                    .map(fixture::id))
            );
        }
        if repeat > 0 {
            samples.entry("window").or_default().push(slowest);
        }
        for (text, expected) in &queries {
            let v = timed!(
                *text,
                reader.open(
                    &BrowseQuery {
                        text: (*text).into(),
                        ..Default::default()
                    },
                    &cancel
                )?
            );
            check(&v, expected);
        }
        let sort = SortOrder::new(
            [Field::Added, Field::Author, Field::Title]
                .map(|field| SortTerm {
                    field,
                    descending: false,
                })
                .to_vec(),
        )?;
        let v = timed!(
            "resort",
            reader.open(
                &BrowseQuery {
                    text: String::new(),
                    sort
                },
                &cancel
            )?
        );
        check(&v, &resort);
        let i = count - 1;
        let found = timed!("import", fixture::lookup(&lib, i)?);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, fixture::id(i));
    }
    for (name, mut values) in samples {
        values.sort_by(f64::total_cmp);
        println!(
            "{count} {name} {:.3} [{:.3}, {:.3}] ms",
            values[2], values[0], values[4]
        );
    }
    Ok(())
}
fn main() -> Result<()> {
    for count in [100000, 1000000] {
        run(count)?;
    }
    Ok(())
}

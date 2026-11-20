//! Row geometry in logical pixels. Packing includes one pixel between adjacent spines.
#[derive(Debug, PartialEq)]
pub struct Row {
    pub first: u32,
    pub top: u64,
    pub caption: Option<usize>,
}

pub fn pack(
    thickness: &[u8],
    groups: &[(u32, String)],
    width: u32,
    spine_height: u32,
    caption_height: u32,
) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut used = 0u32;
    let mut next_group = 0;
    let mut top = 0u64;
    for (rank, &size) in thickness.iter().enumerate() {
        let rank = rank as u32;
        let size = u32::from(size);
        let caption = groups
            .get(next_group)
            .and_then(|(first, _)| (*first == rank).then_some(next_group));
        if rank == 0 || caption.is_some() || used.saturating_add(size + 1) > width {
            rows.push(Row {
                first: rank,
                top,
                caption,
            });
            top += u64::from(spine_height) + 14 + 7 + 12 + 10 + 28;
            if caption.is_some() {
                top += u64::from(caption_height);
                next_group += 1;
            }
            used = size;
        } else {
            used += 1 + size;
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_fit_group_break_and_wide_book() {
        let groups = [(0, "A".into()), (2, "B".into())];
        let packed = pack(&[16, 16, 16, 48, 16], &groups, 33, 144, 20);
        let actual: Vec<_> = packed.iter().map(|r| (r.first, r.top, r.caption)).collect();
        assert_eq!(
            actual,
            [
                (0, 0, Some(0)),
                (2, 235, Some(1)),
                (3, 470, None),
                (4, 685, None)
            ]
        );
        assert!(pack(&[], &[], 0, 144, 20).is_empty());
        assert_eq!(pack(&[16, 16], &[], 0, 144, 20).len(), 2);
        assert_eq!(pack(&[16, 16], &[(1, "B".into())], 300, 144, 20).len(), 2);
    }

    #[test]
    fn packs_a_million_books() {
        let books = vec![24; 1_000_000];
        let start = std::time::Instant::now();
        let rows = pack(&books, &[], 999, 192, 20);
        let elapsed = start.elapsed();
        assert_eq!(rows.len(), 25_000);
        assert_eq!(rows.last().unwrap().first, 999_960);
        eprintln!("1,000,000 spines, 25,000 rows in {elapsed:?}");
    }
}

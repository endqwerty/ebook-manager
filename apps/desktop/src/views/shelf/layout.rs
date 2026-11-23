//! Row geometry in logical pixels. Packing includes one pixel between adjacent spines.
#[derive(Debug, PartialEq)]
pub struct Row {
    pub first: u32,
    pub top: u64,
    pub caption: Option<usize>,
}

impl Row {
    pub fn spine_top(&self, caption_height: u64) -> u64 {
        self.top
            + if self.caption.is_some() {
                caption_height
            } else {
                0
            }
    }
}

/// Whether a row's spine cells, `cell_height` tall below any caption, lie
/// wholly outside the view from `y` down `view_height`.
pub fn spines_hidden(
    row: &Row,
    caption_height: u64,
    cell_height: u64,
    y: u64,
    view_height: u64,
) -> bool {
    let top = row.spine_top(caption_height);
    top + cell_height <= y || top >= y + view_height
}

pub fn first_full_book(rows: &[Row], scroll: u64, caption_height: u64) -> Option<u32> {
    let index = rows.partition_point(|row| row.spine_top(caption_height) < scroll);
    rows.get(index).map(|row| row.first)
}

pub fn row_of(rows: &[Row], rank: u32) -> usize {
    rows.partition_point(|r| r.first <= rank).saturating_sub(1)
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
    fn spines_count_as_hidden_only_once_wholly_out_of_view() {
        let plain = Row {
            first: 0,
            top: 0,
            caption: None,
        };
        let grouped = Row {
            first: 0,
            top: 0,
            caption: Some(0),
        };
        assert!(!spines_hidden(&plain, 24, 206, 205, 600));
        assert!(spines_hidden(&plain, 24, 206, 206, 600));
        assert!(!spines_hidden(&grouped, 24, 206, 218, 600));
        assert!(!spines_hidden(&grouped, 24, 206, 229, 600));
        assert!(spines_hidden(&grouped, 24, 206, 230, 600));
        let lower = Row {
            first: 40,
            top: 600,
            caption: Some(1),
        };
        assert!(!spines_hidden(&lower, 24, 206, 1, 624));
        assert!(spines_hidden(&lower, 24, 206, 0, 624));
    }

    #[test]
    fn deep_resize_tracks_the_first_complete_spine_through_both_widths() {
        let widths = vec![24; 5000];
        let groups = [(4000, String::from("Q"))];
        let wide = pack(&widths, &groups, 999, 192, 24);
        let narrow = pack(&widths, &groups, 499, 192, 24);
        let caption = wide[row_of(&wide, 4000)].top;
        assert_eq!(first_full_book(&wide, caption + 8, 24), Some(4000));
        let rank = first_full_book(&wide, caption + 25, 24).unwrap();
        assert_eq!(rank, 4040);
        let top = narrow[row_of(&narrow, rank)].top;
        assert!(top > wide.last().unwrap().top);
        let rank = first_full_book(&narrow, top, 24).unwrap();
        assert_eq!(wide[row_of(&wide, rank)].first, 4040);
    }

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

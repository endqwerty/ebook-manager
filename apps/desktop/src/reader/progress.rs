//! How far through the book a place is.

/// One reading-order item, as far as progress cares.
#[derive(Clone, Copy, Debug)]
pub struct Weight {
    /// The chapter's size, used only to weigh it against the others.
    pub size: u32,
    pub linear: bool,
}

/// The fraction of the book read with `page` of `pages` showing in
/// `chapter`: the sizes of the linear chapters before it, plus its own size
/// times how far through it the page is, over the sizes of all linear
/// chapters. Showing a chapter's last page counts the whole chapter, so the
/// end of the last linear chapter is exactly 1. A chapter that is not linear
/// counts as the start of the next linear one, or as 1 after the last. When
/// every size is 0 each linear chapter weighs the same.
pub fn fraction(chapters: &[Weight], chapter: usize, page: u32, pages: u32) -> f32 {
    let even = chapters.iter().all(|c| !c.linear || c.size == 0);
    let weight = |c: &Weight| match (c.linear, even) {
        (false, _) => 0.0,
        (true, true) => 1.0,
        (true, false) => f64::from(c.size),
    };
    let total: f64 = chapters.iter().map(weight).sum();
    let Some(current) = chapters.get(chapter) else {
        return 1.0;
    };
    if total <= 0.0 {
        return 0.0;
    }
    let before: f64 = chapters[..chapter].iter().map(weight).sum();
    let through = if !current.linear {
        0.0
    } else if pages == 0 || page + 1 >= pages {
        1.0
    } else {
        f64::from(page) / f64::from(pages)
    };
    let read = before + weight(current) * through;
    // Exactly 1 at the end, whatever the rounding on the way.
    if read >= total {
        1.0
    } else {
        (read / total) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::{Weight, fraction};

    fn linear(size: u32) -> Weight {
        Weight { size, linear: true }
    }
    fn aside(size: u32) -> Weight {
        Weight {
            size,
            linear: false,
        }
    }

    /// Behaviour 13 of docs/design/epub-reading.md.
    #[test]
    fn fraction_weighs_chapters_by_size() {
        // A cover, a title page and two long chapters.
        let book = [linear(100), linear(100), linear(4900), linear(4900)];
        assert_eq!(fraction(&book, 0, 0, 1), 0.01);
        // The start of chapter one is 2% in, not half.
        assert_eq!(fraction(&book, 2, 0, 10), 0.02);
        assert_eq!(fraction(&book, 2, 5, 10), 0.265);
        // A chapter's last page counts the whole chapter.
        assert_eq!(fraction(&book, 2, 9, 10), 0.51);
        assert_eq!(fraction(&book, 3, 9, 10), 1.0);
        assert_eq!(fraction(&book, 3, 8, 10), 0.902);
    }

    #[test]
    fn a_chapter_outside_the_reading_line_is_the_start_of_the_next() {
        let book = [linear(50), aside(1000), linear(50), aside(10)];
        assert_eq!(fraction(&book, 1, 3, 9), 0.5);
        assert_eq!(fraction(&book, 2, 0, 2), 0.5);
        // After the last linear chapter the book is read.
        assert_eq!(fraction(&book, 3, 0, 5), 1.0);
        assert_eq!(fraction(&book, 2, 1, 2), 1.0);
    }

    #[test]
    fn chapters_with_no_size_weigh_the_same() {
        let book = [linear(0), linear(0), aside(0), linear(0), linear(0)];
        assert_eq!(fraction(&book, 1, 0, 4), 0.25);
        assert_eq!(fraction(&book, 3, 1, 2), 0.75);
        assert_eq!(fraction(&book, 4, 0, 1), 1.0);
        assert_eq!(fraction(&[], 0, 0, 1), 1.0);
        assert_eq!(fraction(&[aside(5)], 0, 0, 1), 0.0);
    }
}

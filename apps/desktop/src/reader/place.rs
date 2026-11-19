//! The place the reader holds, and the one rule for replacing it.

/// Where the reader is in a chapter, as the page reported it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub chapter: u32,
    pub offset: u32,
    pub page: u32,
    pub pages: u32,
}

/// The place to save: the place of the newest move the page has reported.
///
/// "Newest" is the order the moves were made in. Each chapter the view loads
/// has a number that only grows, and the page counts its moves within a
/// load, so a move's order is that pair. A place is taken only if it is a
/// move and its order is after the held one's. So a turn answered after the
/// next chapter has reported does not replace that chapter's place, and a
/// reflow, which is not a move, replaces nothing.
#[derive(Debug, Default)]
pub struct Held {
    place: Option<Place>,
    order: (u32, u32),
}

impl Held {
    pub fn get(&self) -> Option<Place> {
        self.place
    }

    /// Take a place the page reported. Says whether it is now the place to
    /// save, which is when to save it.
    pub fn take(&mut self, place: Place, moved: bool, order: (u32, u32)) -> bool {
        if !moved || (self.place.is_some() && order <= self.order) {
            return false;
        }
        self.place = Some(place);
        self.order = order;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Held, Place};

    fn at(chapter: u32, offset: u32) -> Place {
        Place {
            chapter,
            offset,
            page: 0,
            pages: 1,
        }
    }

    #[test]
    fn a_move_is_held_and_a_reflow_is_not() {
        let mut held = Held::default();
        assert!(held.take(at(0, 10), true, (1, 1)));
        // A resize reports new page numbers for the same place.
        let reflowed = Place {
            page: 4,
            pages: 30,
            ..at(0, 10)
        };
        assert!(!held.take(reflowed, false, (1, 1)));
        assert_eq!(held.get(), Some(at(0, 10)));
        assert!(held.take(at(0, 500), true, (1, 2)));
        assert_eq!(held.get(), Some(at(0, 500)));
    }

    #[test]
    fn a_turn_answered_after_the_next_chapter_was_chosen_is_held() {
        let mut held = Held::default();
        // Chapter A landed, then turned twice. The second turn's answer comes
        // after chapter B was asked for, and before B has reported.
        assert!(held.take(at(0, 0), true, (1, 1)));
        assert!(held.take(at(0, 900), true, (1, 2)));
        assert!(held.take(at(0, 1800), true, (1, 3)));
        assert_eq!(held.get(), Some(at(0, 1800)));
        // Then B lands, and is the place.
        assert!(held.take(at(1, 0), true, (2, 1)));
        assert_eq!(held.get(), Some(at(1, 0)));
    }

    #[test]
    fn a_late_answer_does_not_replace_a_newer_place() {
        let mut held = Held::default();
        assert!(held.take(at(0, 0), true, (1, 1)));
        // B has reported its starting place.
        assert!(held.take(at(1, 0), true, (2, 1)));
        // A's turn is answered late. It was made before B's landing.
        assert!(!held.take(at(0, 900), true, (1, 2)));
        assert_eq!(held.get(), Some(at(1, 0)));
        // The same move told twice is taken once.
        assert!(!held.take(at(1, 0), true, (2, 1)));
        // A move in B is newer.
        assert!(held.take(at(1, 700), true, (2, 2)));
    }
}

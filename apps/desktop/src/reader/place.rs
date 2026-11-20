//! The reader's place-keeping, as one function over an explicit state.
//!
//! Nothing here knows about WebKit, GTK or the library. The view and the
//! reader tell `Keeper::step` what happened, and do what it answers. So the
//! rule can be run through thousands of orders of events that no one would
//! think to write by hand: see the tests.
//!
//! The rule, from docs/design/epub-reading.md, "A place in the book":
//!
//! - The place held is the place of the most recently made move among those
//!   whose answers have arrived, whatever is showing or loading now.
//! - What is displayed belongs to the load being shown.
//! - Saves are submitted in the order their places were made, and the last
//!   one before leaving is the place held.
//! - A report that is not a move changes only what is displayed.

/// One chapter load the view made: its number, which only grows, the chapter
/// it showed and the length of that chapter's text. Every answer and message
/// from the page carries the record of the load it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Load {
    pub number: u32,
    pub chapter: u32,
    pub length: u32,
}

/// The order a move was made in: its load's number, then the page's count
/// of its moves in that load.
pub type Order = (u32, u32);

/// Where the reader is in a chapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub chapter: u32,
    pub offset: u32,
    pub page: u32,
    pub pages: u32,
}

/// What the page said about where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub load: Load,
    pub offset: u32,
    pub page: u32,
    pub pages: u32,
    /// The page's count of its moves in this load, at this report.
    pub count: u32,
    /// False for a reflow, which changes the page numbers and not the place.
    pub moved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// The reader asked the view for a chapter, as load `number`.
    Asked {
        number: u32,
    },
    /// A load is laid out and about to land.
    Ready {
        load: Load,
    },
    Report(Report),
    /// The load being shown cannot be shown, or its page has stopped.
    Failed,
    /// The timer that saves a while after the last move has fired.
    Timer,
    /// The reader is going, by any way.
    Leave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Show this place as where the reader is: the line under the page, and
    /// what a turn starts from.
    Show(Place),
    /// Nothing is showing: a chapter was asked for and has not landed.
    Blank,
    /// Start the save timer, or start it again.
    StartTimer,
    StopTimer,
    /// Hand this place to the library now. Submissions are made in the
    /// order this function answers them.
    Submit(Place),
    /// Wait for the library to have what was submitted.
    AwaitLibrary,
    /// End the page's process.
    EndPage,
}

#[derive(Debug, Default)]
pub struct Keeper {
    /// The place to save, and the order of the move that made it.
    held: Option<(Place, Order)>,
    /// The order of the place last handed to the library.
    submitted: Option<Order>,
    /// The load the reader asked for last, and whether it is ready.
    showing: Option<(u32, bool)>,
    /// What is displayed, and the rank of the report it came from: its
    /// order, and then whether it is a reflow. A reflow repeats the count
    /// of the move it follows, so of two reports with one order the reflow
    /// is the later made.
    displayed: Option<(Place, (Order, bool))>,
    timer: bool,
    gone: bool,
    #[cfg(test)]
    fault: Fault,
}

/// A deliberate mistake, to show the tests can see it. Each is one of the
/// faults a reviewer found in the code this module replaced.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Fault {
    #[default]
    None,
    /// A timed save that is submitted late: after the reader has left.
    LateTimedSave,
    /// Remembering only the load before this one: an answer from any load
    /// older than that is dropped.
    OneEarlierLoad,
    /// Showing a place because its chapter's number is the one showing,
    /// and not because its load is.
    ShownByChapter,
    /// Treating a reflow as a move.
    ReflowIsAMove,
}

impl Keeper {
    /// The place to save, if any move has arrived. The tests read it; the
    /// reader is told it by `Effect::Submit`.
    #[allow(dead_code)]
    pub fn held(&self) -> Option<Place> {
        self.held.map(|(place, _)| place)
    }

    /// The place showing and the load it belongs to, for turning from.
    pub fn displayed(&self) -> Option<(Place, u32)> {
        self.displayed.map(|(place, (order, _))| (place, order.0))
    }

    pub fn gone(&self) -> bool {
        self.gone
    }

    /// Take one thing that happened, and say what to do about it.
    pub fn step(&mut self, input: Input) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.gone {
            #[cfg(test)]
            if self.fault == Fault::LateTimedSave
                && input == Input::Timer
                && let Some((place, _)) = self.held
            {
                effects.push(Effect::Submit(place));
            }
            return effects;
        }
        match input {
            Input::Asked { number } => {
                self.showing = Some((number, false));
                self.displayed = None;
                effects.push(Effect::Blank);
            }
            Input::Ready { load } => {
                if let Some((number, ready)) = &mut self.showing
                    && *number == load.number
                {
                    *ready = true;
                }
            }
            Input::Report(report) => self.report(report, &mut effects),
            Input::Failed => {
                // Nothing is showing, so there is nothing to turn from. The
                // place held is kept: it is still where the reader was.
                if let Some((_, ready)) = &mut self.showing {
                    *ready = false;
                }
                self.displayed = None;
            }
            Input::Timer => {
                self.timer = false;
                self.submit(&mut effects);
            }
            Input::Leave => {
                self.gone = true;
                #[cfg(test)]
                let stop = self.fault != Fault::LateTimedSave;
                #[cfg(not(test))]
                let stop = true;
                if self.timer && stop {
                    self.timer = false;
                    effects.push(Effect::StopTimer);
                }
                // Always, even when the timer has submitted this place: that
                // save may have failed, and its answer may not be in yet.
                if let Some((place, order)) = self.held {
                    self.submitted = Some(order);
                    effects.push(Effect::Submit(place));
                }
                effects.push(Effect::AwaitLibrary);
                effects.push(Effect::EndPage);
            }
        }
        effects
    }

    fn report(&mut self, report: Report, effects: &mut Vec<Effect>) {
        // A report is the page's word, so it is checked: a page inside the
        // page count and an offset inside the chapter its load showed.
        if report.pages == 0 || report.page >= report.pages || report.offset > report.load.length {
            return;
        }
        #[cfg(test)]
        if self.fault == Fault::OneEarlierLoad
            && self
                .showing
                .is_some_and(|(number, _)| report.load.number + 1 < number)
        {
            return;
        }
        let place = Place {
            chapter: report.load.chapter,
            offset: report.offset,
            page: report.page,
            pages: report.pages,
        };
        let order = (report.load.number, report.count);
        #[cfg(test)]
        let moved = report.moved || self.fault == Fault::ReflowIsAMove;
        #[cfg(not(test))]
        let moved = report.moved;
        // What is saved: the newest move, for the chapter it was made in.
        if moved && self.held.is_none_or(|(_, held)| order > held) {
            self.held = Some((place, order));
            self.timer = true;
            effects.push(Effect::StartTimer);
        }
        // What is shown: the load being shown, once ready, and never an
        // older report over a newer one.
        let mine = self
            .showing
            .is_some_and(|(number, ready)| ready && number == report.load.number);
        #[cfg(test)]
        let mine = mine
            || (self.fault == Fault::ShownByChapter
                && self.showing.is_some_and(|(_, ready)| ready)
                && self
                    .displayed
                    .is_none_or(|(shown, _)| shown.chapter == place.chapter));
        // A move's answer that arrives after a reflow made since has the
        // page numbers of before the reflow, and is not shown over it.
        let rank = (order, !report.moved);
        if mine && self.displayed.is_none_or(|(_, shown)| rank >= shown) {
            // A reflow shows the held offset on new page numbers.
            self.displayed = Some((place, rank));
            effects.push(Effect::Show(place));
        }
    }

    /// Hand the held place to the library unless it was handed it before.
    fn submit(&mut self, effects: &mut Vec<Effect>) {
        if let Some((place, order)) = self.held
            && self.submitted != Some(order)
        {
            self.submitted = Some(order);
            effects.push(Effect::Submit(place));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(number: u32, chapter: u32) -> Load {
        Load {
            number,
            chapter,
            length: 10_000,
        }
    }

    fn moved(load: Load, count: u32, offset: u32) -> Input {
        Input::Report(Report {
            load,
            offset,
            page: offset / 1000,
            pages: 11,
            count,
            moved: true,
        })
    }

    #[test]
    fn a_turn_answered_after_the_next_chapter_was_chosen_is_held() {
        let mut k = Keeper::default();
        let a = load(1, 0);
        k.step(Input::Asked { number: 1 });
        k.step(Input::Ready { load: a });
        k.step(moved(a, 1, 0));
        // The turn is made, B is asked for, and then the turn is answered.
        k.step(Input::Asked { number: 2 });
        assert_eq!(k.step(moved(a, 2, 900)), [Effect::StartTimer]);
        assert_eq!(k.held().map(|p| (p.chapter, p.offset)), Some((0, 900)));
        assert_eq!(k.displayed(), None);
        // Leaving now saves the turn, in chapter A.
        let left = k.step(Input::Leave);
        assert!(
            matches!(left[..], [Effect::StopTimer, Effect::Submit(p), Effect::AwaitLibrary, Effect::EndPage] if p.offset == 900 && p.chapter == 0)
        );
    }

    #[test]
    fn a_late_answer_does_not_replace_a_newer_place() {
        let mut k = Keeper::default();
        let (a, b) = (load(1, 0), load(2, 1));
        k.step(Input::Asked { number: 1 });
        k.step(Input::Ready { load: a });
        k.step(moved(a, 1, 0));
        k.step(Input::Asked { number: 2 });
        k.step(Input::Ready { load: b });
        assert_eq!(k.step(moved(b, 1, 0)).len(), 2);
        // A's turn is answered after B has reported.
        assert_eq!(k.step(moved(a, 2, 900)), []);
        assert_eq!(k.held().map(|p| p.chapter), Some(1));
        assert_eq!(k.displayed().map(|(p, l)| (p.chapter, l)), Some((1, 2)));
    }

    #[test]
    fn three_chapters_in_quick_succession_keep_the_turn() {
        let mut k = Keeper::default();
        let (a, b) = (load(1, 0), load(2, 1));
        k.step(Input::Asked { number: 1 });
        k.step(Input::Ready { load: a });
        k.step(moved(a, 1, 0));
        // Turn in A, choose B, B is ready, choose C, and only then A answers.
        k.step(Input::Asked { number: 2 });
        k.step(Input::Ready { load: b });
        k.step(Input::Asked { number: 3 });
        assert_eq!(k.step(moved(a, 2, 900)), [Effect::StartTimer]);
        assert_eq!(k.held().map(|p| (p.chapter, p.offset)), Some((0, 900)));
    }

    #[test]
    fn a_chapter_loaded_twice_shows_only_its_second_load() {
        let mut k = Keeper::default();
        let (first, second) = (load(1, 4), load(2, 4));
        k.step(Input::Asked { number: 1 });
        k.step(Input::Ready { load: first });
        k.step(moved(first, 1, 0));
        k.step(Input::Asked { number: 2 });
        k.step(Input::Ready { load: second });
        // A late report from the first load, of the same chapter number.
        let late = k.step(moved(first, 2, 3000));
        assert!(
            !late.iter().any(|e| matches!(e, Effect::Show(_))),
            "{late:?}"
        );
        assert_eq!(k.displayed(), None);
        let landed = k.step(moved(second, 1, 0));
        assert!(landed.contains(&Effect::Show(Place {
            chapter: 4,
            offset: 0,
            page: 0,
            pages: 11
        })));
    }

    #[test]
    fn a_reflow_changes_what_is_shown_and_nothing_that_is_saved() {
        let mut k = Keeper::default();
        let a = load(1, 0);
        k.step(Input::Asked { number: 1 });
        k.step(Input::Ready { load: a });
        k.step(moved(a, 1, 2000));
        assert_eq!(k.step(Input::Timer).len(), 1);
        let reflow = Input::Report(Report {
            load: a,
            offset: 2000,
            page: 7,
            pages: 40,
            count: 1,
            moved: false,
        });
        assert_eq!(
            k.step(reflow),
            [Effect::Show(Place {
                chapter: 0,
                offset: 2000,
                page: 7,
                pages: 40
            })]
        );
        assert_eq!(k.held().map(|p| (p.page, p.pages)), Some((2, 11)));
        // No timer was started.
        assert_eq!(k.step(Input::Timer), []);
        // Leaving submits the held place again: the timed save may have failed.
        assert!(matches!(
            k.step(Input::Leave)[..],
            [Effect::Submit(p), Effect::AwaitLibrary, Effect::EndPage] if (p.page, p.pages) == (2, 11)
        ));
    }

    #[test]
    fn a_timed_save_and_a_leave_submit_in_the_order_the_places_were_made() {
        let mut k = Keeper::default();
        let a = load(1, 0);
        k.step(Input::Asked { number: 1 });
        k.step(Input::Ready { load: a });
        k.step(moved(a, 1, 0));
        k.step(moved(a, 2, 1000));
        assert!(matches!(k.step(Input::Timer)[..], [Effect::Submit(p)] if p.offset == 1000));
        k.step(moved(a, 3, 2000));
        let left = k.step(Input::Leave);
        assert!(
            matches!(left[..], [Effect::StopTimer, Effect::Submit(p), Effect::AwaitLibrary, Effect::EndPage] if p.offset == 2000)
        );
        // Nothing after leaving does anything: not a late timer, not a report.
        assert_eq!(k.step(Input::Timer), []);
        assert_eq!(k.step(moved(a, 4, 3000)), []);
        assert_eq!(k.step(Input::Leave), []);
    }

    #[test]
    fn a_failed_chapter_shows_nothing_and_keeps_the_place_held() {
        let mut k = Keeper::default();
        let a = load(1, 0);
        k.step(Input::Asked { number: 1 });
        k.step(Input::Ready { load: a });
        k.step(moved(a, 1, 2000));
        assert_eq!(k.step(Input::Failed), []);
        assert_eq!(k.displayed(), None);
        assert_eq!(k.held().map(|p| p.offset), Some(2000));
        // A report that was on its way is still a move, and still not shown.
        assert_eq!(k.step(moved(a, 2, 3000)), [Effect::StartTimer]);
        assert_eq!(k.displayed(), None);
    }

    /// The timer's save may have failed, and its answer may not be in when
    /// the reader leaves, so leaving submits the held place again.
    #[test]
    fn leaving_submits_the_held_place_even_after_the_timer_did() {
        let mut k = Keeper::default();
        let a = load(1, 0);
        k.step(Input::Asked { number: 1 });
        k.step(Input::Ready { load: a });
        k.step(moved(a, 1, 100));
        assert!(matches!(k.step(Input::Timer)[..], [Effect::Submit(p)] if p.offset == 100));
        assert!(matches!(
            k.step(Input::Leave)[..],
            [Effect::Submit(p), Effect::AwaitLibrary, Effect::EndPage] if p.offset == 100
        ));
    }

    /// A turn's answer that arrives after a reflow made since the turn has
    /// the page numbers of before the reflow. It is saved and not shown.
    #[test]
    fn a_move_answered_after_its_reflow_is_not_shown_over_it() {
        let mut k = Keeper::default();
        let a = load(1, 0);
        k.step(Input::Asked { number: 1 });
        k.step(Input::Ready { load: a });
        k.step(moved(a, 1, 0));
        let reflow = Report {
            load: a,
            offset: 2000,
            page: 1,
            pages: 6,
            count: 2,
            moved: false,
        };
        assert_eq!(k.step(Input::Report(reflow)).len(), 1);
        assert_eq!(k.step(moved(a, 2, 2000)), [Effect::StartTimer]);
        assert_eq!(k.displayed().map(|(p, _)| (p.page, p.pages)), Some((1, 6)));
        assert_eq!(k.held().map(|p| p.offset), Some(2000));
    }

    #[test]
    fn a_report_outside_its_chapter_is_ignored() {
        let mut k = Keeper::default();
        let a = load(1, 0);
        k.step(Input::Asked { number: 1 });
        k.step(Input::Ready { load: a });
        for bad in [
            Report {
                load: a,
                offset: 10_001,
                page: 0,
                pages: 1,
                count: 1,
                moved: true,
            },
            Report {
                load: a,
                offset: 0,
                page: 3,
                pages: 3,
                count: 1,
                moved: true,
            },
            Report {
                load: a,
                offset: 0,
                page: 0,
                pages: 0,
                count: 1,
                moved: true,
            },
        ] {
            assert_eq!(k.step(Input::Report(bad)), []);
        }
        assert_eq!(k.held(), None);
    }

    // ---- The random test.

    /// A small generator with a seed, so a failure can be run again.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    /// What must be true, kept the obvious way: by remembering everything.
    #[derive(Default)]
    struct Reference {
        /// Every valid move that has arrived, with its order.
        arrived: Vec<(Place, Order)>,
        asked: Option<u32>,
        /// The load asked for last has said it is ready.
        ready: bool,
        /// Everything valid that load has said since, in the order it came,
        /// with whether it was a reflow.
        said: Vec<(Place, (Order, bool))>,
        submitted: Vec<Place>,
        /// The order of the move each submitted place was made by.
        orders: Vec<Order>,
        left: bool,
    }

    /// Run one random sequence. Returns the first thing that is wrong.
    fn run(seed: u64, fault: Fault) -> Result<(), String> {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut k = Keeper {
            fault,
            ..Keeper::default()
        };
        let mut reference = Reference::default();
        // Loads made so far, the moves each has made, and whether it is ready.
        let mut loads: Vec<(Load, u32, bool)> = Vec::new();
        // Answers the page has made that have not arrived yet.
        let mut in_flight: Vec<Report> = Vec::new();
        let mut timer = false;
        let steps = 20 + rng.below(60);
        for step in 0..steps {
            let leave_now = step + 1 == steps;
            let input = match rng.below(100) {
                _ if leave_now => Input::Leave,
                // Ask for a chapter. Few chapters, so numbers repeat.
                0..=14 => {
                    let number = loads.len() as u32 + 1;
                    let load = Load {
                        number,
                        chapter: rng.below(3) as u32,
                        length: 5000,
                    };
                    loads.push((load, 0, false));
                    Input::Asked { number }
                }
                // A load becomes ready and lands: its first move.
                15..=29 if loads.iter().any(|l| !l.2) => {
                    let pending: Vec<usize> = (0..loads.len()).filter(|i| !loads[*i].2).collect();
                    let at = pending[rng.below(pending.len() as u64) as usize];
                    loads[at].2 = true;
                    loads[at].1 = 1;
                    let pages = 1 + rng.below(9) as u32;
                    in_flight.push(Report {
                        load: loads[at].0,
                        offset: rng.below(5001) as u32,
                        page: rng.below(u64::from(pages)) as u32,
                        pages,
                        count: 1,
                        moved: true,
                    });
                    Input::Ready { load: loads[at].0 }
                }
                // A ready load makes a move, or reflows. Its answer is in flight.
                30..=59 if loads.iter().any(|l| l.2) => {
                    let ready: Vec<usize> = (0..loads.len()).filter(|i| loads[*i].2).collect();
                    let at = ready[rng.below(ready.len() as u64) as usize];
                    let is_move = rng.below(4) != 0;
                    if is_move {
                        loads[at].1 += 1;
                    }
                    // A reflow has its own page count, as a resize gives.
                    let pages = 1 + rng.below(9) as u32;
                    in_flight.push(Report {
                        load: loads[at].0,
                        // Now and then the page says something impossible.
                        offset: rng.below(5200) as u32,
                        page: rng.below(u64::from(pages) + 1) as u32,
                        pages,
                        count: loads[at].1,
                        moved: is_move,
                    });
                    continue;
                }
                // An answer arrives: any of those in flight, in any order.
                // A turn's answer comes back on its own, so it can pass or be
                // passed by anything. Reflows are all told by one path, the
                // page's messages, so a load's reflows arrive in the order
                // they were made: the oldest of them is the one delivered.
                60..=89 if !in_flight.is_empty() => {
                    let mut at = rng.below(in_flight.len() as u64) as usize;
                    let chosen = in_flight[at];
                    if !chosen.moved {
                        at = in_flight
                            .iter()
                            .position(|r| !r.moved && r.load == chosen.load)
                            .unwrap_or(at);
                    }
                    Input::Report(in_flight.remove(at))
                }
                90..=96 if timer => Input::Timer,
                97 => Input::Failed,
                98 => Input::Leave,
                _ => continue,
            };
            // The reference takes the input first.
            let before_held = k.held();
            let timer_before = timer;
            let before_submitted = reference.submitted.len();
            match input {
                // A reader that has left asks for nothing.
                Input::Asked { number } if !reference.left => {
                    reference.asked = Some(number);
                    reference.ready = false;
                    reference.said.clear();
                }
                Input::Ready { load } if !reference.left => {
                    reference.ready |= reference.asked == Some(load.number);
                }
                Input::Failed if !reference.left => {
                    reference.ready = false;
                    reference.said.clear();
                }
                Input::Report(r) if !reference.left => {
                    let valid = r.pages > 0 && r.page < r.pages && r.offset <= r.load.length;
                    let place = Place {
                        chapter: r.load.chapter,
                        offset: r.offset,
                        page: r.page,
                        pages: r.pages,
                    };
                    let order = (r.load.number, r.count);
                    if valid && r.moved {
                        reference.arrived.push((place, order));
                    }
                    if valid && reference.ready && reference.asked == Some(r.load.number) {
                        reference.said.push((place, (order, !r.moved)));
                    }
                }
                _ => {}
            }
            let effects = k.step(input);
            for effect in &effects {
                match effect {
                    Effect::StartTimer => timer = true,
                    Effect::StopTimer => timer = false,
                    Effect::Submit(place) => {
                        if reference.left {
                            return Err(format!("step {step}: a save was submitted after leaving"));
                        }
                        // Every save is of the newest move to have arrived.
                        let newest = reference.arrived.iter().max_by_key(|(_, order)| *order);
                        let Some((_, order)) = newest.filter(|(held, _)| held == place) else {
                            return Err(format!(
                                "step {step}: {place:?} was submitted, and the newest move is {newest:?}"
                            ));
                        };
                        reference.submitted.push(*place);
                        reference.orders.push(*order);
                    }
                    _ => {}
                }
            }
            if input == Input::Timer {
                timer = false;
            }
            // 1. The place held is the most recently made move that arrived.
            let newest = reference
                .arrived
                .iter()
                .max_by_key(|(_, order)| *order)
                .map(|(place, _)| *place);
            if !reference.left && k.held() != newest {
                return Err(format!(
                    "step {step}: held {:?}, the newest move to arrive is {newest:?}",
                    k.held()
                ));
            }
            // 2. What is displayed belongs to the load being shown.
            if let Some((_, shown)) = k.displayed()
                && Some(shown) != reference.asked
            {
                return Err(format!(
                    "step {step}: a place of load {shown} is displayed while load {:?} is showing",
                    reference.asked
                ));
            }
            // It is the newest thing that load has said since it was ready.
            let said = reference
                .said
                .iter()
                .max_by_key(|(_, rank)| *rank)
                .map(|(place, _)| *place);
            // The reader is told what to show, so that is checked and not
            // only what the keeper remembers: one `Show` when it changes.
            let shows: Vec<Effect> = effects
                .iter()
                .copied()
                .filter(|e| matches!(e, Effect::Show(_) | Effect::Blank))
                .collect();
            let expected: Vec<Effect> = match input {
                Input::Asked { .. } if !reference.left => vec![Effect::Blank],
                Input::Report(r)
                    if !reference.left
                        && reference.said.last().is_some_and(|(_, rank)| {
                            *rank == ((r.load.number, r.count), !r.moved)
                                && reference.said.iter().all(|(_, other)| other <= rank)
                        })
                        && reference.asked == Some(r.load.number)
                        && r.pages > 0
                        && r.page < r.pages
                        && r.offset <= r.load.length =>
                {
                    said.map(Effect::Show).into_iter().collect()
                }
                _ => Vec::new(),
            };
            if shows != expected {
                return Err(format!(
                    "step {step}: the reader was told {shows:?}, and should be told {expected:?}"
                ));
            }
            if !reference.left && k.displayed().map(|(place, _)| place) != said {
                return Err(format!(
                    "step {step}: {:?} is displayed, and the load showing last said {said:?}",
                    k.displayed()
                ));
            }
            // 3. A report that is not a move changes nothing that is saved.
            if let Input::Report(Report { moved: false, .. }) = input
                && (k.held() != before_held
                    || reference.submitted.len() != before_submitted
                    || effects.contains(&Effect::StartTimer))
            {
                return Err(format!(
                    "step {step}: a reflow changed what is saved: {effects:?}"
                ));
            }
            // 4. Leaving submits the held place last, waits, and ends the page.
            if input == Input::Leave && !reference.left {
                reference.left = true;
                // Exactly this: the held place, submitted once whether or
                // not the timer submitted it before, then the wait, then
                // the page's end.
                let mut expected: Vec<Effect> = Vec::new();
                // The timer is stopped first, once, if and only if it runs.
                if timer_before {
                    expected.push(Effect::StopTimer);
                }
                expected.extend(newest.map(Effect::Submit));
                expected.extend([Effect::AwaitLibrary, Effect::EndPage]);
                if effects != expected {
                    return Err(format!(
                        "step {step}: leaving did {effects:?}, with {newest:?} held"
                    ));
                }
            }
        }
        // 5. Saves were submitted in the order their places were made.
        let orders = &reference.orders;
        if orders.windows(2).any(|pair| pair[0] > pair[1]) {
            return Err(format!("saves were submitted out of order: {orders:?}"));
        }
        // A timer left running after leaving can still fire.
        if timer && reference.left && !k.step(Input::Timer).is_empty() {
            return Err("a save was submitted after leaving".into());
        }
        Ok(())
    }

    const SEQUENCES: u64 = 20_000;

    #[test]
    fn random_sequences_keep_the_rule() {
        let started = std::time::Instant::now();
        for seed in 0..SEQUENCES {
            if let Err(why) = run(seed, Fault::None) {
                panic!("seed {seed}: {why}");
            }
        }
        println!("{SEQUENCES} sequences in {:?}", started.elapsed());
    }

    /// Each fault a reviewer found in the code this replaced is caught.
    #[test]
    fn the_random_test_catches_each_known_fault() {
        for fault in [
            Fault::LateTimedSave,
            Fault::OneEarlierLoad,
            Fault::ShownByChapter,
            Fault::ReflowIsAMove,
        ] {
            let caught =
                (0..SEQUENCES).find_map(|seed| run(seed, fault).err().map(|why| (seed, why)));
            let Some((seed, why)) = caught else {
                panic!("{fault:?} was not caught in {SEQUENCES} sequences");
            };
            println!("{fault:?}: caught at seed {seed}: {why}");
        }
    }
}

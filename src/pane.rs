//! **What a pane rebuilt, and when it did not have to**
//! (`DATA_PALETTE_CLEANUP.md` §Frames).
//!
//! # The measurement this exists for
//!
//! The frame loop is already idle-until-work: a settled scene asks for no
//! successor (`SceneModel::frame_cost::a_quiet_scene_does_not_ask_for_another_frame`),
//! so nothing redraws on a timer. What the waker does NOT do is bound how much
//! of a frame is rebuilt. Any wake - a keystroke, a pointer move mid-drag, a
//! pipeline answer - rebuilds the whole `DrawList`, including the panes that did
//! not move. Tim:
//!
//! > the goal is to reduce actual draws, not extra memoizations, think of it
//! > like vdom in React ... we want to gate something that fully draws to the
//! > gpu
//!
//! # Why this is not a memo, and must not become one
//!
//! React re-renders and compares because the browser will not say what changed.
//! We know: every write goes through a door we own. So a write MARKS its pane
//! ([`PaneMark::mark`]) and the composition splices what the pane drew last time
//! - no hashing, no diff, and above all no walk of the tree to ask "did anything
//! change". That question is what cost `DATA_PALETTE_CLEANUP.md` §Frames' memo
//! 106us to answer, five times the 21us it was avoiding.
//!
//! [`PaneKey`] is the other half and is deliberately NOT that walk: it is the
//! handful of SCALARS a pane is handed per frame (its rect, which screen it is
//! on, whether a keyboard is up), compared by `Eq`. That is what a React
//! `memo()` compares, and it is nanoseconds. Anything that would need a
//! traversal to compute belongs on the mark instead - the write that caused it
//! knows, and the reader should not have to go looking.
//!
//! # Why a revision rather than a boolean
//!
//! The pane's own render MUTATES: drawing the hi-fi phone re-solves its layout
//! and writes scroll bounds back into the app's world. A boolean set by the
//! writer and cleared by the reader cannot tell that from a real edit, so the
//! flag would be set by the very act of answering it. A revision has no such
//! ambiguity: [`PaneGate::draw`] records the revision the kept ink was drawn AT,
//! and a later frame is a hit exactly while nothing has moved it since.
//!
//! # Why this lives in libmsdf
//!
//! Because [`DrawList::append`] does, and this is the only reason that method
//! exists: keeping one pane's ink and splicing it is a draw-list operation, and
//! the rebasing it needs is private to the list. It is also the layer at which a
//! pane is only a RANGE OF INSTANCES - this module knows nothing about panes,
//! phones or apps, which is what lets a second surface use it unchanged.
//!
//! It is not here to dodge `libhbui`'s drawing allowlist, and that gate's
//! finding is recorded rather than worked around: its needle `DrawList::new(`
//! stands for "this module emits instances", and a module that CONSTRUCTS a
//! list for somebody else to fill falsifies that proxy. The airtight half of the
//! rule is the push family; [`DrawList::append`] cannot originate an instance,
//! only relocate one that some module the gate already reads pushed.
//!
//! # An unmarked write is a STALE PANE
//!
//! That is the failure mode, and it looks like a win - a cheaper frame, showing
//! the wrong thing. [`PaneGate`] therefore carries a verifier
//! ([`PaneGate::with_audit`]): it rebuilds anyway and asserts the kept ink still
//! equals what a fresh build produces, reporting the pane by name when it does
//! not. It is what turns every existing test and every review frame into a check
//! for a write nobody marked, rather than leaving that to a grep.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::DrawList;

/// **A pane's dirty mark**: a revision that every write which could change what
/// the pane draws moves forward.
///
/// Shared (`Rc`) because the writer and the composer are rarely the same value:
/// the hi-fi phone's mark is moved by input routed into the app, by the bundle
/// being replaced, by the scheme flipping - and read by the frame composition.
///
/// Cheap on purpose. Marking is a `Cell` increment, so no caller ever has a
/// reason to be selective about calling it: **marking too often costs one
/// rebuild, marking too rarely draws the wrong frame.** Where the two are in
/// doubt, mark.
#[derive(Clone, Debug)]
pub struct PaneMark {
    rev: Rc<Cell<u64>>,
}

impl Default for PaneMark {
    fn default() -> Self {
        Self::new()
    }
}

impl PaneMark {
    /// A mark at its first revision.
    pub fn new() -> Self {
        Self { rev: Rc::new(Cell::new(1)) }
    }

    /// **A write happened.** Anything the pane draws may have changed.
    pub fn mark(&self) {
        self.rev.set(self.rev.get().wrapping_add(1));
    }

    /// The revision the pane's inputs stand at.
    pub fn revision(&self) -> u64 {
        self.rev.get()
    }
}

/// What a pane was handed this frame, beside its durable model: the scalars a
/// composition passes down and can compare by `Eq`.
///
/// A `PaneKey` is a bound on the SIZE of what may be compared, not a place to
/// put anything a pane reads. See the module doc: a value that would need a
/// walk to compute belongs on [`PaneMark`].
pub trait PaneKey: PartialEq {}

impl<T: PartialEq> PaneKey for T {}

/// **One pane's kept ink**, and the gate that decides whether to draw it again.
///
/// `K` is the pane's per-frame key ([`PaneKey`]). Held by whatever owns the
/// pane's durable model, so the ink outlives the frame that produced it.
pub struct PaneGate<K> {
    kept: RefCell<Option<Kept<K>>>,
    rebuilds: Cell<usize>,
    skips: Cell<usize>,
    /// The pane's name, for what an audit failure says.
    name: &'static str,
    /// Whether to rebuild on a HIT as well and assert the two agree - the
    /// unmarked-write check. See [`PaneGate::with_audit`].
    audit: bool,
}

struct Kept<K> {
    key: K,
    rev: u64,
    ink: DrawList,
}

impl<K: PaneKey> PaneGate<K> {
    /// A gate for the pane called `name`, holding nothing yet.
    pub fn new(name: &'static str) -> Self {
        Self {
            kept: RefCell::new(None),
            rebuilds: Cell::new(0),
            skips: Cell::new(0),
            name,
            audit: false,
        }
    }

    /// **...that rebuilds on every hit and asserts the kept ink still agrees**,
    /// so a write nobody marked panics where it happened instead of drawing a
    /// stale pane.
    ///
    /// It costs the whole saving, which is the point: this is the mode a test
    /// suite runs in, never the mode a frame ships in. The counters still report
    /// the skip that WOULD have happened, so a test can assert the gate hit and
    /// that the answer was right in the same run.
    pub fn with_audit(mut self, audit: bool) -> Self {
        self.audit = audit;
        self
    }

    /// **Draw the pane into `out`, or splice what it drew last time.**
    ///
    /// `build` renders the pane into a list of its OWN - never into `out`
    /// directly - which is what makes the result keepable. Both paths then go
    /// through [`DrawList::append`], so the splice is exercised on a miss as
    /// well as a hit and a rebasing bug cannot hide until the first skip.
    pub fn draw(&self, mark: &PaneMark, key: K, out: &mut DrawList, build: impl FnOnce(&mut DrawList)) {
        let rev = mark.revision();
        let hit = self
            .kept
            .borrow()
            .as_ref()
            .is_some_and(|kept| kept.rev == rev && kept.key == key);
        if hit {
            self.skips.set(self.skips.get() + 1);
            if self.audit {
                let mut fresh = DrawList::new();
                build(&mut fresh);
                let kept = self.kept.borrow();
                let ink = &kept.as_ref().expect("just checked").ink;
                assert_eq!(
                    ink.instances,
                    fresh.instances,
                    "pane `{}` was skipped as clean and is NOT: something wrote to it \
                     without moving its PaneMark, so a shipping frame would draw this \
                     pane's previous ink",
                    self.name,
                );
                assert_eq!(ink.chars(), fresh.chars(), "pane `{}`: kept glyphs are stale", self.name);
                out.append(ink);
                return;
            }
            out.append(&self.kept.borrow().as_ref().expect("just checked").ink);
            return;
        }
        let mut ink = DrawList::new();
        build(&mut ink);
        out.append(&ink);
        self.rebuilds.set(self.rebuilds.get() + 1);
        *self.kept.borrow_mut() = Some(Kept { key, rev, ink });
    }

    /// **How many times the pane was actually drawn.**
    ///
    /// Counted and exposed because "it skipped the rebuild" is vacuous on its
    /// own: a pane that would have drawn nothing skips perfectly. A test that
    /// asserts a skip has to assert this was NON-ZERO first.
    pub fn rebuilds(&self) -> usize {
        self.rebuilds.get()
    }

    /// How many times the kept ink answered instead.
    pub fn skips(&self) -> usize {
        self.skips.get()
    }

    /// Forget the kept ink - for a host that has replaced the pane's model
    /// outright and would rather not reason about the mark.
    pub fn forget(&self) {
        *self.kept.borrow_mut() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SdfInstance, SdfKind};

    fn boxy(x: f32) -> SdfInstance {
        SdfInstance {
            kind: SdfKind::Box,
            position: [x, 0.0],
            size: [4.0, 4.0],
            color: [1.0; 4],
            anim: 0,
        }
    }

    /// The whole gate in one frame pair: the first frame DRAWS (non-zero
    /// rebuilds, so the skip below is not a skip of nothing), the second frame
    /// skips, and the ink is the same either way.
    #[test]
    fn a_clean_pane_is_spliced_and_the_first_frame_is_not() {
        let gate: PaneGate<u32> = PaneGate::new("test");
        let mark = PaneMark::new();
        let mut drawn = 0;

        let mut first = DrawList::new();
        gate.draw(&mark, 7, &mut first, |ink| {
            drawn += 1;
            ink.push(boxy(0.0));
        });
        assert_eq!(gate.rebuilds(), 1, "the first frame must actually draw");
        assert_eq!(drawn, 1);

        let mut second = DrawList::new();
        gate.draw(&mark, 7, &mut second, |ink| {
            drawn += 1;
            ink.push(boxy(0.0));
        });
        assert_eq!(gate.rebuilds(), 1, "nothing moved: the pane must not draw again");
        assert_eq!(gate.skips(), 1);
        assert_eq!(drawn, 1, "the build closure must not have been entered");
        assert_eq!(first.instances, second.instances);
    }

    /// A write moves the mark and the pane draws again - with the NEW ink.
    #[test]
    fn a_marked_pane_draws_again() {
        let gate: PaneGate<u32> = PaneGate::new("test");
        let mark = PaneMark::new();
        let mut x = 0.0;

        let mut first = DrawList::new();
        gate.draw(&mark, 7, &mut first, |ink| ink.push(boxy(x)));
        x = 50.0;
        mark.mark();
        let mut second = DrawList::new();
        gate.draw(&mark, 7, &mut second, |ink| ink.push(boxy(x)));

        assert_eq!(gate.rebuilds(), 2);
        assert_eq!(gate.skips(), 0);
        assert_eq!(second.instances[0].position[0], 50.0);
    }

    /// The key is the other half. A pane handed a different rect draws again
    /// even though nothing wrote to its model.
    #[test]
    fn a_changed_key_draws_again_with_the_mark_unmoved() {
        let gate: PaneGate<u32> = PaneGate::new("test");
        let mark = PaneMark::new();
        let mut list = DrawList::new();
        gate.draw(&mark, 7, &mut list, |ink| ink.push(boxy(0.0)));
        gate.draw(&mark, 8, &mut list, |ink| ink.push(boxy(0.0)));
        assert_eq!(gate.rebuilds(), 2);
        assert_eq!(mark.revision(), 1, "nothing wrote to the model");
    }

    /// **The stale pane, caught.** A write that does NOT move the mark is the
    /// one failure this design can have, and the audit is what turns it from a
    /// wrong frame into a panic that names the pane.
    #[test]
    #[should_panic(expected = "pane `phone` was skipped as clean and is NOT")]
    fn an_unmarked_write_is_caught_by_the_audit() {
        let gate: PaneGate<u32> = PaneGate::new("phone").with_audit(true);
        let mark = PaneMark::new();
        let mut x = 0.0;

        let mut list = DrawList::new();
        gate.draw(&mark, 7, &mut list, |ink| ink.push(boxy(x)));
        // The write nobody marked.
        x = 50.0;
        gate.draw(&mark, 7, &mut list, |ink| ink.push(boxy(x)));
    }

    /// ...and the audit is not a blanket panic: an honestly clean pane passes
    /// it, and still reports the skip it would have taken.
    #[test]
    fn the_audit_passes_a_pane_that_really_is_clean() {
        let gate: PaneGate<u32> = PaneGate::new("phone").with_audit(true);
        let mark = PaneMark::new();
        let mut list = DrawList::new();
        gate.draw(&mark, 7, &mut list, |ink| ink.push(boxy(0.0)));
        gate.draw(&mark, 7, &mut list, |ink| ink.push(boxy(0.0)));
        assert_eq!(gate.skips(), 1);
        assert_eq!(list.instances.len(), 2, "the pane is still composed on a skip");
    }
}

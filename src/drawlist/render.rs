//! **Something that draws itself into a [`DrawList`]** (the Render trait
//! plan, `docs/design/20261021_02_RENDER_TRAIT_PLAN.md`).

use super::DrawList;

/// Something that draws itself into a [`DrawList`].
///
/// `render_into` appends this thing's drawing to `out` and never clears it.
/// The name says where the drawing goes, nothing more: there is no context
/// and no clip. A renderer that needs fonts, atlases, a rectangle or a time
/// holds them, or is handed them by its own methods; a caller clips with
/// [`DrawList::push_clip`] around the call (see "Open scopes").
///
/// `render_mut` is for a renderer that settles state as it draws (a measured
/// layout, a scroll bound written back). It is `render_into` unless
/// overridden. **After `render_mut`, a `render_into` from the same state
/// draws the same thing**: hosts with only `&self` access (a read-only pass,
/// a gate's audit) rely on it.
///
/// # Open scopes
///
/// What a renderer draws with `push_*` calls inherits any clip, effect or
/// rotation the caller has open. Ink appended with
/// [`DrawList::append_from`] (a kept list, as [`DrawList`]'s own
/// implementation does) carries its own scopes and must be rendered at top
/// level: `append_from` asserts the destination has none open.
///
/// # Names
///
/// `render_into` and `render_mut` are spelled so a source-level check can
/// find them, as [`DrawList::append_from`] is: neither collides with a
/// common std method.
///
/// ```
/// use libmsdf::Render;
/// use libmsdf::drawlist::DrawList;
///
/// struct Underline { from: [f32; 2], to: [f32; 2] }
///
/// impl Render for Underline {
///     fn render_into(&self, out: &mut DrawList) {
///         out.push_line(self.from, self.to, 2.0, [1.0, 1.0, 1.0, 1.0]);
///     }
/// }
///
/// let mut list = DrawList::new();
/// Underline { from: [0.0, 10.0], to: [40.0, 10.0] }.render_into(&mut list);
/// assert_eq!(list.instances.len(), 1);
/// ```
pub trait Render {
    /// Append this thing's drawing to `out`, never clearing it.
    fn render_into(&self, out: &mut DrawList);

    /// As [`Render::render_into`], for a renderer that settles state as it
    /// draws; by default exactly `render_into`.
    fn render_mut(&mut self, out: &mut DrawList) {
        self.render_into(out)
    }
}

/// A kept list renders by appending itself ([`DrawList::append_from`]'s
/// rules apply: at top level only).
impl Render for DrawList {
    fn render_into(&self, out: &mut DrawList) {
        out.append_from(self)
    }
}

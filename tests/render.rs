//! The Render trait (docs/design/20261021_02_RENDER_TRAIT_PLAN.md, section 3).

use std::cell::Cell;

use libmsdf::Render;
use libmsdf::drawlist::DrawList;

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Two lines from its position.
struct Mark {
    at: [f32; 2],
}

impl Render for Mark {
    fn render_into(&self, out: &mut DrawList) {
        let [x, y] = self.at;
        out.push_line([x, y], [x + 10.0, y], 1.0, WHITE);
        out.push_line([x, y], [x, y + 10.0], 1.0, WHITE);
    }
}

/// A renderer that settles a width the first time it draws, then draws from
/// it; `render_into` reads what `render_mut` settled.
struct Measured {
    text_len: usize,
    width: Option<f32>,
    measured: Cell<u32>,
}

impl Render for Measured {
    fn render_into(&self, out: &mut DrawList) {
        let w = self.width.unwrap_or(self.text_len as f32 * 8.0);
        out.push_line([0.0, 0.0], [w, 0.0], 1.0, WHITE);
    }

    fn render_mut(&mut self, out: &mut DrawList) {
        if self.width.is_none() {
            self.width = Some(self.text_len as f32 * 8.0);
            self.measured.set(self.measured.get() + 1);
        }
        self.render_into(out);
    }
}

#[test]
fn render_into_appends_without_clearing() {
    let mut list = DrawList::new();
    list.push_line([0.0, 0.0], [5.0, 5.0], 1.0, WHITE);
    let before = list.instances[0];
    Mark { at: [20.0, 20.0] }.render_into(&mut list);
    assert_eq!(list.instances.len(), 3);
    assert_eq!(list.instances[0], before);
    assert_eq!(list.instances[1].position, [20.0, 20.0]);
}

#[test]
fn render_mut_defaults_to_render_into() {
    let mut m = Mark { at: [3.0, 4.0] };
    let (mut a, mut b) = (DrawList::new(), DrawList::new());
    m.render_into(&mut a);
    m.render_mut(&mut b);
    assert_eq!(a.instances, b.instances);
    assert_eq!(a.chars(), b.chars());
}

#[test]
fn render_into_after_render_mut_draws_the_same() {
    let mut m = Measured {
        text_len: 5,
        width: None,
        measured: Cell::new(0),
    };
    let (mut a, mut b) = (DrawList::new(), DrawList::new());
    m.render_mut(&mut a);
    m.render_into(&mut b);
    assert_eq!(a.instances, b.instances);
    // Settled once, not again.
    m.render_mut(&mut DrawList::new());
    assert_eq!(m.measured.get(), 1);
}

#[test]
fn a_kept_list_renders_as_append_from() {
    let mut kept = DrawList::new();
    Mark { at: [1.0, 2.0] }.render_into(&mut kept);
    let (mut a, mut b) = (DrawList::new(), DrawList::new());
    a.push_line([9.0, 9.0], [10.0, 10.0], 1.0, WHITE);
    b.push_line([9.0, 9.0], [10.0, 10.0], 1.0, WHITE);
    kept.render_into(&mut a);
    b.append_from(&kept);
    assert_eq!(a.instances, b.instances);
    assert_eq!(a.chars(), b.chars());
    assert_eq!(a.path_segments(), b.path_segments());
}

#[test]
#[should_panic]
fn a_kept_list_inside_an_open_clip_panics() {
    let mut kept = DrawList::new();
    Mark { at: [1.0, 2.0] }.render_into(&mut kept);
    let mut out = DrawList::new();
    out.push_clip([0.0, 0.0], [50.0, 50.0]);
    kept.render_into(&mut out);
}

#[test]
fn a_drawing_renderer_is_clipped_by_the_callers_clip() {
    let mut out = DrawList::new();
    out.push_clip([0.0, 0.0], [50.0, 40.0]);
    Mark { at: [5.0, 5.0] }.render_into(&mut out);
    out.push_clip_end();
    for i in 0..out.instances.len() {
        let c = out.instance_clip(i).expect("clipped");
        assert_eq!(c.min, [0.0, 0.0]);
        assert_eq!(c.max, [50.0, 40.0]);
    }
}

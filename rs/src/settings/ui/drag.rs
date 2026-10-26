//! The grip on a priority row: press, drag up or down, release to move the row there. Plain mouse
//! events, no NSDraggingSession; the row's new place comes from `drop_target`, the move itself from
//! the caller's closure.

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSColor, NSCursor, NSEvent, NSImageView, NSStackView, NSView};
use objc2_foundation::NSRect;

use super::widgets::{size, symbol_image, to_view};

/// Where a row dragged from `from` and released at `y` lands, as `DeviceList::move_listed`'s `to`
/// (the insertion point before removal). `rows` are each row's `(min_y, max_y)` in the container's
/// coordinates, in list order; `flipped` is whether y grows downward there. `None` when the row
/// would stay where it is.
pub fn drop_target(rows: &[(f64, f64)], from: usize, y: f64, flipped: bool) -> Option<usize> {
    if from >= rows.len() {
        return None;
    }
    // The insertion point is after every row whose middle the pointer has passed; above the first
    // row that is 0, below the last it is `rows.len()`, so a release out of range clamps.
    let to = rows
        .iter()
        .take_while(|(lo, hi)| {
            let mid = (lo + hi) / 2.0;
            if flipped { mid < y } else { mid > y }
        })
        .count();
    (to != from && to != from + 1).then_some(to)
}

fn parent(v: &NSView) -> Option<Retained<NSView>> {
    // SAFETY: the result is retained here, and only read on the main thread.
    unsafe { v.superview() }
}

pub struct HandleIvars {
    from: usize,
    on_drop: Box<dyn Fn(usize)>,
    dragging: Cell<bool>,
}

define_class!(
    /// The `line.3.horizontal` grip. An image view itself, so it is the view the mouse lands on.
    #[unsafe(super(NSImageView))]
    #[thread_kind = MainThreadOnly]
    #[name = "CleatRsDragHandle"]
    #[ivars = HandleIvars]
    pub struct DragHandle;

    impl DragHandle {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            self.ivars().dragging.set(false);
            NSCursor::closedHandCursor().push();
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) {
            if !self.ivars().dragging.replace(true) {
                if let Some(row) = parent(self) {
                    row.setAlphaValue(0.5);
                }
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            let i = self.ivars();
            NSCursor::pop_class();
            if !i.dragging.replace(false) {
                return;
            }
            let Some(row) = parent(self) else { return };
            row.setAlphaValue(1.0);
            let Some(container) = parent(&row) else { return };
            // The page was rebuilt mid-drag and this grip is gone: its coordinates mean nothing, drop the release.
            if container.window().is_none() {
                return;
            }
            let y = container.convertPoint_fromView(event.locationInWindow(), None).y;
            // The container also holds the hairlines between rows; only the rows are stacks.
            let frames: Vec<(f64, f64)> = container
                .subviews()
                .iter()
                .filter(|v| v.downcast_ref::<NSStackView>().is_some())
                .map(|v| {
                    let f = v.frame();
                    (f.origin.y, f.origin.y + f.size.height)
                })
                .collect();
            if let Some(to) = drop_target(&frames, i.from, y, container.isFlipped()) {
                (i.on_drop)(to);
            }
        }

        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            self.addCursorRect_cursor(self.bounds(), &NSCursor::openHandCursor());
        }

        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            false
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }
    }
);

impl DragHandle {
    /// `from` is the row's index among the card's rows; `on_drop` gets the `to` for `move_listed`.
    pub fn view(mtm: MainThreadMarker, from: usize, color: &NSColor, on_drop: impl Fn(usize) + 'static) -> Retained<NSView> {
        let this = Self::alloc(mtm).set_ivars(HandleIvars { from, on_drop: Box::new(on_drop), dragging: Cell::new(false) });
        // SAFETY: NSImageView's designated initialiser.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        if let Some(img) = symbol_image("line.3.horizontal", 14.0 * 0.75) {
            view.setImage(Some(&img));
        }
        view.setContentTintColor(Some(color));
        size(&view, 14.0, 14.0);
        to_view(&view.into_super())
    }
}

#[cfg(test)]
mod tests {
    use super::drop_target;

    // Three 50 pt rows, flipped (y down): 0..50, 50..100, 100..150.
    const ROWS: [(f64, f64); 3] = [(0.0, 50.0), (50.0, 100.0), (100.0, 150.0)];

    #[test]
    fn dragged_down_lands_after_the_row_under_the_pointer() {
        assert_eq!(drop_target(&ROWS, 0, 140.0, true), Some(3));
        assert_eq!(drop_target(&ROWS, 0, 80.0, true), Some(2));
    }

    #[test]
    fn dragged_up_lands_before_the_row_under_the_pointer() {
        assert_eq!(drop_target(&ROWS, 2, 10.0, true), Some(0));
        assert_eq!(drop_target(&ROWS, 2, 60.0, true), Some(1));
    }

    #[test]
    fn released_where_it_started_is_no_move() {
        assert_eq!(drop_target(&ROWS, 1, 75.0, true), None);
        assert_eq!(drop_target(&ROWS, 1, 30.0, true), None); // above its own middle but not past the row above's
        assert_eq!(drop_target(&ROWS, 1, 120.0, true), None); // below its middle, before the next one's
        assert_eq!(drop_target(&ROWS, 0, 10.0, true), None);
        assert_eq!(drop_target(&ROWS, 2, 140.0, true), None);
    }

    #[test]
    fn released_outside_the_rows_clamps_to_the_ends() {
        assert_eq!(drop_target(&ROWS, 2, -500.0, true), Some(0));
        assert_eq!(drop_target(&ROWS, 0, 900.0, true), Some(3));
    }

    #[test]
    fn unflipped_container_counts_from_the_top_edge() {
        // Same rows, y up: row 0 on top at 100..150.
        let rows = [(100.0, 150.0), (50.0, 100.0), (0.0, 50.0)];
        assert_eq!(drop_target(&rows, 0, 10.0, false), Some(3));
        assert_eq!(drop_target(&rows, 2, 140.0, false), Some(0));
        assert_eq!(drop_target(&rows, 1, 75.0, false), None);
        assert_eq!(drop_target(&rows, 2, -500.0, false), None);
    }

    #[test]
    fn from_outside_the_list_is_no_move() {
        assert_eq!(drop_target(&ROWS, 3, 10.0, true), None);
        assert_eq!(drop_target(&[], 0, 10.0, true), None);
    }
}

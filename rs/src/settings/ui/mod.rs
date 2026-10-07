//! The window's controller: owns the store, the current page and the views, rebuilds the detail
//! pane after each edit, and saves 400 ms after the last one.

mod devices;
mod levels;
mod pages;
pub mod widgets;
pub mod window;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSScrollView, NSView, NSWindow};
use objc2_foundation::NSTimer;

use super::sources;
use super::store::Store;
use super::text::Page;
use widgets::{Action, Ctx, FlippedView};

pub struct App {
    pub mtm: MainThreadMarker,
    pub store: RefCell<Store>,
    pub page: Cell<Page>,
    pub others_expanded: Cell<bool>,
    pub window: RefCell<Option<Retained<NSWindow>>>,
    pub detail: RefCell<Option<Retained<NSScrollView>>>,
    targets: RefCell<Vec<Retained<Action>>>,
    save_timer: RefCell<Option<Retained<NSTimer>>>,
    live_key: RefCell<String>,
}

thread_local! {
    static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) };
}

/// The one App of this process; only called on the main thread after `install`.
pub fn app() -> Rc<App> {
    APP.with(|a| a.borrow().clone().expect("settings app installed"))
}

impl App {
    pub fn install(mtm: MainThreadMarker) -> Rc<App> {
        let a = Rc::new(App {
            mtm,
            store: RefCell::new(Store::new()),
            page: Cell::new(Page::Output),
            others_expanded: Cell::new(false),
            window: RefCell::new(None),
            detail: RefCell::new(None),
            targets: RefCell::new(vec![]),
            save_timer: RefCell::new(None),
            live_key: RefCell::new(String::new()),
        });
        APP.with(|slot| *slot.borrow_mut() = Some(a.clone()));
        a
    }

    /// Reads the file and the devices now; the pairing list arrives on a worker thread.
    pub fn load(&self) {
        let ok = self.store.borrow_mut().begin_load();
        self.render();
        if ok {
            std::thread::spawn(|| {
                let paired = sources::paired_devices();
                DispatchQueue::main().exec_async(move || {
                    let a = app();
                    a.store.borrow_mut().apply_paired(&paired);
                    a.render();
                });
            });
        }
    }

    /// Applies one edit, schedules the save, and rebuilds the pane once the control's action has
    /// returned (rebuilding inside it would free the target that is running).
    pub fn edit(&self, change: impl FnOnce(&mut Store)) {
        change(&mut self.store.borrow_mut());
        self.schedule_save();
        Self::render_later();
    }

    /// An edit that needs no rebuild (a slider being dragged).
    pub fn edit_quiet(&self, change: impl FnOnce(&mut Store)) {
        change(&mut self.store.borrow_mut());
        self.schedule_save();
    }

    pub fn render_later() {
        DispatchQueue::main().exec_async(|| app().render());
    }

    fn schedule_save(&self) {
        if let Some(t) = self.save_timer.borrow_mut().take() {
            t.invalidate();
        }
        let block = RcBlock::new(|_t: std::ptr::NonNull<NSTimer>| app().save_now());
        // SAFETY: the block only touches main-thread state and the timer fires on the main run loop.
        let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.4, false, &block) };
        *self.save_timer.borrow_mut() = Some(timer);
    }

    pub fn save_now(&self) {
        if let Some(t) = self.save_timer.borrow_mut().take() {
            t.invalidate();
        }
        let had_error = self.store.borrow().error_message.clone();
        self.store.borrow_mut().save_now();
        if self.store.borrow().error_message != had_error {
            Self::render_later();
        }
    }

    /// Saves a pending edit now; called when the window closes or the app quits.
    pub fn flush(&self) {
        if self.save_timer.borrow().is_some() {
            self.save_now();
        }
    }

    pub fn reload(&self) {
        if let Some(t) = self.save_timer.borrow_mut().take() {
            t.invalidate();
        }
        self.store.borrow_mut().reset_errors();
        self.load();
    }

    pub fn select(&self, page: Page) {
        if self.page.get() != page {
            self.page.set(page);
            self.render();
        }
    }

    /// The 1 Hz tick: rebuild only when a reading on screen changed and no button is held down.
    pub fn tick(&self) {
        self.store.borrow_mut().refresh_live();
        let key = {
            let s = self.store.borrow();
            format!("{:?}{:?}{:?}", s.live, s.last_revert, s.vitals)
        };
        if *self.live_key.borrow() != key && NSEvent::pressedMouseButtons() == 0 {
            self.render();
        }
    }

    /// Rebuilds the detail pane, keeping the scroll position.
    pub fn render(&self) {
        let Some(scroll) = self.detail.borrow().clone() else { return };
        {
            let s = self.store.borrow();
            *self.live_key.borrow_mut() = format!("{:?}{:?}{:?}", s.live, s.last_revert, s.vitals);
        }
        let mut ctx = Ctx { mtm: self.mtm, targets: vec![] };
        let content = pages::page(self, &mut ctx);
        let clip = scroll.contentView();
        let origin = clip.bounds().origin;
        let doc = FlippedView::new(self.mtm);
        doc.setTranslatesAutoresizingMaskIntoConstraints(false);
        doc.addSubview(&content);
        // 52 pt: where SwiftUI's grouped Form starts under the transparent title bar.
        content.topAnchor().constraintEqualToAnchor_constant(&doc.topAnchor(), 52.0).setActive(true);
        content.bottomAnchor().constraintEqualToAnchor_constant(&doc.bottomAnchor(), -20.0).setActive(true);
        widgets::fill(&doc, &content, 20.0);
        scroll.setDocumentView(Some(&doc));
        doc.leadingAnchor().constraintEqualToAnchor(&clip.leadingAnchor()).setActive(true);
        doc.trailingAnchor().constraintEqualToAnchor(&clip.trailingAnchor()).setActive(true);
        doc.topAnchor().constraintEqualToAnchor(&clip.topAnchor()).setActive(true);
        scroll.layoutSubtreeIfNeeded();
        clip.scrollToPoint(origin);
        scroll.reflectScrolledClipView(&clip);
        // Old targets go only now: their controls are out of the window.
        *self.targets.borrow_mut() = ctx.targets;
    }
}

/// `.disabled(true)` for a subtree: every control in it stops taking clicks.
pub fn disable_tree(view: &NSView) {
    if let Some(control) = view.downcast_ref::<objc2_app_kit::NSControl>() {
        control.setEnabled(false);
    }
    if let Some(sw) = view.downcast_ref::<widgets::AccentSwitch>() {
        sw.set_enabled(false);
    }
    for sub in view.subviews().iter() {
        disable_tree(&sub);
    }
}

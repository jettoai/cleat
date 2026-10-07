//! The window: 720 x 560, a source-list sidebar of the three panes, the detail pane on the right
//! (Swift `SettingsWindow.makeWindow`, `SettingsView.body`).

use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSControlTextEditingDelegate, NSScrollView, NSSplitViewController, NSSplitViewItem, NSTableColumn, NSTableView,
    NSTableViewDataSource, NSTableViewDelegate, NSTableViewStyle, NSView, NSViewController, NSWindow,
    NSWindowDidBecomeKeyNotification, NSWindowDidResignKeyNotification,
    NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{NSEdgeInsets, NSIndexSet, NSInteger, NSNotification, NSNotificationCenter, NSPoint, NSRange, NSRect, NSSize};

use super::super::text::Page;
use super::widgets::{to_view, body, ns, overflowing_icon, stack};
use super::{app, App};

pub struct SidebarIvars {
    mtm: MainThreadMarker,
    table: OnceCell<Retained<NSTableView>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CleatRsSettingsSidebar"]
    #[ivars = SidebarIvars]
    pub struct Sidebar;

    impl Sidebar {
        /// The window became or stopped being key: redraw the rows in that state's colours.
        #[unsafe(method(keyChanged:))]
        fn key_changed(&self, _notification: &NSNotification) {
            // Only the cells: a full `reloadData` here drops the selected row's highlight.
            if let Some(table) = self.ivars().table.get() {
                let rows = NSIndexSet::indexSetWithIndexesInRange(NSRange::new(0, Page::ALL.len()));
                table.reloadDataForRowIndexes_columnIndexes(&rows, &NSIndexSet::indexSetWithIndex(0));
            }
        }
    }

    unsafe impl NSObjectProtocol for Sidebar {}

    unsafe impl NSTableViewDataSource for Sidebar {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn rows(&self, _table: &NSTableView) -> NSInteger {
            Page::ALL.len() as NSInteger
        }
    }

    unsafe impl NSControlTextEditingDelegate for Sidebar {}

    unsafe impl NSTableViewDelegate for Sidebar {
        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn view_for(&self, table: &NSTableView, _column: Option<&NSTableColumn>, row: NSInteger) -> Option<Retained<NSView>> {
            let mtm = self.ivars().mtm;
            let page = Page::ALL[row as usize];
            // Swift `SettingsIcon(symbol:)` in the sidebar `List`, as the Swift window draws it: in a
            // key window accent glyphs and opaque text; otherwise opaque glyphs and text at about
            // 40% (measured: 117 on 40 dark, 140 on 242 light). The glyph sits in a 20pt square at
            // 1.3x its 15pt font (mic 20pt tall).
            let key = table.window().is_some_and(|w| w.isKeyWindow());
            let (glyph_color, text_color) = if key {
                (NSColor::controlAccentColor(), NSColor::textColor())
            } else {
                (NSColor::textColor(), NSColor::textColor().colorWithAlphaComponent(0.4))
            };
            let glyph = overflowing_icon(mtm, page.symbol(), 20.0, 15.0 * 1.3, &glyph_color);
            let title = body(mtm, page.title());
            title.setTextColor(Some(&text_color));
            let line = stack(mtm, false, 7.0, &[&glyph, &title]);
            // Measured off the Swift row: glyph 3pt further in, title 7pt after it.
            line.setEdgeInsets(NSEdgeInsets { top: 0.0, left: 3.0, bottom: 0.0, right: 0.0 });
            Some(to_view(&line))
        }

        #[unsafe(method(tableView:heightOfRow:))]
        fn height(&self, _table: &NSTableView, _row: NSInteger) -> f64 {
            32.0
        }

        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_changed(&self, notification: &NSNotification) {
            let Some(table) = notification.object().and_then(|o| o.downcast::<NSTableView>().ok()) else { return };
            let row = table.selectedRow();
            if (0..Page::ALL.len() as NSInteger).contains(&row) {
                app().select(Page::ALL[row as usize]);
            }
        }
    }
);

/// Screenshot hooks for side-by-side checks; a release build ignores them.
pub fn debug_env(name: &str) -> Option<String> {
    if cfg!(debug_assertions) {
        std::env::var(name).ok()
    } else {
        None
    }
}

fn controller(mtm: MainThreadMarker, view: &NSView) -> Retained<NSViewController> {
    let vc = NSViewController::new(mtm);
    vc.setView(view);
    vc
}

fn scroll(mtm: MainThreadMarker) -> Retained<NSScrollView> {
    let s = NSScrollView::new(mtm);
    s.setHasVerticalScroller(true);
    s.setHasHorizontalScroller(false);
    s.setDrawsBackground(false);
    s
}

/// Builds the window and keeps the sidebar's delegate alive in the returned tuple.
pub fn make_window(app: &App) -> (Retained<NSWindow>, Retained<Sidebar>) {
    let mtm = app.mtm;
    let height = debug_env("CLEAT_SETTINGS_HEIGHT").and_then(|h| h.parse().ok()).unwrap_or(560.0);
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(720.0, height));
    let mask = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::FullSizeContentView;
    // SAFETY: the designated initialiser with a valid rect and mask.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), rect, mask, NSBackingStoreType::Buffered, false)
    };
    window.setTitle(&ns("Cleat 設定"));
    window.setTitlebarAppearsTransparent(true);
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    // SAFETY: the window is held by the App for the life of the process.
    unsafe { window.setReleasedWhenClosed(false) };

    let sidebar = Sidebar::alloc(mtm).set_ivars(SidebarIvars { mtm, table: OnceCell::new() });
    // SAFETY: NSObject's init.
    let sidebar: Retained<Sidebar> = unsafe { msg_send![super(sidebar), init] };
    let table = NSTableView::new(mtm);
    let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &ns("page"));
    table.addTableColumn(&column);
    table.setHeaderView(None);
    table.setStyle(NSTableViewStyle::SourceList);
    table.setRowHeight(32.0);
    // Like SwiftUI's sidebar List: a click gives the list key focus, so ↑/↓ change pages.
    // SAFETY: the sidebar object lives as long as the window (returned to the caller).
    unsafe {
        table.setDataSource(Some(ProtocolObject::from_ref(&*sidebar)));
        table.setDelegate(Some(ProtocolObject::from_ref(&*sidebar)));
    }
    let _ = sidebar.ivars().table.set(table.clone());
    let center = NSNotificationCenter::defaultCenter();
    for name in [unsafe { NSWindowDidBecomeKeyNotification }, unsafe { NSWindowDidResignKeyNotification }] {
        // SAFETY: `keyChanged:` takes one NSNotification; the sidebar and window outlive the observation.
        unsafe { center.addObserver_selector_name_object(&sidebar, sel!(keyChanged:), Some(name), Some(&window)) };
    }
    let start = match debug_env("CLEAT_SETTINGS_PAGE").as_deref() {
        Some("input") => Page::Input,
        Some("headphones") => Page::Headphones,
        _ => Page::Output,
    };
    app.page.set(start);
    let index = Page::ALL.iter().position(|p| *p == start).unwrap_or(0);
    table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(index as _), false);
    let side_scroll = scroll(mtm);
    side_scroll.setDocumentView(Some(&table));

    let split = NSSplitViewController::new(mtm);
    let side_item = NSSplitViewItem::sidebarWithViewController(&controller(mtm, &side_scroll));
    side_item.setMinimumThickness(140.0);
    side_item.setMaximumThickness(240.0);
    side_item.setCanCollapse(false);
    split.addSplitViewItem(&side_item);
    let detail = scroll(mtm);
    // SwiftUI's Form: an overlay scroller that shows only while scrolling.
    detail.setAutohidesScrollers(true);
    detail.setScrollerStyle(objc2_app_kit::NSScrollerStyle::Overlay);
    // The page's own 52 pt top margin already clears the title bar; no inset on top of it.
    detail.setAutomaticallyAdjustsContentInsets(false);
    let detail_item = NSSplitViewItem::splitViewItemWithViewController(&controller(mtm, &detail));
    split.addSplitViewItem(&detail_item);
    window.setContentViewController(Some(&split));
    window.setContentSize(NSSize::new(720.0, height));
    split.splitView().setPosition_ofDividerAtIndex(144.0, 0);
    window.setContentMinSize(NSSize::new(680.0, 480.0));
    window.center();
    *app.detail.borrow_mut() = Some(detail);
    (window, sidebar)
}

//! The window: 720 x 560, a source-list sidebar of the three panes, the detail pane on the right
//! (Swift `SettingsWindow.makeWindow`, `SettingsView.body`).

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSControlTextEditingDelegate, NSScrollView, NSSplitViewController, NSSplitViewItem, NSTableColumn, NSTableView,
    NSTableViewDataSource, NSTableViewDelegate, NSTableViewStyle, NSView, NSViewController, NSWindow,
    NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{NSIndexSet, NSInteger, NSNotification, NSPoint, NSRect, NSSize};

use super::super::text::Page;
use super::widgets::{to_view, body, icon, ns, stack};
use super::{app, App};

pub struct SidebarIvars {
    mtm: MainThreadMarker,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CleatRsSettingsSidebar"]
    #[ivars = SidebarIvars]
    pub struct Sidebar;

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
        fn view_for(&self, _table: &NSTableView, _column: Option<&NSTableColumn>, row: NSInteger) -> Option<Retained<NSView>> {
            let mtm = self.ivars().mtm;
            let page = Page::ALL[row as usize];
            let line = stack(mtm, false, 6.0, &[&icon(mtm, page.symbol(), 24.0, &NSColor::labelColor()), &body(mtm, page.title())]);
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

    let sidebar = Sidebar::alloc(mtm).set_ivars(SidebarIvars { mtm });
    // SAFETY: NSObject's init.
    let sidebar: Retained<Sidebar> = unsafe { msg_send![super(sidebar), init] };
    let table = NSTableView::new(mtm);
    let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &ns("page"));
    table.addTableColumn(&column);
    table.setHeaderView(None);
    table.setStyle(NSTableViewStyle::SourceList);
    table.setRowHeight(32.0);
    // SAFETY: the sidebar object lives as long as the window (returned to the caller).
    unsafe {
        table.setDataSource(Some(ProtocolObject::from_ref(&*sidebar)));
        table.setDelegate(Some(ProtocolObject::from_ref(&*sidebar)));
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

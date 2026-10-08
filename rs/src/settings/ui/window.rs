//! The window: 720 x 560, a source-list sidebar of the three panes, the detail pane on the right
//! (Swift `SettingsWindow.makeWindow`, `SettingsView.body`).

use std::cell::OnceCell;
use std::ptr::NonNull;

use block2::RcBlock;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAppearance, NSBackingStoreType, NSColor, NSFont, NSImageView, NSTableViewSelectionHighlightStyle, NSControlTextEditingDelegate, NSScrollView, NSSplitViewController, NSSplitViewItem, NSTableColumn, NSTableView,
    NSTableViewDataSource, NSTableViewDelegate, NSTableViewStyle, NSView, NSViewController, NSWindow,
    NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{NSEdgeInsets, NSIndexSet, NSInteger, NSNotification, NSPoint, NSRange, NSRect, NSSize};

use super::super::text::Page;
use super::widgets::{to_view, label, ns, overflowing_icon, regular, rounded_box, size, stack};
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
            // Jetto voice `SidebarButton`: monochrome 15 pt glyph in a 22 pt column, 15 pt title 8 pt
            // after it; the selected row in the primary colour (title medium) on a primary-8% rounded
            // fill, the others secondary. Same whether or not the list has focus: the table's own
            // highlight is off.
            let selected = table.selectedRow() == row;
            let color = if selected { NSColor::labelColor() } else { NSColor::secondaryLabelColor() };
            let glyph = overflowing_icon(mtm, page.symbol(), 22.0, 15.0, &color);
            // SAFETY: an AppKit constant.
            let weight = if selected { unsafe { objc2_app_kit::NSFontWeightMedium } } else { regular() };
            let title = label(mtm, page.title(), 15.0, weight, &color);
            let line = stack(mtm, false, 8.0, &[&glyph, &title]);
            let fill_color = if selected { NSColor::labelColor().colorWithAlphaComponent(0.08) } else { NSColor::clearColor() };
            let cell = rounded_box(mtm, 8.0, &fill_color, None);
            let inner = NSView::new(mtm);
            cell.setContentView(Some(&inner));
            line.setTranslatesAutoresizingMaskIntoConstraints(false);
            inner.addSubview(&line);
            line.leadingAnchor().constraintEqualToAnchor_constant(&inner.leadingAnchor(), 10.0).setActive(true);
            line.centerYAnchor().constraintEqualToAnchor(&inner.centerYAnchor()).setActive(true);
            Some(to_view(&cell))
        }

        #[unsafe(method(tableView:heightOfRow:))]
        fn height(&self, _table: &NSTableView, _row: NSInteger) -> f64 {
            40.0
        }

        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_changed(&self, notification: &NSNotification) {
            let Some(table) = notification.object().and_then(|o| o.downcast::<NSTableView>().ok()) else { return };
            let row = table.selectedRow();
            if (0..Page::ALL.len() as NSInteger).contains(&row) {
                app().select(Page::ALL[row as usize]);
            }
            // The selection is drawn by the cells; redraw them for the new one.
            let rows = NSIndexSet::indexSetWithIndexesInRange(NSRange::new(0, Page::ALL.len()));
            table.reloadDataForRowIndexes_columnIndexes(&rows, &NSIndexSet::indexSetWithIndex(0));
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

/// Jetto voice `SidebarBackground`: an opaque #28282A dark, #EBEBED light, under the list.
fn sidebar_color() -> Retained<NSColor> {
    let solid = |v: u8| Retained::into_raw(NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(v) / 255.0, f64::from(v) / 255.0, f64::from(v + 2) / 255.0, 1.0,
    ));
    // ponytail: two colours leaked once per window build; fine for a window opened a handful of times.
    let (dark, light) = (solid(0x28), solid(0xEB));
    let pick = RcBlock::new(move |appearance: NonNull<NSAppearance>| -> NonNull<NSColor> {
        // SAFETY: AppKit hands a live appearance; the two colours are leaked, so never freed.
        let dark_mode = unsafe { appearance.as_ref() }.name().to_string().contains("Dark");
        NonNull::new(if dark_mode { dark } else { light }).expect("leaked colour")
    });
    // SAFETY: the provider returns one of two colours that live for the process.
    unsafe { NSColor::colorWithName_dynamicProvider(None, &pick) }
}

/// The sidebar: under the traffic lights, "Cleat by ⬢Jetto" (Tally's About row: "by" 11 pt,
/// 6 pt apart, the wordmark 53 x 12) and a version capsule (Jetto voice `SidebarHeader`), then the list.
fn with_brand(mtm: MainThreadMarker, list: &NSScrollView) -> Retained<NSView> {
    // SAFETY: an AppKit constant.
    let bold = unsafe { objc2_app_kit::NSFontWeightBold };
    let name = label(mtm, "Cleat", 18.0, bold, &NSColor::labelColor());
    let by = label(mtm, "by", 11.0, regular(), &NSColor::labelColor());
    let brand = stack(mtm, false, 6.0, &[&name, &by]);
    brand.setAlignment(objc2_app_kit::NSLayoutAttribute::FirstBaseline);
    let mark_size = super::super::jetto::ABOUT;
    if let Some(image) = super::super::jetto::wordmark(mark_size, NSColor::labelColor) {
        let mark = NSImageView::imageViewWithImage(&image, mtm);
        size(&mark, mark_size.0, mark_size.1);
        brand.addArrangedSubview(&mark);
    } else {
        brand.addArrangedSubview(&label(mtm, "Jetto", 11.0, regular(), &NSColor::labelColor()));
    }
    let version = crate::identity::Identity::current().version();
    let v = label(mtm, &format!("v{version}"), 9.0, regular(), &NSColor::secondaryLabelColor());
    // SAFETY: an AppKit constant.
    v.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(9.0, unsafe { objc2_app_kit::NSFontWeightMedium })));
    let pill = rounded_box(mtm, 7.5, &NSColor::labelColor().colorWithAlphaComponent(0.06), None);
    let inner = stack(mtm, false, 0.0, &[&v]);
    inner.setEdgeInsets(NSEdgeInsets { top: 2.0, left: 6.0, bottom: 2.0, right: 6.0 });
    pill.setContentView(Some(&inner));
    inner.topAnchor().constraintEqualToAnchor(&pill.topAnchor()).setActive(true);
    inner.bottomAnchor().constraintEqualToAnchor(&pill.bottomAnchor()).setActive(true);
    inner.leadingAnchor().constraintEqualToAnchor(&pill.leadingAnchor()).setActive(true);
    inner.trailingAnchor().constraintEqualToAnchor(&pill.trailingAnchor()).setActive(true);
    let head = stack(mtm, true, 6.0, &[&brand, &pill]);
    let side = rounded_box(mtm, 0.0, &sidebar_color(), None);
    let body = NSView::new(mtm);
    side.setContentView(Some(&body));
    for v in [to_view(list), to_view(&head)] {
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        body.addSubview(&v);
    }
    // 52 pt: Jetto voice keeps the traffic-light row clear above its header.
    head.topAnchor().constraintEqualToAnchor_constant(&body.topAnchor(), 52.0).setActive(true);
    head.leadingAnchor().constraintEqualToAnchor_constant(&body.leadingAnchor(), 20.0).setActive(true);
    list.topAnchor().constraintEqualToAnchor_constant(&head.bottomAnchor(), 12.0).setActive(true);
    list.leadingAnchor().constraintEqualToAnchor(&body.leadingAnchor()).setActive(true);
    list.trailingAnchor().constraintEqualToAnchor(&body.trailingAnchor()).setActive(true);
    list.bottomAnchor().constraintEqualToAnchor(&body.bottomAnchor()).setActive(true);
    to_view(&side)
}

/// Builds the window and keeps the sidebar's delegate alive in the returned tuple.
pub fn make_window(app: &App) -> (Retained<NSWindow>, Retained<Sidebar>) {
    let mtm = app.mtm;
    // Screenshot hook: force light or dark without touching the system setting.
    if let Some(look) = debug_env("CLEAT_SETTINGS_APPEARANCE") {
        // SAFETY: AppKit constants.
        let name = unsafe { if look == "dark" { objc2_app_kit::NSAppearanceNameDarkAqua } else { objc2_app_kit::NSAppearanceNameAqua } };
        objc2_app_kit::NSApplication::sharedApplication(mtm).setAppearance(NSAppearance::appearanceNamed(name).as_deref());
    }
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
    // The rows draw Jetto voice's own selection (see `view_for`); the arrow keys still move it.
    table.setSelectionHighlightStyle(NSTableViewSelectionHighlightStyle::None);
    table.setRowHeight(40.0);
    // Like SwiftUI's sidebar List: a click gives the list key focus, so ↑/↓ change pages.
    // SAFETY: the sidebar object lives as long as the window (returned to the caller).
    unsafe {
        table.setDataSource(Some(ProtocolObject::from_ref(&*sidebar)));
        table.setDelegate(Some(ProtocolObject::from_ref(&*sidebar)));
    }
    let _ = sidebar.ivars().table.set(table.clone());
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
    side_scroll.setAutomaticallyAdjustsContentInsets(false);
    let side = with_brand(mtm, &side_scroll);

    let split = NSSplitViewController::new(mtm);
    let side_item = NSSplitViewItem::sidebarWithViewController(&controller(mtm, &side));
    // 180: "Cleat by Jetto" needs the room; narrower clips the wordmark.
    side_item.setMinimumThickness(180.0);
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
    split.splitView().setPosition_ofDividerAtIndex(180.0, 0);
    window.setContentMinSize(NSSize::new(680.0, 480.0));
    window.center();
    *app.detail.borrow_mut() = Some(detail);
    (window, sidebar)
}

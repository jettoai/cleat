//! The daemon's menu bar item (no Swift counterpart; Jetto voice `StatusItemController.swift` is
//! the model). The daemon's main thread runs NSApplication instead of a bare CFRunLoop; the engine
//! stays on its own thread and is never touched from here. The menu reads status.json each time it
//! opens, so it needs no channel to the engine.

use std::cell::RefCell;
use std::ptr::NonNull;

use block2::RcBlock;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSAboutPanelOptionCredits, NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
    NSBundleImageExtension, NSColor, NSImage, NSMenu, NSMenuDelegate, NSMenuItem, NSMutableParagraphStyle, NSParagraphStyleAttributeName,
    NSAttributedStringAttachmentConveniences, NSSquareStatusItemLength, NSStatusBar, NSStatusItem, NSTextAlignment,
    NSTextAttachment,
};
use objc2_foundation::{
    NSAttributedString, NSBundle, NSDictionary, NSMutableAttributedString, NSPoint, NSRange, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, NSString, NSTimer,
};

use crate::config::paths;
use crate::engine::Status;
use crate::settings::lang::{self, Lang};
use crate::settings::text::{fill, BYLINE_CREDIT};
use crate::settings::words::W;

/// The swap point for the icon: Contents/Resources/MenuBarIcon.png (+ MenuBarIcon@2x.png), copied
/// from rs/bundle by scripts/bundle.sh, black on transparent at 18 pt. Replace those files to
/// change the icon; no code change.
pub const ICON_RESOURCE: &str = "MenuBarIcon";
/// Shown when the bundle has no MenuBarIcon (a bare `cargo run`).
/// Neutral on purpose: Cleat is to look after more than sound.
pub const FALLBACK_SYMBOL: &str = "slider.horizontal.3";

/// The two read-only lines at the top of the menu.
pub fn device_lines(status: Option<&Status>, l: Lang) -> [String; 2] {
    let name = |v: Option<&String>| v.map_or(W::Loading.get(l).to_string(), String::clone);
    [
        fill(W::MenuOutput.get(l), &[&name(status.and_then(|s| s.default_output.as_ref()))]),
        fill(W::MenuInput.get(l), &[&name(status.and_then(|s| s.default_input.as_ref()))]),
    ]
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CleatRsMenuBar"]
    struct MenuBar;

    unsafe impl NSObjectProtocol for MenuBar {}

    unsafe impl NSApplicationDelegate for MenuBar {
        /// Raycast or Finder opening the bundle while this daemon runs can arrive here instead of
        /// as a new process: the same answer as `opened_by_hand`, the settings window.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn reopen(&self, _app: &NSApplication, _visible: bool) -> bool {
            super::open_settings::open_settings();
            false
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn keep_running(&self, _app: &NSApplication) -> bool {
            false
        }
    }

    unsafe impl NSMenuDelegate for MenuBar {
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            rebuild(menu, self);
        }
    }

    impl MenuBar {
        #[unsafe(method(openSettings:))]
        fn open_settings_action(&self, _sender: Option<&AnyObject>) {
            super::open_settings::open_settings();
        }

        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            show_about_panel();
        }
    }
);

thread_local! {
    static KEEP: RefCell<Vec<Retained<NSObject>>> = const { RefCell::new(vec![]) };
}

/// Replaces `CFRunLoop::run()` for the enforcing daemon; returns only if the app stops (Quit exits).
pub fn run(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon even without the bundle's LSUIElement (a bare `cargo run`).
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    // SAFETY: NSObject's init.
    let bar: Retained<MenuBar> = unsafe { msg_send![MenuBar::alloc(mtm), init] };
    app.setDelegate(Some(ProtocolObject::from_ref(&*bar)));
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSSquareStatusItemLength);
    if let Some(button) = item.button(mtm) {
        match icon() {
            Some(img) => button.setImage(Some(&img)),
            None => button.setTitle(&NSString::from_str("Cleat")),
        }
        button.setToolTip(Some(&NSString::from_str("Cleat")));
    }
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    menu.setDelegate(Some(ProtocolObject::from_ref(&*bar)));
    item.setMenu(Some(&menu));
    debug_drive(&item, &menu, &bar);
    // The status item disappears the moment it is released.
    KEEP.with(|k| {
        let mut k = k.borrow_mut();
        k.push(Retained::into_super(bar));
        k.push(Retained::into_super(item));
    });
    app.run();
}

/// Screenshot hooks, debug builds only (as `settings::ui::window::debug_env`): 2 s after launch,
/// CLEAT_MENUBAR_ACTION=<index> fires that menu item; CLEAT_MENUBAR_OPEN=<seconds> opens the menu
/// from inside the process (no synthetic input) and closes it after that many seconds.
fn debug_drive(item: &NSStatusItem, menu: &NSMenu, bar: &MenuBar) {
    use crate::settings::ui::window::debug_env;
    let (action, open) = (debug_env("CLEAT_MENUBAR_ACTION"), debug_env("CLEAT_MENUBAR_OPEN"));
    if action.is_none() && open.is_none() {
        return;
    }
    let (item, menu, bar) = (item.retain(), menu.retain(), bar.retain());
    let block = RcBlock::new(move |_t: NonNull<NSTimer>| {
        if let Some(i) = action.as_deref().and_then(|a| a.parse().ok()) {
            rebuild(&menu, &bar);
            menu.performActionForItemAtIndex(i);
        }
        if let Some(secs) = open.as_deref().and_then(|s| s.parse().ok()) {
            let tracked = menu.clone();
            let close = RcBlock::new(move |_t: NonNull<NSTimer>| tracked.cancelTracking());
            // SAFETY: main run loop; common modes so it fires while the menu tracks.
            unsafe {
                let t = NSTimer::timerWithTimeInterval_repeats_block(secs, false, &close);
                NSRunLoop::currentRunLoop().addTimer_forMode(&t, NSRunLoopCommonModes);
            }
            if let Some(button) = item.button(MainThreadMarker::from(&*bar)) {
                // A status button's performClick does not pop its menu; pop it under the button.
                let below = NSPoint::new(0.0, button.bounds().size.height + 5.0);
                menu.popUpMenuPositioningItem_atLocation_inView(None, below, Some(&button));
            }
        }
    });
    // SAFETY: main run loop only.
    let _ = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(2.0, false, &block) };
}

/// The bundle resource first (the swap point), the SF Symbol second; always a template image.
fn icon() -> Option<Retained<NSImage>> {
    let img = NSBundle::mainBundle().imageForResource(&NSString::from_str(ICON_RESOURCE)).or_else(|| {
        NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(FALLBACK_SYMBOL),
            Some(&NSString::from_str("Cleat")),
        )
    })?;
    img.setTemplate(true);
    img.setSize(NSSize::new(18.0, 18.0));
    Some(img)
}

fn rebuild(menu: &NSMenu, target: &MenuBar) {
    let mtm = MainThreadMarker::from(target);
    let l = lang::current();
    menu.removeAllItems();
    let status = Status::read(&paths::status_path());
    for line in device_lines(status.as_ref(), l) {
        let it = item(mtm, &line, None, "");
        it.setEnabled(false);
        menu.addItem(&it);
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    let open = item(mtm, W::OpenSettings.get(l), Some(sel!(openSettings:)), ",");
    let about = item(mtm, W::About.get(l), Some(sel!(showAbout:)), "");
    for it in [&open, &about] {
        // SAFETY: the target is kept alive in KEEP for the life of the process.
        unsafe { it.setTarget(Some(target)) };
        menu.addItem(it);
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    let quit = item(mtm, W::Quit.get(l), Some(sel!(terminate:)), "q");
    // SAFETY: the shared application outlives the menu.
    unsafe { quit.setTarget(Some(&NSApplication::sharedApplication(mtm))) };
    menu.addItem(&quit);
}

fn item(mtm: MainThreadMarker, title: &str, action: Option<Sel>, key: &str) -> Retained<NSMenuItem> {
    // SAFETY: the designated initialiser; the action is a selector on the item's target.
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(key),
        )
    }
}

/// "by" and the Jetto wordmark, as the settings sidebar shows it; the words alone without it.
fn about_credits() -> Retained<NSAttributedString> {
    let Some(image) = crate::settings::jetto::wordmark(crate::settings::jetto::ABOUT, NSColor::labelColor) else {
        return NSAttributedString::from_nsstring(&NSString::from_str(BYLINE_CREDIT));
    };
    let (w, h) = crate::settings::jetto::ABOUT;
    let mark = NSTextAttachment::new();
    mark.setImage(Some(&image));
    // 2 pt below the baseline: the wordmark's foot sits where the words' descenders start.
    mark.setBounds(NSRect::new(NSPoint::new(0.0, -2.0), NSSize::new(w, h)));
    let s = NSMutableAttributedString::from_nsstring(&NSString::from_str("by "));
    s.appendAttributedString(&NSAttributedString::attributedStringWithAttachment(&mark));
    let paragraph = NSMutableParagraphStyle::new();
    paragraph.setAlignment(NSTextAlignment::Center);
    // SAFETY: an AppKit constant key with the NSParagraphStyle value it documents.
    unsafe { s.addAttribute_value_range(NSParagraphStyleAttributeName, &paragraph, NSRange::new(0, s.length())) };
    Retained::into_super(s)
}

/// The standard About panel; its Credits carry the attribution, as the settings sidebar does.
fn show_about_panel() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    let credits = about_credits();
    let value: &AnyObject = &credits;
    // SAFETY: an AppKit constant key, with the NSAttributedString value the key documents.
    unsafe {
        let options = NSDictionary::from_slices(&[NSAboutPanelOptionCredits], &[value]);
        // An accessory app is never active by itself; without this the panel opens behind.
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        app.orderFrontStandardAboutPanelWithOptions(&options);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(output: Option<&str>, input: Option<&str>) -> Status {
        let mut s: Status = serde_json::from_str(
            r#"{"pid":1,"updatedAt":"","configState":"","microphone":"","rules":{},"liveness":{},"recentEvents":[]}"#,
        )
        .unwrap();
        s.default_output = output.map(Into::into);
        s.default_input = input.map(Into::into);
        s
    }

    #[test]
    fn device_lines_from_status() {
        let s = status(Some("外接耳機"), Some("Wireless microphone"));
        assert_eq!(device_lines(Some(&s), Lang::ZhHant), ["輸出：外接耳機".to_string(), "輸入：Wireless microphone".to_string()]);
        assert_eq!(device_lines(Some(&s), Lang::En), ["Output: 外接耳機".to_string(), "Input: Wireless microphone".to_string()]);
    }

    #[test]
    fn device_lines_without_status() {
        let reading = ["輸出：讀取中…".to_string(), "輸入：讀取中…".to_string()];
        assert_eq!(device_lines(None, Lang::ZhHant), reading);
        assert_eq!(device_lines(Some(&status(None, None)), Lang::ZhHant), reading);
        assert_eq!(device_lines(None, Lang::En), ["Output: Reading…".to_string(), "Input: Reading…".to_string()]);
    }
}

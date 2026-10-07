//! `cleat settings`: the settings window, in its own process (Swift `SettingsWindow.swift`). It
//! starts no Engine; edits reach the daemon only through the config file, which it hot-reloads.

pub mod document;
pub mod draft;
pub mod sources;
pub mod store;
pub mod text;
pub mod ui;
pub mod vitals;

use std::cell::RefCell;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSMenu, NSMenuItem};
use objc2_foundation::{NSNotification, NSTimer};

use ui::widgets::ns;
use ui::window::Sidebar;
use ui::{app, App};

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CleatRsSettingsDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish(&self, _n: &NSNotification) {
            open_window();
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn close_quits(&self, _app: &NSApplication) -> bool {
            true
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, _n: &NSNotification) {
            app().flush();
        }
    }
);

thread_local! {
    static KEEP: RefCell<Vec<Retained<NSObject>>> = const { RefCell::new(vec![]) };
}

fn keep(o: Retained<NSObject>) {
    KEEP.with(|k| k.borrow_mut().push(o));
}

/// `cleat settings`; `--snapshot <dir>` is not ported (B-1209 deviation list).
pub fn run(args: &[String]) -> i32 {
    if let Some(unexpected) = args.first() {
        eprintln!("cleat settings: unexpected argument '{unexpected}'");
        return 2;
    }
    let mtm = MainThreadMarker::new().expect("settings must run on the main thread");
    App::install(mtm);
    let application = NSApplication::sharedApplication(mtm);
    application.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    // SAFETY: NSObject's init.
    let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(mtm), init] };
    application.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    keep(Retained::into_super(delegate));
    application.setMainMenu(Some(&main_menu(mtm)));
    application.run();
    0
}

fn open_window() {
    let a = app();
    let (window, sidebar): (_, Retained<Sidebar>) = ui::window::make_window(&a);
    keep(Retained::into_super(sidebar));
    *a.window.borrow_mut() = Some(window.clone());
    a.load();
    window.makeKeyAndOrderFront(None);
    let application = NSApplication::sharedApplication(a.mtm);
    application.activate();
    // Screenshot runs start from a shell that is not frontmost, where `activate` is only a
    // request; a debug build asked for a page takes the front so controls draw as key.
    if ui::window::debug_env("CLEAT_SETTINGS_PAGE").is_some() {
        #[allow(deprecated)]
        application.activateIgnoringOtherApps(true);
        window.makeKeyAndOrderFront(None);
    }
    // Swift opens with the window itself focused (grey selection); AppKit would hand the list,
    // the first key view, focus at once. A click on the list still takes it, for ↑/↓.
    window.makeFirstResponder(None);
    let block = RcBlock::new(|_t: std::ptr::NonNull<NSTimer>| app().tick());
    // SAFETY: as above, main run loop only.
    let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.0, true, &block) };
    keep(Retained::into_super(timer));
}

fn menu_item(mtm: MainThreadMarker, title: &str, action: objc2::runtime::Sel, key: &str) -> Retained<NSMenuItem> {
    // SAFETY: a standard responder-chain selector.
    unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &ns(title), Some(action), &ns(key)) }
}

/// "結束 Cleat 設定" ⌘Q and "檔案 > 關閉視窗" ⌘W.
fn main_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let bar = NSMenu::new(mtm);
    let app_item = NSMenuItem::new(mtm);
    let app_menu = NSMenu::new(mtm);
    app_menu.addItem(&menu_item(mtm, "結束 Cleat 設定", sel!(terminate:), "q"));
    app_item.setSubmenu(Some(&app_menu));
    bar.addItem(&app_item);
    let file_item = NSMenuItem::new(mtm);
    let file_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &ns("檔案"));
    file_menu.addItem(&menu_item(mtm, "關閉視窗", sel!(performClose:), "w"));
    file_item.setSubmenu(Some(&file_menu));
    bar.addItem(&file_item);
    bar
}

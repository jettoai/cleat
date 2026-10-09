//! `cleat settings`: the settings window, in its own process (Swift `SettingsWindow.swift`). It
//! starts no Engine; edits reach the daemon only through the config file, which it hot-reloads.

pub mod document;
pub mod draft;
pub mod jetto;
pub mod lang;
pub mod single;
pub mod sources;
pub mod store;
pub mod text;
pub mod ui;
pub mod vitals;
pub mod words;

use std::cell::RefCell;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy, NSApplicationDelegate, NSMenu,
    NSMenuItem, NSRunningApplication,
};
use objc2_foundation::{NSNotification, NSTimer};

use ui::widgets::ns;
use ui::window::Sidebar;
use ui::{app, App};
use words::W;

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

/// macOS defaults on the command line (`-AppleLanguages '(en)'`, Xcode's `-NS...` pairs) are
/// NSUserDefaults' to read, not ours: each flag and the value after it are skipped.
pub fn without_defaults_args(args: &[String]) -> Vec<String> {
    let mut out = vec![];
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if (a.starts_with("-Apple") || a.starts_with("-NS")) && a.len() > 3 {
            i += 2;
            continue;
        }
        out.push(a.clone());
        i += 1;
    }
    out
}

/// `cleat settings`; `--snapshot <dir>` is not ported (B-1209 deviation list).
pub fn run(args: &[String]) -> i32 {
    if let Some(unexpected) = without_defaults_args(args).first() {
        eprintln!("cleat settings: unexpected argument '{unexpected}'");
        return 2;
    }
    let mtm = MainThreadMarker::new().expect("settings must run on the main thread");
    // Held until `run` returns, i.e. for the life of the window. No lock (I/O error) opens anyway:
    // two windows beat a settings window that will not open.
    let _lock = match single::acquire(&crate::config::paths::support_dir().join("settings.lock")) {
        Ok(single::Lock::Held(f)) => Some(f),
        Ok(single::Lock::TakenBy(pid)) => {
            if let Some(app) = pid.and_then(NSRunningApplication::runningApplicationWithProcessIdentifier) {
                app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
            }
            return 0;
        }
        Err(e) => {
            eprintln!("cleat settings: no lock ({e}), opening anyway");
            None
        }
    };
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
    // Screenshot runs (debug builds): show the window without taking the front from whoever has it.
    if ui::window::debug_env("CLEAT_SETTINGS_PAGE").is_some() {
        window.orderFront(None);
    } else {
        take_front(&window, a.mtm);
    }
    let block = RcBlock::new(|_t: std::ptr::NonNull<NSTimer>| app().tick());
    // SAFETY: as above, main run loop only.
    let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.0, true, &block) };
    keep(Retained::into_super(timer));
}

fn take_front(window: &objc2_app_kit::NSWindow, mtm: MainThreadMarker) {
    window.makeKeyAndOrderFront(None);
    let application = NSApplication::sharedApplication(mtm);
    application.activate();
    // `activate` is only a request since macOS 14, and the daemon's menu (an accessory app that is
    // never active) cannot yield the front to us: opened from the menu, the window landed behind
    // the frontmost app (B-1222). Take the front, as a window a person just asked for should.
    #[allow(deprecated)]
    application.activateIgnoringOtherApps(true);
    window.makeKeyAndOrderFront(None);
    // Swift opens with the window itself focused (grey selection); AppKit would hand the list,
    // the first key view, focus at once. A click on the list still takes it, for ↑/↓.
    window.makeFirstResponder(None);
}

fn menu_item(mtm: MainThreadMarker, title: &str, action: objc2::runtime::Sel, key: &str) -> Retained<NSMenuItem> {
    // SAFETY: a standard responder-chain selector.
    unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &ns(title), Some(action), &ns(key)) }
}

/// "Quit Cleat Settings" ⌘Q and "File > Close Window" ⌘W.
fn main_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let l = lang::current();
    let bar = NSMenu::new(mtm);
    let app_item = NSMenuItem::new(mtm);
    let app_menu = NSMenu::new(mtm);
    app_menu.addItem(&menu_item(mtm, W::QuitSettings.get(l), sel!(terminate:), "q"));
    app_item.setSubmenu(Some(&app_menu));
    bar.addItem(&app_item);
    let file_item = NSMenuItem::new(mtm);
    let file_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &ns(W::FileMenu.get(l)));
    file_menu.addItem(&menu_item(mtm, W::CloseWindow.get(l), sel!(performClose:), "w"));
    file_item.setSubmenu(Some(&file_menu));
    bar.addItem(&file_item);
    bar
}

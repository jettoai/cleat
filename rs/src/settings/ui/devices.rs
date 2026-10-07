//! Swift `DeviceListSection`: the priority card, then every other device seen or named.

use objc2::rc::Retained;
use objc2::sel;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSButton, NSColor, NSControlSize, NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem, NSView};

use super::super::draft::{DeviceRow, Side};
use super::super::text::{self, device_list_footer, noun};
use super::pages::device_label;
use super::widgets::{to_view, attributed, icon, ns, plain_button, plain_row, row, secondary, section, spacer, symbol_exists, tag, width, Ctx};
use super::{app, App};
use crate::model::device_name;

fn list_of(a: &App, side: Side) -> super::super::draft::DeviceList {
    let s = a.store.borrow();
    match side {
        Side::Input => s.draft.input.clone(),
        Side::Output => s.draft.output.clone(),
    }
}

fn tags(a: &App, mtm: MainThreadMarker, side: Side, r: &DeviceRow) -> Option<Retained<NSView>> {
    let s = a.store.borrow();
    let current = match side {
        Side::Input => s.live.input_device.clone(),
        Side::Output => s.live.output_device.clone(),
    };
    if r.is_blocked {
        Some(tag(mtm, "已排除", false))
    } else if r.is_connected && current.is_some_and(|c| device_name::matches(&r.display_name, &c, "")) {
        Some(tag(mtm, "使用中", true))
    } else {
        None
    }
}

fn device_icon(a: &App, mtm: MainThreadMarker, side: Side, r: &DeviceRow) -> Retained<NSView> {
    let transport = a.store.borrow().transport_of(&r.display_name);
    let wanted = text::device_symbol(&r.display_name, transport, side);
    let name = if symbol_exists(wanted) { wanted } else { "speaker.wave.2" };
    let v = icon(mtm, name, 20.0, &NSColor::secondaryLabelColor());
    if r.is_blocked {
        v.setAlphaValue(0.4);
    }
    v
}

fn label_view(mtm: MainThreadMarker, r: &DeviceRow) -> Retained<NSView> {
    let v = device_label(mtm, &r.display_name, r.is_connected);
    if r.is_blocked {
        v.setAlphaValue(0.4);
    }
    v
}

fn block_toggle(mtm: MainThreadMarker, ctx: &mut Ctx, side: Side, r: &DeviceRow) -> Retained<NSButton> {
    let entry = r.entry.clone();
    let act = ctx.act(move |sender| {
        let on = sender.downcast_ref::<NSButton>().is_some_and(|b| b.state() == NSControlStateValueOn);
        let entry = entry.clone();
        app().edit(move |s| s.draft.list_mut(side).set_blocked(&entry, on));
    });
    // SAFETY: target and selector match `Action::fire:`.
    let b = unsafe { NSButton::checkboxWithTitle_target_action(&ns("不使用"), Some(&act), Some(sel!(fire:)), mtm) };
    b.setControlSize(NSControlSize::Small);
    b.setAttributedTitle(&attributed("不使用", 10.0, &NSColor::secondaryLabelColor()));
    b.setState(if r.is_blocked { NSControlStateValueOn } else { NSControlStateValueOff });
    b.setToolTip(Some(&ns(text::BLOCK_HELP)));
    b
}

fn menu_item(ctx: &mut Ctx, title: &str, enabled: bool, f: impl Fn() + 'static) -> Retained<NSMenuItem> {
    let act = ctx.act(move |_| f());
    // SAFETY: target and selector match `Action::fire:`.
    let item = unsafe {
        let i = NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(ctx.mtm), &ns(title), Some(sel!(fire:)), &ns(""));
        i.setTarget(Some(&act));
        i
    };
    item.setEnabled(enabled);
    item
}

pub fn priority(a: &App, ctx: &mut Ctx, side: Side) -> Retained<NSView> {
    let mtm = ctx.mtm;
    let listed = list_of(a, side).listed();
    let mut rows = vec![];
    if listed.is_empty() {
        let r = plain_row(mtm, &[&secondary(mtm, &format!("尚未設定，Cleat 不會切換{}", noun(side)), 13.0), &spacer(mtm)]);
        rows.push(to_view(&r));
    }
    let count = listed.len();
    for (i, r) in listed.iter().enumerate() {
        let handle = icon(mtm, "line.3.horizontal", 14.0, &NSColor::tertiaryLabelColor());
        let number = secondary(mtm, &format!("{}", i + 1), 13.0);
        number.setFont(Some(&objc2_app_kit::NSFont::monospacedDigitSystemFontOfSize_weight(13.0, super::widgets::regular())));
        number.setAlignment(objc2_app_kit::NSTextAlignment::Right);
        width(&number, 18.0);
        let mut views: Vec<Retained<NSView>> = vec![
            handle,
            to_view(&number),
            device_icon(a, mtm, side, r),
            label_view(mtm, r),
        ];
        if let Some(t) = tags(a, mtm, side, r) {
            views.push(t);
        }
        views.push(spacer(mtm));
        views.push(to_view(&block_toggle(mtm, ctx, side, r)));
        let refs: Vec<&NSView> = views.iter().map(|v| &**v).collect();
        let line = row(mtm, &refs);
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        menu.addItem(&menu_item(ctx, "上移", i > 0, move || app().edit(move |s| s.draft.list_mut(side).move_listed(i, i.wrapping_sub(1)))));
        menu.addItem(&menu_item(ctx, "下移", i + 1 < count, move || app().edit(move |s| s.draft.list_mut(side).move_listed(i, i + 2))));
        let entry = r.entry.clone();
        menu.addItem(&menu_item(ctx, "移出優先順序", true, move || {
            let entry = entry.clone();
            app().edit(move |s| s.draft.list_mut(side).set_listed(&entry, false))
        }));
        // SAFETY: the menu outlives the row (both are rebuilt together).
        unsafe { line.setMenu(Some(&menu)) };
        rows.push(to_view(&line));
    }
    let header = format!("{}優先順序", noun(side));
    section(mtm, Some(&header), &rows, Some(device_list_footer(side)))
}

pub fn others(a: &App, ctx: &mut Ctx, side: Side) -> Option<Retained<NSView>> {
    let mtm = ctx.mtm;
    let others = list_of(a, side).others();
    if others.is_empty() {
        return None;
    }
    let mut rows = vec![];
    for r in &others {
        let entry = r.entry.clone();
        let act = ctx.act(move |_| {
            let entry = entry.clone();
            app().edit(move |s| s.draft.list_mut(side).set_listed(&entry, true))
        });
        let add = plain_button(mtm, "plus.circle", "加入順序", 10.0, &NSColor::secondaryLabelColor(), &act);
        let mut views: Vec<Retained<NSView>> = vec![device_icon(a, mtm, side, r), label_view(mtm, r)];
        if let Some(t) = tags(a, mtm, side, r) {
            views.push(t);
        }
        views.push(spacer(mtm));
        views.push(to_view(&add));
        views.push(to_view(&block_toggle(mtm, ctx, side, r)));
        let refs: Vec<&NSView> = views.iter().map(|v| &**v).collect();
        let line = row(mtm, &refs);
        line.setCustomSpacing_afterView(18.0, &views[views.len() - 2]);
        rows.push(to_view(&line));
    }
    let header = format!("其他{}裝置", noun(side));
    Some(section(mtm, Some(&header), &rows, None))
}

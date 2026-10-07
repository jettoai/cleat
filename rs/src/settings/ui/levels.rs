//! Output volume hold, left/right balance and input volumes (Swift `SettingsView.outputLevels`,
//! `volumes`, `LevelRows.swift`).

use objc2::rc::Retained;
use objc2::sel;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSColor, NSMenu, NSMenuItem, NSModalResponseOK, NSOpenPanel, NSSlider, NSTextField, NSView};
use objc2_foundation::{NSArray, NSPoint, NSURL};
use objc2_uniform_type_identifiers::UTTypeApplication;

use super::super::store::Store;
use super::super::text::{self, balance_describe, now_text, percent_text};
use super::pages::device_label;
use super::widgets::{to_view, body, icon, keycap, ns, padded_column, plain_button, row, secondary, section, spacer, stack, width, AccentSwitch, Ctx};
use super::{app, App};

fn view<T: AsRef<NSView>>(v: &T) -> &NSView {
    v.as_ref()
}

/// `title ... 固定 [switch]`, the slider (dimmed while not fixed), and the now line.
fn pinned_row(
    mtm: MainThreadMarker,
    title: &str,
    fixed: bool,
    on_fixed: impl Fn(bool) + 'static,
    control: Option<Retained<NSView>>,
    now: &str,
) -> Retained<NSView> {
    let sw = AccentSwitch::new(mtm, fixed, true, on_fixed);
    let top = stack(mtm, false, 6.0, &[&body(mtm, title), &spacer(mtm), &body(mtm, "固定"), &sw]);
    let mut views: Vec<Retained<NSView>> = vec![to_view(&top)];
    if let Some(c) = control {
        if !fixed {
            super::disable_tree(&c);
            c.setAlphaValue(0.4);
        }
        views.push(c);
    }
    views.push(to_view(&secondary(mtm, now, 10.0)));
    let refs: Vec<&NSView> = views.iter().map(|v| &**v).collect();
    let col = padded_column(mtm, 8.0, 12.0, &refs);
    to_view(&col)
}

fn slider(mtm: MainThreadMarker, ctx: &mut Ctx, value: f64, max: f64, f: impl Fn(&NSSlider) + 'static) -> Retained<NSSlider> {
    let act = ctx.act(move |sender| {
        if let Some(s) = sender.downcast_ref::<NSSlider>() {
            f(s);
        }
    });
    // SAFETY: target and selector match `Action::fire:`.
    let s = unsafe { NSSlider::sliderWithValue_minValue_maxValue_target_action(value, 0.0, max, Some(&act), Some(sel!(fire:)), mtm) };
    s.setContinuous(true);
    s
}

/// 0...100 with the percent in a keycap.
fn percent_slider(mtm: MainThreadMarker, ctx: &mut Ctx, value: f64, set: impl Fn(&mut Store, f64) + 'static) -> Retained<NSView> {
    let (cap, cap_label) = keycap(mtm, &percent_text(value), 56.0);
    let cap_label: Retained<NSTextField> = cap_label;
    let s = slider(mtm, ctx, value, 100.0, move |s| {
        let v = s.doubleValue();
        cap_label.setStringValue(&ns(&percent_text(v)));
        app().edit_quiet(|st| set(st, v));
    });
    let line = stack(mtm, false, 8.0, &[&s, &cap]);
    to_view(&line)
}

/// 0 (left) ... 1 (right); within 0.02 of the centre snaps to it.
fn balance_slider(mtm: MainThreadMarker, ctx: &mut Ctx, value: f64) -> Retained<NSView> {
    let (cap, cap_label) = keycap(mtm, &balance_describe(value), 72.0);
    let s = slider(mtm, ctx, value, 1.0, move |s| {
        let raw = s.doubleValue();
        let v = if (raw - 0.5).abs() < 0.02 { 0.5 } else { raw };
        if v != raw {
            s.setDoubleValue(v);
        }
        cap_label.setStringValue(&ns(&balance_describe(v)));
        app().edit_quiet(|st| st.draft.balance = v);
    });
    let left = secondary(mtm, "左 L", 10.0);
    left.setTextColor(Some(&NSColor::labelColor()));
    width(&left, 30.0);
    let right = secondary(mtm, "R 右", 10.0);
    right.setTextColor(Some(&NSColor::labelColor()));
    right.setAlignment(objc2_app_kit::NSTextAlignment::Right);
    width(&right, 30.0);
    let track = stack(mtm, false, 6.0, &[&left, &s, &right]);
    let mark = icon(mtm, "arrowtriangle.up", 9.0, &NSColor::secondaryLabelColor());
    let col = stack(mtm, true, 2.0, &[&track, &mark]);
    col.setAlignment(objc2_app_kit::NSLayoutAttribute::CenterX);
    let line = stack(mtm, false, 8.0, &[&col, &cap]);
    line.setAlignment(objc2_app_kit::NSLayoutAttribute::Top);
    to_view(&line)
}

/// An .app picked from /Applications, as its name without ".app".
fn choose_app(mtm: MainThreadMarker) -> Option<String> {
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setDirectoryURL(Some(&NSURL::fileURLWithPath_isDirectory(&ns("/Applications"), true)));
    // SAFETY: a framework constant.
    let app_type = unsafe { UTTypeApplication };
    panel.setAllowedContentTypes(&NSArray::from_slice(&[app_type]));
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    panel.setPrompt(Some(&ns("加入")));
    if panel.runModal() != NSModalResponseOK {
        return None;
    }
    let url = panel.URL()?;
    let name = url.lastPathComponent()?.to_string();
    Some(name.strip_suffix(".app").unwrap_or(&name).to_string())
}

pub fn output_levels(a: &App, ctx: &mut Ctx) -> Retained<NSView> {
    let mtm = ctx.mtm;
    let (d, live, last_revert) = {
        let s = a.store.borrow();
        (s.draft.clone(), s.live.clone(), s.last_revert.clone())
    };
    let hold = d.hold_enabled;
    let sw = AccentSwitch::new(mtm, hold, true, |on| app().edit(move |s| s.draft.set_hold_enabled(on)));
    let head = row(mtm, &[&body(mtm, "輸出音量"), &spacer(mtm), &body(mtm, "被其他程式改掉時拉回"), &sw]);
    head.setCustomSpacing_afterView(6.0, &head.arrangedSubviews().objectAtIndex(2));
    let mut rows: Vec<Retained<NSView>> = vec![to_view(&head)];
    for name in &d.hold_against {
        let n = name.clone();
        let act = ctx.act(move |_| {
            let n = n.clone();
            app().edit(move |s| s.draft.remove_hold_app(&n))
        });
        let minus = plain_button(mtm, "minus.circle", "", 13.0, &NSColor::secondaryLabelColor(), &act);
        let r = row(mtm, &[view(&minus), &body(mtm, name), &spacer(mtm)]);
        if !hold {
            super::disable_tree(&r);
            r.setAlphaValue(0.6);
        }
        rows.push(to_view(&r));
    }
    let act = ctx.act(move |_| {
        if let Some(name) = choose_app(MainThreadMarker::new().expect("main thread")) {
            app().edit(move |s| s.draft.add_hold_app(&name));
        }
    });
    let add = plain_button(mtm, "plus.circle", "加入程式…", 13.0, &NSColor::labelColor(), &act);
    let r = row(mtm, &[view(&add), &spacer(mtm)]);
    if !hold {
        super::disable_tree(&r);
        r.setAlphaValue(0.6);
    }
    rows.push(to_view(&r));
    let revert = last_revert.map_or_else(|| "還沒有拉回紀錄".to_string(), |t| format!("最近一次：{t}"));
    rows.push(to_view(&row(mtm, &[&secondary(mtm, &revert, 10.0), &spacer(mtm)])));

    let no_balance = live.output_device.is_some() && live.balance.is_none();
    let now = if no_balance {
        format!("現在：{}不支援左右平衡，Cleat 不會動它", live.output_device.clone().unwrap_or_default())
    } else {
        let reading = live.balance.map(balance_describe);
        now_text(live.output_device.as_deref(), reading.as_deref(), "輸出")
    };
    let control = (!no_balance).then(|| balance_slider(mtm, ctx, d.balance));
    rows.push(pinned_row(mtm, "左右平衡", d.balance_enabled, |on| app().edit(move |s| s.draft.balance_enabled = on), control, &now));
    section(mtm, Some("輸出音量與平衡"), &rows, Some(text::OUTPUT_LEVELS_FOOTER))
}

pub fn volumes(a: &App, ctx: &mut Ctx) -> Retained<NSView> {
    let mtm = ctx.mtm;
    let (d, live, candidates) = {
        let s = a.store.borrow();
        (s.draft.clone(), s.live.clone(), s.volume_candidates())
    };
    let reading = live.input_volume.map(percent_text);
    let now = now_text(live.input_device.as_deref(), reading.as_deref(), "輸入");
    let wildcard = percent_slider(mtm, ctx, d.wildcard_percent, |s, v| s.draft.wildcard_percent = v);
    let mut rows = vec![pinned_row(
        mtm,
        "所有麥克風的預設音量",
        d.wildcard_enabled,
        |on| app().edit(move |s| s.draft.wildcard_enabled = on),
        Some(wildcard),
        &now,
    )];
    for (i, e) in d.named_volumes.iter().enumerate() {
        let entry = e.entry.clone();
        let act = ctx.act(move |_| {
            let entry = entry.clone();
            app().edit(move |s| s.draft.remove_volume(&entry))
        });
        let minus = plain_button(mtm, "minus.circle", "", 13.0, &NSColor::secondaryLabelColor(), &act);
        let name = device_label(mtm, &e.display_name, e.is_connected);
        width(&name, 150.0);
        let s = percent_slider(mtm, ctx, e.percent, move |s, v| {
            if let Some(x) = s.draft.named_volumes.get_mut(i) {
                x.percent = v;
            }
        });
        rows.push(to_view(&row(mtm, &[view(&minus), &name, &s])));
    }
    let list = candidates.clone();
    let act = ctx.act(move |sender| {
        let mtm = MainThreadMarker::new().expect("main thread");
        let Some(button) = sender.downcast_ref::<NSView>() else { return };
        let menu = NSMenu::new(mtm);
        let keep: Vec<_> = list
            .iter()
            .map(|name| {
                let n = name.clone();
                let a = super::widgets::Action::new(mtm, move |_| {
                    let n = n.clone();
                    let a = app();
                    let present = a.store.borrow().present.clone();
                    a.edit(move |s| s.draft.add_volume(&n, &present));
                });
                // SAFETY: target and selector match `Action::fire:`.
                let item = unsafe {
                    let i = NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &ns(name), Some(sel!(fire:)), &ns(""));
                    i.setTarget(Some(&a));
                    i
                };
                menu.addItem(&item);
                a
            })
            .collect();
        let h = button.frame().size.height;
        menu.popUpMenuPositioningItem_atLocation_inView(None, NSPoint::new(0.0, h + 4.0), Some(button));
        drop(keep);
    });
    let add = plain_button(mtm, "plus.circle", "新增麥克風", 13.0, &NSColor::labelColor(), &act);
    if candidates.is_empty() {
        add.setEnabled(false);
    }
    rows.push(to_view(&row(mtm, &[view(&add), &spacer(mtm)])));
    section(mtm, Some("輸入音量"), &rows, Some(text::VOLUMES_FOOTER))
}

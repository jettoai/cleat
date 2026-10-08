//! One pane: header with the daemon's vitals, the error banner, then the page's sections
//! (Swift `SettingsView.pageContent`, `paneHeader`, `errorBanner`, `headphones`, `VitalsBand`).

use objc2::rc::Retained;
use objc2::sel;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSBox, NSBoxType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont, NSFontDescriptor, NSView};

use super::super::draft::{HeadsetOption, Side};
use super::super::store::Phase;
use super::super::text::{self, Page};
use super::super::vitals::{DaemonVitals, VitalsState};
use super::widgets::{to_view, 
    attributed, card, fill, icon, label, ns, padded_column, plain_row, row, row_note, row_title, secondary, section, semibold, spacer, stack,
    wrapping, AccentSwitch, Ctx,
};
use super::{app, devices, disable_tree, levels, App};

pub fn page(a: &App, ctx: &mut Ctx) -> Retained<NSView> {
    let mtm = ctx.mtm;
    let page = a.page.get();
    let col = stack(mtm, true, 28.0, &[]);
    let add = |v: Retained<NSView>| {
        col.addArrangedSubview(&v);
        fill(&col, &v, 0.0);
    };
    if let Some(banner) = error_banner(a, ctx) {
        add(banner);
    }
    // Jetto voice settings page: 28 pt between the page header and each section.
    let content = stack(mtm, true, 28.0, &[]);
    let put = |v: Retained<NSView>| {
        content.addArrangedSubview(&v);
        fill(&content, &v, 0.0);
    };
    put(pane_header(mtm, page));
    put(card(mtm, &[vitals_band(mtm, &a.store.borrow().vitals)]));
    match page {
        Page::Output => {
            put(devices::priority(a, ctx, Side::Output));
            if let Some(o) = devices::others(a, ctx, Side::Output) {
                put(o);
            }
            put(levels::output_levels(a, ctx));
        }
        Page::Input => {
            put(devices::priority(a, ctx, Side::Input));
            if let Some(o) = devices::others(a, ctx, Side::Input) {
                put(o);
            }
            put(levels::volumes(a, ctx));
        }
        Page::Headphones => put(headphones(a, ctx)),
    }
    if a.store.borrow().phase != Phase::Ready {
        disable_tree(&content);
    }
    add(to_view(&content));
    to_view(&col)
}

/// Jetto voice `PageHeader`: 26 pt bold title over a 13 pt secondary line, 2 pt in, no card.
fn pane_header(mtm: MainThreadMarker, page: Page) -> Retained<NSView> {
    // SAFETY: an AppKit constant.
    let bold = unsafe { objc2_app_kit::NSFontWeightBold };
    let title = label(mtm, page.title(), 26.0, bold, &NSColor::labelColor());
    let words = stack(mtm, true, 4.0, &[&title, &secondary(mtm, page.subtitle(), 13.0)]);
    words.setEdgeInsets(objc2_foundation::NSEdgeInsets { top: 0.0, left: 2.0, bottom: 0.0, right: 0.0 });
    to_view(&words)
}

fn big_font() -> Retained<NSFont> {
    let base = NSFont::monospacedDigitSystemFontOfSize_weight(28.0, semibold());
    // SAFETY: an AppKit constant.
    let design = unsafe { objc2_app_kit::NSFontDescriptorSystemDesignRounded };
    let desc: Option<Retained<NSFontDescriptor>> = base.fontDescriptor().fontDescriptorWithDesign(design);
    desc.and_then(|d| NSFont::fontWithDescriptor_size(&d, 28.0)).unwrap_or(base)
}

fn cell(mtm: MainThreadMarker, name: &str, value: &str, note: &str, help: &str) -> Retained<NSView> {
    let v = label(mtm, value, 28.0, semibold(), &NSColor::labelColor());
    v.setFont(Some(&big_font()));
    // SAFETY: an AppKit constant.
    let medium = unsafe { objc2_app_kit::NSFontWeightMedium };
    let n = label(mtm, name, 12.0, medium, &NSColor::labelColor());
    let s = stack(mtm, true, 2.0, &[&v, &n, &secondary(mtm, note, 10.0)]);
    s.setToolTip(Some(&ns(help)));
    s.setEdgeInsets(objc2_foundation::NSEdgeInsets { top: 0.0, left: 14.0, bottom: 0.0, right: 0.0 });
    to_view(&s)
}

fn divider(mtm: MainThreadMarker) -> Retained<NSView> {
    let d = NSBox::new(mtm);
    d.setBoxType(NSBoxType::Separator);
    d.setTranslatesAutoresizingMaskIntoConstraints(false);
    d.heightAnchor().constraintEqualToConstant(52.0).setActive(true);
    d.widthAnchor().constraintEqualToConstant(1.0).setActive(true);
    to_view(&d)
}

/// Swift `VitalsBand`.
fn vitals_band(mtm: MainThreadMarker, v: &DaemonVitals) -> Retained<NSView> {
    let dot_color = match v.state {
        VitalsState::Running => NSColor::systemGreenColor(),
        VitalsState::NotRunning => NSColor::secondaryLabelColor(),
        VitalsState::Unreadable => NSColor::systemYellowColor(),
    };
    let dot = icon(mtm, "circle.fill", 7.0, &dot_color);
    let headline = label(mtm, text::vitals_headline(v.state), 10.0, semibold(), &NSColor::secondaryLabelColor());
    let top = stack(mtm, false, 6.0, &[&dot, &headline]);
    let cells = [
        cell(mtm, "CPU", &text::vitals_cpu(v), &text::vitals_cpu_note(v), text::CPU_HELP),
        cell(mtm, "記憶體", &text::vitals_memory(v), "實體記憶體", text::MEMORY_HELP),
        cell(mtm, "拉回速度", &text::reaction_value(v), &text::reaction_note(v), &text::reaction_help(v)),
    ];
    let grid = stack(mtm, false, 0.0, &[&cells[0], &divider(mtm), &cells[1], &divider(mtm), &cells[2]]);
    grid.setAlignment(objc2_app_kit::NSLayoutAttribute::Top);
    cells[0].widthAnchor().constraintEqualToAnchor(&cells[1].widthAnchor()).setActive(true);
    cells[1].widthAnchor().constraintEqualToAnchor(&cells[2].widthAnchor()).setActive(true);
    let col = padded_column(mtm, 10.0, 14.0, &[&top, &grid]);
    to_view(&col)
}

fn error_banner(a: &App, ctx: &mut Ctx) -> Option<Retained<NSView>> {
    let mtm = ctx.mtm;
    let s = a.store.borrow();
    let unreadable = match &s.phase {
        Phase::Unreadable(r) => Some(r.clone()),
        _ => None,
    };
    let message = unreadable
        .as_ref()
        .map(|r| format!("設定檔無法讀取：{r}。修好檔案後按重新載入。"))
        .or(s.error_message.clone())?;
    let red = NSColor::systemRedColor();
    let line = stack(mtm, false, 6.0, &[&icon(mtm, "exclamationmark.triangle.fill", 16.0, &red), &wrapping(mtm, &message, 13.0, &red)]);
    let mut rows = vec![to_view(&padded_column(mtm, 0.0, 10.0, &[&line]))];
    if s.has_conflict || unreadable.is_some() {
        let act = ctx.act(|_| app().reload());
        // SAFETY: target and selector match `Action::fire:`.
        let b = unsafe { NSButton::buttonWithTitle_target_action(&ns("重新載入"), Some(&act), Some(sel!(fire:)), mtm) };
        rows.push(to_view(&plain_row(mtm, &[&b, &spacer(mtm)])));
    }
    Some(section(mtm, None, &rows, None))
}

/// Label + switch. `spread`: switch at the trailing edge; otherwise right next to the words.
pub fn switch_row(mtm: MainThreadMarker, title: &str, on: bool, enabled: bool, f: impl Fn(bool) + 'static) -> Retained<NSView> {
    let sw = AccentSwitch::new(mtm, on, enabled, f);
    // Jetto voice `SettingsRow`: 15 pt title, at least 52 tall.
    let r = row(mtm, &[&row_title(mtm, title), &spacer(mtm), &sw]);
    to_view(&r)
}

/// A device name with "未連線" after it when nothing present matches.
pub fn device_label(mtm: MainThreadMarker, name: &str, connected: bool) -> Retained<NSView> {
    let s = stack(mtm, false, 6.0, &[&row_title(mtm, name)]);
    if !connected {
        s.addArrangedSubview(&row_note(mtm, "未連線"));
    }
    to_view(&s)
}

fn headset_row(mtm: MainThreadMarker, ctx: &mut Ctx, h: &HeadsetOption, reclaim: bool) -> Retained<NSView> {
    let entry = h.entry.clone();
    let act = ctx.act(move |sender| {
        let on = sender.downcast_ref::<NSButton>().is_some_and(|b| b.state() == NSControlStateValueOn);
        let entry = entry.clone();
        app().edit(move |s| s.draft.set_headset(&entry, on));
    });
    // SAFETY: target and selector match `Action::fire:`.
    let b = unsafe { NSButton::checkboxWithTitle_target_action(&ns(&h.display_name), Some(&act), Some(sel!(fire:)), mtm) };
    let title = attributed(&h.display_name, 15.0, &NSColor::labelColor());
    if !h.is_connected {
        let full = objc2_foundation::NSMutableAttributedString::from_attributed_nsstring(&title);
        full.appendAttributedString(&attributed(" 未連線", 13.0, &NSColor::secondaryLabelColor()));
        b.setAttributedTitle(&full);
    } else {
        b.setAttributedTitle(&title);
    }
    b.setState(if h.is_selected { NSControlStateValueOn } else { NSControlStateValueOff });
    if !reclaim {
        b.setEnabled(false);
    }
    let r = plain_row(mtm, &[&b, &spacer(mtm)]);
    r.setEdgeInsets(objc2_foundation::NSEdgeInsets { top: 10.0, left: 36.0, bottom: 10.0, right: 20.0 });
    if !reclaim {
        r.setAlphaValue(0.6);
    }
    to_view(&r)
}

fn headphones(a: &App, ctx: &mut Ctx) -> Retained<NSView> {
    let mtm = ctx.mtm;
    let d = a.store.borrow().draft.clone();
    let mut rows = vec![
        switch_row(mtm, "藍牙耳機連上時自動切過去", d.headphones_take_over, true, |on| {
            app().edit(move |s| s.draft.headphones_take_over = on)
        }),
        switch_row(mtm, "耳機被其他裝置拿走時要回來", d.reclaim_enabled, true, |on| {
            app().edit(move |s| s.draft.set_reclaim_enabled(on))
        }),
    ];
    let reclaim = d.reclaim_enabled;
    let audio: Vec<&HeadsetOption> = d.headsets.iter().filter(|h| !h.is_other).collect();
    let others: Vec<&HeadsetOption> = d.headsets.iter().filter(|h| h.is_other).collect();
    if audio.is_empty() {
        let r = plain_row(mtm, &[&secondary(mtm, "沒有找到配對過的藍牙耳機", 13.0), &spacer(mtm)]);
        r.setEdgeInsets(objc2_foundation::NSEdgeInsets { top: 10.0, left: 36.0, bottom: 10.0, right: 20.0 });
        if !reclaim {
            r.setAlphaValue(0.6);
        }
        rows.push(to_view(&r));
    }
    for h in audio {
        rows.push(headset_row(mtm, ctx, h, reclaim));
    }
    if !others.is_empty() {
        let expanded = a.others_expanded.get();
        let act = ctx.act(|_| {
            let a = app();
            a.others_expanded.set(!a.others_expanded.get());
            App::render_later();
        });
        let chevron = icon(mtm, if expanded { "chevron.down" } else { "chevron.right" }, 14.0, &NSColor::secondaryLabelColor());
        let note = wrapping(mtm, text::OTHERS_NOTE, 13.0, &NSColor::secondaryLabelColor());
        let words = stack(mtm, true, 2.0, &[&row_title(mtm, &format!("其他藍牙裝置（{}）", others.len())), &note]);
        fill(&words, &note, 0.0);
        // SAFETY: target and selector match `Action::fire:`.
        let hit = unsafe { NSButton::buttonWithTitle_target_action(&ns(""), Some(&act), Some(sel!(fire:)), mtm) };
        hit.setBordered(false);
        hit.setTransparent(true);
        let r = plain_row(mtm, &[&chevron, &words]);
        r.setAlignment(objc2_app_kit::NSLayoutAttribute::Top);
        r.setEdgeInsets(objc2_foundation::NSEdgeInsets { top: 10.0, left: 36.0, bottom: 10.0, right: 20.0 });
        // Jetto voice `SettingsRow`: at least 52 tall, taller with the content, 10 above and below.
        r.heightAnchor().constraintGreaterThanOrEqualToConstant(52.0).setActive(true);
        // The tap target pinned to the row hugs it to the button's height; this keeps a two-line
        // note's 10 pt below it.
        r.bottomAnchor().constraintGreaterThanOrEqualToAnchor_constant(&note.bottomAnchor(), 10.0).setActive(true);
        r.addSubview(&hit);
        hit.setTranslatesAutoresizingMaskIntoConstraints(false);
        fill(&r, &hit, 0.0);
        hit.topAnchor().constraintEqualToAnchor(&r.topAnchor()).setActive(true);
        hit.bottomAnchor().constraintEqualToAnchor(&r.bottomAnchor()).setActive(true);
        if !reclaim {
            r.setAlphaValue(0.6);
            hit.setEnabled(false);
        }
        rows.push(to_view(&r));
        if expanded {
            for h in others {
                rows.push(headset_row(mtm, ctx, h, reclaim));
            }
        }
    }
    if reclaim && !d.headsets.is_empty() && !d.headsets.iter().any(|h| h.is_selected) {
        let r = plain_row(mtm, &[&secondary(mtm, "請至少勾選一副耳機，否則不會有動作", 13.0), &spacer(mtm)]);
        rows.push(to_view(&r));
    }
    section(mtm, Some(("headphones", "耳機")), &rows, None)
}

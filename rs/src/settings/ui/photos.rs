//! The photos page: the HEIC to JPEG switch, the two folders and the long edge (`heic` in the config).

use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::sel;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSButton, NSColor, NSLineBreakMode, NSModalResponseOK, NSOpenPanel, NSControlStateValueOn, NSView};
use objc2_foundation::NSURL;

use super::super::draft::{SettingsDraft, LONG_EDGES};
use super::super::lang;
use super::super::text::{self, Page};
use super::super::words::W;
use super::widgets::{card, icon, ns, padded_column, row, row_title, secondary, section, spacer, stack, to_view, wrapping, Ctx};
use super::pages::switch_row;
use super::{app, disable_tree, App};
use crate::config::paths;

/// A folder picked in the open panel, starting where `current` already points.
fn choose_folder(mtm: MainThreadMarker, current: &str) -> Option<PathBuf> {
    let panel = NSOpenPanel::openPanel(mtm);
    let start = paths::expand_tilde(&paths::home(), current);
    panel.setDirectoryURL(Some(&NSURL::fileURLWithPath_isDirectory(&ns(&start.to_string_lossy()), true)));
    panel.setCanChooseFiles(false);
    panel.setCanChooseDirectories(true);
    panel.setCanCreateDirectories(true);
    panel.setAllowsMultipleSelection(false);
    panel.setPrompt(Some(&ns(W::PanelChoose.get(lang::current()))));
    if panel.runModal() != NSModalResponseOK {
        return None;
    }
    panel.URL()?.path().map(|p| PathBuf::from(p.to_string()))
}

/// Title, the folder as the config names it, and a button that picks another.
fn folder_row(
    ctx: &mut Ctx,
    title: W,
    value: &str,
    pick: fn(&mut SettingsDraft, &std::path::Path, &std::path::Path),
) -> Retained<NSView> {
    let mtm = ctx.mtm;
    let l = lang::current();
    let current = value.to_string();
    let act = ctx.act(move |_| {
        if let Some(path) = choose_folder(MainThreadMarker::new().expect("main thread"), &current) {
            app().edit(move |s| pick(&mut s.draft, &path, &paths::home()));
        }
    });
    // SAFETY: target and selector match `Action::fire:`.
    let button = unsafe { NSButton::buttonWithTitle_target_action(&ns(W::ChooseFolder.get(l)), Some(&act), Some(sel!(fire:)), mtm) };
    let path = secondary(mtm, value, 13.0);
    path.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    path.setContentCompressionResistancePriority_forOrientation(250.0, objc2_app_kit::NSLayoutConstraintOrientation::Horizontal);
    to_view(&row(mtm, &[&row_title(mtm, title.get(l)), &spacer(mtm), &path, &button]))
}

fn long_edge_row(ctx: &mut Ctx, chosen: u32) -> Retained<NSView> {
    let mtm = ctx.mtm;
    let l = lang::current();
    let title = row_title(mtm, W::LongEdge.get(l));
    let mut views: Vec<Retained<NSView>> = vec![to_view(&title), spacer(mtm)];
    for edge in LONG_EDGES {
        let act = ctx.act(move |_| app().edit(move |s| s.draft.heic_long_edge = edge));
        let name = if edge == 0 { W::OriginalSize.get(l).to_string() } else { edge.to_string() };
        // SAFETY: target and selector match `Action::fire:`.
        let radio = unsafe { NSButton::radioButtonWithTitle_target_action(&ns(&name), Some(&act), Some(sel!(fire:)), mtm) };
        radio.setState(if edge == chosen { NSControlStateValueOn } else { objc2_app_kit::NSControlStateValueOff });
        views.push(to_view(&radio));
    }
    let refs: Vec<&NSView> = views.iter().map(|v| &**v).collect();
    let r = row(mtm, &refs);
    r.setSpacing(14.0);
    to_view(&r)
}

/// The page's views, top to bottom: the orange line when the folder cannot be read, then the section.
pub fn views(a: &App, ctx: &mut Ctx) -> Vec<Retained<NSView>> {
    let mtm = ctx.mtm;
    let l = lang::current();
    let (d, no_access) = {
        let s = a.store.borrow();
        (s.draft.clone(), s.photos_no_access)
    };
    let on = d.convert_heic;
    let mut out = vec![];
    if on && no_access {
        let orange = NSColor::systemOrangeColor();
        let words = text::fill(W::PhotosNoAccess.get(l), &[&d.heic_watch]);
        let line = stack(mtm, false, 6.0, &[&icon(mtm, "exclamationmark.circle", 14.0, &orange), &wrapping(mtm, &words, 13.0, &orange)]);
        out.push(card(mtm, &[to_view(&padded_column(mtm, 0.0, 10.0, &[&line]))]));
    }
    let switch = switch_row(mtm, W::ConvertHeic.get(l), on, true, |on| app().edit(move |s| s.draft.convert_heic = on));
    let rest = [
        folder_row(ctx, W::WatchFolder, &d.heic_watch, SettingsDraft::set_heic_watch),
        folder_row(ctx, W::OriginalsFolder, &d.heic_originals, SettingsDraft::set_heic_originals),
        long_edge_row(ctx, d.heic_long_edge),
    ];
    if !on {
        for r in &rest {
            disable_tree(r);
            r.setAlphaValue(0.6);
        }
    }
    let rows: Vec<Retained<NSView>> = std::iter::once(switch).chain(rest).collect();
    out.push(section(mtm, Some(("photo", Page::Photos.title(l))), &rows, Some(W::PhotosNote.get(l))));
    out
}

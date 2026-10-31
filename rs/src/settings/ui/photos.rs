//! The photos page: the HEIC to JPEG switch, the two folders, the long edge and the quality (`heic` in the config).

use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::sel;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSButton, NSColor, NSLineBreakMode, NSModalResponseOK, NSOpenPanel, NSControlStateValueOn, NSView};
use objc2_foundation::NSURL;

use super::super::draft::{SettingsDraft, LONG_EDGES, QUALITIES};
use super::super::lang;
use super::super::text::{self, Page};
use super::super::words::W;
use super::widgets::{card, icon, ns, padded_column, row, row_title, secondary, section, spacer, stack, to_view, wrapping, Ctx};
use super::pages::switch_row;
use super::{app, disable_tree, App};
use crate::config::{paths, HeicConfig};

/// Where the open panel starts for a folder the config spells as `value`: the folder itself, or the
/// nearest existing one above it (the originals folder is only made on the first conversion), or `home`.
/// With a `base`, a relative `value` sits inside it, as `HeicConfig::originals_in` resolves it.
fn start_folder(value: &str, base: Option<&str>, home: &Path) -> PathBuf {
    let dir = match base {
        Some(base) => HeicConfig::originals_in(&paths::expand_tilde(home, base), value, home),
        None => paths::expand_tilde(home, value),
    };
    dir.ancestors().find(|p| p.is_dir()).map_or_else(|| home.to_path_buf(), Path::to_path_buf)
}

/// A folder picked in the open panel, starting where `current` already points.
fn choose_folder(mtm: MainThreadMarker, current: &str, base: Option<&str>) -> Option<PathBuf> {
    let panel = NSOpenPanel::openPanel(mtm);
    let start = start_folder(current, base, &paths::home());
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
/// `base` is the folder a relative `value` sits in (the originals folder sits in the watched one).
fn folder_row(
    ctx: &mut Ctx,
    title: W,
    value: &str,
    base: Option<&str>,
    pick: fn(&mut SettingsDraft, &std::path::Path, &std::path::Path),
) -> Retained<NSView> {
    let mtm = ctx.mtm;
    let l = lang::current();
    let (current, base) = (value.to_string(), base.map(str::to_string));
    let act = ctx.act(move |_| {
        if let Some(path) = choose_folder(MainThreadMarker::new().expect("main thread"), &current, base.as_deref()) {
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

/// A title and one radio button per `(value, label)`; `set` stores the picked value in the draft.
fn choice_row(ctx: &mut Ctx, title: W, chosen: u32, options: &[(u32, String)], set: fn(&mut SettingsDraft, u32)) -> Retained<NSView> {
    let mtm = ctx.mtm;
    let l = lang::current();
    let title = row_title(mtm, title.get(l));
    let mut views: Vec<Retained<NSView>> = vec![to_view(&title), spacer(mtm)];
    for (value, name) in options.iter().cloned() {
        let act = ctx.act(move |_| app().edit(move |s| set(&mut s.draft, value)));
        // SAFETY: target and selector match `Action::fire:`.
        let radio = unsafe { NSButton::radioButtonWithTitle_target_action(&ns(&name), Some(&act), Some(sel!(fire:)), mtm) };
        radio.setState(if value == chosen { NSControlStateValueOn } else { objc2_app_kit::NSControlStateValueOff });
        views.push(to_view(&radio));
    }
    let refs: Vec<&NSView> = views.iter().map(|v| &**v).collect();
    let r = row(mtm, &refs);
    r.setSpacing(14.0);
    to_view(&r)
}

fn long_edge_options() -> Vec<(u32, String)> {
    let l = lang::current();
    LONG_EDGES.iter().map(|&e| (e, if e == 0 { W::OriginalSize.get(l).to_string() } else { e.to_string() })).collect()
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
        folder_row(ctx, W::WatchFolder, &d.heic_watch, None, SettingsDraft::set_heic_watch),
        folder_row(ctx, W::OriginalsFolder, &d.heic_originals, Some(&d.heic_watch), SettingsDraft::set_heic_originals),
        choice_row(ctx, W::LongEdge, d.heic_long_edge, &long_edge_options(), |s, v| s.heic_long_edge = v),
        choice_row(ctx, W::Quality, d.heic_quality, &QUALITIES.map(|q| (q, q.to_string())), |s, v| s.heic_quality = v),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch home holding `Downloads/` (and nothing under it).
    fn home() -> PathBuf {
        let home = std::env::temp_dir().join(format!("cleat-start-folder-{}", std::process::id()));
        std::fs::create_dir_all(home.join("Downloads")).unwrap();
        home
    }

    #[test]
    fn the_panel_starts_at_the_folder_or_the_nearest_one_above_it() {
        let home = home();
        let watch = Some("~/Downloads");
        // Relative value sits inside the watch folder, which exists but the originals folder does not yet.
        assert_eq!(start_folder("HEIC原檔", watch, &home), home.join("Downloads"));
        std::fs::create_dir_all(home.join("Downloads/HEIC原檔")).unwrap();
        assert_eq!(start_folder("HEIC原檔", watch, &home), home.join("Downloads/HEIC原檔"));
        // Absolute and `~` values ignore the base.
        assert_eq!(start_folder(&home.join("Downloads").display().to_string(), watch, &home), home.join("Downloads"));
        assert_eq!(start_folder("~/Downloads/HEIC原檔", Some("/nonexistent"), &home), home.join("Downloads/HEIC原檔"));
        // Nothing exists above a missing absolute value but the root: back to home only when no ancestor is a folder.
        assert_eq!(start_folder("~/Downloads/a/b/c", None, &home), home.join("Downloads"));
        // The watch row has no base.
        assert_eq!(start_folder("~/Downloads", None, &home), home.join("Downloads"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_relative_value_with_nothing_existing_falls_back_to_home() {
        let home = home();
        assert_eq!(start_folder("HEIC原檔", Some("nope/nowhere"), &home), home);
        assert_eq!(start_folder("sub", None, &home), home);
        let _ = std::fs::remove_dir_all(&home);
    }
}

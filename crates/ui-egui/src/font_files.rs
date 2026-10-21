//! Fonts the user adds without installing them: a font file or a folder of fonts, picked in a
//! dialog (the font picker's footer, Preferences ▸ Fonts, the missing-fonts dialog) or dropped on
//! the window. They go through `text.loadFont` / `text.addFontFolder`, which remember them for the
//! next start.

use std::path::Path;

use egui::Id;
use serde_json::json;

use crate::FigApp;
use crate::i18n::{tr, trf, trn};

/// The font file extensions the app reads.
pub const FONT_EXTS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];

/// What a font picker asked for this frame (it has no app to open a dialog with).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    File,
    Folder,
}

fn request_id() -> Id {
    Id::new("font_files_request")
}

/// Ask for a font file or folder dialog; [`poll`] opens it at the end of the frame.
pub fn request(ctx: &egui::Context, r: Request) {
    ctx.data_mut(|d| d.insert_temp(request_id(), r));
}

/// Open the dialog a font picker asked for.
pub fn poll(app: &mut FigApp, ctx: &egui::Context) {
    let asked = ctx.data_mut(|d| {
        let r = d.get_temp::<Request>(request_id());
        d.remove::<Request>(request_id());
        r
    });
    match asked {
        Some(Request::File) => pick_file(app),
        Some(Request::Folder) => pick_folder(app),
        None => {}
    }
}

/// `path` names a font file (by its extension).
pub fn is_font(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| FONT_EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Pick a font file and load it.
pub fn pick_file(app: &mut FigApp) {
    app.pick_open(tr("Load font file"), &FONT_EXTS, |app, p| {
        load_file(app, &p);
    });
}

/// Pick a folder and add the fonts in it.
pub fn pick_folder(app: &mut FigApp) {
    app.pick_folder(tr("Add font folder"), |app, p| {
        load_folder(app, &p);
    });
}

/// Load font file `path`, say which families it added, and keep it for the next start.
pub fn load_file(app: &mut FigApp, path: &Path) -> bool {
    let Some(r) = app.run("text.loadFont", json!({"path": path.to_string_lossy()})) else { return false };
    added(app, &r);
    true
}

/// Add the fonts in folder `path`, say which families it added, and keep it for the next start.
pub fn load_folder(app: &mut FigApp, path: &Path) -> bool {
    let Some(r) = app.run("text.addFontFolder", json!({"path": path.to_string_lossy()})) else { return false };
    added(app, &r);
    true
}

fn added(app: &mut FigApp, r: &serde_json::Value) {
    let families: Vec<&str> = r["families"].as_array().map(|a| a.iter().filter_map(|f| f.as_str()).collect()).unwrap_or_default();
    let lang = crate::i18n::current().map_or("en", |l| l.code);
    let mut shown: Vec<String> = families.iter().take(3).map(|f| figcraft_text::family_display_name(f, lang)).collect();
    if families.len() > shown.len() {
        shown.push(trf("{n} more", &[("n", &(families.len() - shown.len()))]));
    }
    app.toast(trf("Fonts added: {families}", &[("families", &shown.join(", "))]));
    app.services.save_prefs(&app.session.prefs);
    crate::missing_fonts::refresh(app);
}

/// Walk the system font folders again (fonts installed while the app runs).
pub fn rescan(app: &mut FigApp) {
    if let Some(r) = app.run("text.rescanFonts", json!({})) {
        let n = r["added"].as_array().map_or(0, Vec::len);
        app.toast(trn("{n} new font families", n, &[]));
        crate::missing_fonts::refresh(app);
    }
}

/// Preferences ▸ Fonts: the font files and folders added (loaded again at every start), each with
/// a Remove button, and buttons to add more or look for fonts installed since.
pub fn prefs_section(app: &mut FigApp, ui: &mut egui::Ui) {
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(tr("Fonts")).font(crate::theme::semibold(12.0)));
    let prefs = &app.session.prefs;
    let added: Vec<(&'static str, &'static str, String)> = prefs
        .font_sources
        .iter()
        .map(|p| ("text.fontSources", "file", p.clone()))
        .chain(prefs.font_folders.iter().map(|p| ("text.fontFolders", "folder", p.clone())))
        .collect();
    if added.is_empty() {
        crate::widgets::label(ui, tr("Fonts you add from a file or a folder show here and load again whenever FigCraft starts."), t.text_secondary);
    }
    let mut remove = None;
    egui::ScrollArea::vertical().id_salt("pref_fonts").max_height(160.0).show(ui, |ui| {
        for (cmd, kind, path) in &added {
            ui.horizontal(|ui| {
                let name = Path::new(path).file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
                let shown = if *kind == "folder" { format!("{name}/") } else { name };
                ui.label(egui::RichText::new(shown).font(crate::theme::regular(11.5))).on_hover_text(path);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::widgets::button(ui, tr("Remove"), false, true).on_hover_text(tr("Not loaded at the next start")).clicked() {
                        remove = Some((*cmd, path.clone()));
                    }
                });
            });
        }
    });
    if let Some((cmd, path)) = remove {
        app.run(cmd, json!({"remove": path}));
        app.services.save_prefs(&app.session.prefs);
    }
    ui.horizontal(|ui| {
        if crate::widgets::button(ui, tr("Load font file…"), false, true).clicked() {
            request(ui.ctx(), Request::File);
        }
        if crate::widgets::button(ui, tr("Add font folder…"), false, true).clicked() {
            request(ui.ctx(), Request::Folder);
        }
        if crate::widgets::button(ui, tr("Rescan fonts"), false, true)
            .on_hover_text(tr("Look again for fonts installed since FigCraft started"))
            .clicked()
        {
            rescan(app);
        }
    });
}

/// Font files and folders dropped on the window load as fonts; returns the files left to import
/// or open.
pub fn take_dropped(app: &mut FigApp, files: Vec<egui::DroppedFileHandle>) -> Vec<egui::DroppedFileHandle> {
    let mut rest = Vec::new();
    for f in files {
        let path = f.path().to_path_buf();
        if path.as_os_str().is_empty() {
            rest.push(f);
        } else if is_font(&path) {
            load_file(app, &path);
        } else if path.is_dir() {
            load_folder(app, &path);
        } else {
            rest.push(f);
        }
    }
    rest
}

use super::{Align2, CornerRadius, Icon, Id, Rect, Sense, Tokens, Ui, icons, pos2, regular, semibold, tr, vec2, widgets};

// ---- Font picker ----

const RECENT_KEY: &str = "figcraft_recent_fonts";

const ROW: f32 = 26.0;

/// The font list's tallest (it scrolls past that).
const LIST_HEIGHT: f32 = 380.0;
/// The search field's height.
const SEARCH_H: f32 = 28.0;

/// Fonts that aren't installed, from a file or a folder ([`crate::font_files`] opens the dialog).
fn load_buttons(ui: &mut Ui) {
    ui.horizontal(|ui| {
        if widgets::button(ui, tr("Load font file…"), false, true).clicked() {
            crate::font_files::request(ui.ctx(), crate::font_files::Request::File);
            ui.close();
        }
        if widgets::button(ui, tr("Add font folder…"), false, true).clicked() {
            crate::font_files::request(ui.ctx(), crate::font_files::Request::Folder);
            ui.close();
        }
    });
}

/// The UI language's code, for fonts' localized names.
fn ui_lang() -> &'static str {
    crate::i18n::current().map_or("en", |l| l.code)
}

/// `family` as the UI shows it: its name in the UI language when the font has one (나눔고딕 for
/// NanumGothic), else as stored.
pub(crate) fn family_label(family: &str) -> String {
    figcraft_text::family_display_name(family, ui_lang())
}

/// Return the persisted list of recently used font families.
fn recent_families(ctx: &egui::Context) -> Vec<String> {
    ctx.data_mut(|d| d.get_persisted::<Vec<String>>(Id::unique(RECENT_KEY))).unwrap_or_default()
}

/// Remember a font family at the front of the recent list, removing an earlier
/// case-insensitive match and keeping at most eight entries.
pub(super) fn remember_family(ctx: &egui::Context, family: &str) {
    ctx.data_mut(|d| {
        let v = d.get_persisted_mut_or_default::<Vec<String>>(Id::unique(RECENT_KEY));
        v.retain(|f| !f.eq_ignore_ascii_case(family));
        v.insert(0, family.to_string());
        v.truncate(8);
    });
}

enum PickRow {
    Header(&'static str),
    Family(figcraft_text::FamilyName),
}

/// What the family list did this frame.
#[derive(Default)]
pub(crate) struct FontPick {
    /// The family clicked, or the one Enter picked.
    pub picked: Option<String>,
    /// The family Up and Down are on (Enter picks it).
    pub highlighted: Option<String>,
    /// The family the pointer is over (the caller previews it on the canvas).
    pub hovered: Option<String>,
}

/// The most the font menu is tall (its search field and the "Apply variable" row above the list
/// included, and the font file and folder buttons under it); the list scrolls past that.
const MENU_HEIGHT: f32 = LIST_HEIGHT + 80.0;

/// What the menu's header worked out for its list.
#[derive(Default)]
struct Header {
    rows: Vec<PickRow>,
    /// The row Up and Down are on.
    hi: Option<usize>,
    /// The rows Up and Down moved this frame (the list scrolls to the row they are on).
    step: i64,
    /// Enter picked this family.
    entered: Option<String>,
}

/// The searchable family menu under `anchor`: click a family, or step through it with Up and Down
/// and press Enter. Hovering a family previews it on the canvas (closing without picking restores
/// the original). `before_search` draws the rows above the search field and returns true when one
/// of them handled the menu (it closed it). None while the menu is closed.
///
/// The search field and those rows stay in place; only the list scrolls. Headers ("Recent",
/// "All fonts") are as tall as family rows, so the list is uniform and virtualised.
pub(crate) fn font_picker(
    ui: &Ui,
    anchor: &egui::Response,
    current: Option<&str>,
    width: f32,
    before_search: impl FnOnce(&mut Ui) -> bool,
) -> Option<FontPick> {
    // The menu is dark in both themes ([`crate::right::menu_style`]).
    let t = crate::theme::with_dark_tokens(ui.ctx(), || Tokens::get(ui.ctx()));
    let just_opened = anchor.clicked();
    let hi_id = Id::unique("font_highlight");
    let q_id = Id::unique("font_query");
    craft_ui::combo::Menu::new(ui, anchor)
        .height(MENU_HEIGHT)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .style(crate::right::menu_style)
        .show_rows_with_header(
            |ui| {
                // The menu is at least `width` wide, even under a narrower field.
                ui.set_min_width(width);
                if before_search(ui) {
                    return Header::default();
                }
                let mut q: String = ui.data(|d| d.get_temp(q_id)).unwrap_or_default();
                if just_opened {
                    q.clear();
                    ui.data_mut(|d| d.remove::<(String, usize)>(hi_id));
                }
                // Taken before the search field sees them (it would move its cursor).
                let step = ui.input_mut(|i| {
                    i.count_and_consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) as i64
                        - i.count_and_consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) as i64
                });
                ui.add_space(2.0);
                let search = ui.add(
                    egui::TextEdit::singleline(&mut q)
                        .hint_text(tr("Search fonts"))
                        .desired_width(ui.available_width())
                        .min_size(vec2(0.0, SEARCH_H))
                        .margin(vec2(8.0, 5.0))
                        .align(egui::Align2::LEFT_CENTER)
                        .font(regular(13.0)),
                );
                crate::widgets::select_all_on_focus(ui, &search, &q);
                if just_opened {
                    search.request_focus();
                }
                ui.data_mut(|d| d.insert_temp(q_id, q.clone()));
                load_buttons(ui);
                let ql = q.trim().to_lowercase();
                // Found by any of its names: "나눔" finds NanumGothic.
                let all = figcraft_text::family_list(ui_lang());
                let hit = |f: &figcraft_text::FamilyName| {
                    ql.is_empty()
                        || f.family.to_lowercase().contains(&ql)
                        || f.display.to_lowercase().contains(&ql)
                        || f.localized.iter().any(|n| n.to_lowercase().contains(&ql))
                };
                let matches: Vec<figcraft_text::FamilyName> = all.iter().filter(|f| hit(f)).cloned().collect();
                let mut rows: Vec<PickRow> = Vec::new();
                if ql.is_empty() {
                    let recent: Vec<figcraft_text::FamilyName> = recent_families(ui.ctx())
                        .into_iter()
                        .filter_map(|r| all.iter().find(|a| a.family.eq_ignore_ascii_case(&r)).cloned())
                        .collect();
                    if !recent.is_empty() {
                        rows.push(PickRow::Header(tr("Recent")));
                        rows.extend(recent.into_iter().map(PickRow::Family));
                        rows.push(PickRow::Header(tr("All fonts")));
                    }
                }
                if matches.is_empty() {
                    widgets::label(ui, tr("No fonts match"), t.text_tertiary);
                    return Header::default();
                }
                rows.extend(matches.into_iter().map(PickRow::Family));
                let family = |i: usize| match rows.get(i) {
                    Some(PickRow::Family(f)) => Some(f.family.clone()),
                    _ => None,
                };
                // A new search starts again; the first step starts from the current family.
                let mut hi = ui.data(|d| d.get_temp::<(String, usize)>(hi_id)).filter(|(hq, _)| *hq == q).map(|(_, i)| i);
                if step != 0 {
                    let next = |i: usize| if step > 0 { i.checked_add(1) } else { i.checked_sub(1) };
                    let mut from = hi.or_else(|| {
                        rows.iter().rposition(|r| matches!(r, PickRow::Family(f) if current.is_some_and(|c| c.eq_ignore_ascii_case(&f.family))))
                    });
                    for _ in 0..step.unsigned_abs() {
                        let mut at = match from {
                            Some(i) => next(i),
                            None if step > 0 => Some(0),
                            None => rows.len().checked_sub(1),
                        };
                        // Past the headers; at either end it stays where it was.
                        while let Some(i) = at.filter(|i| *i < rows.len() && family(*i).is_none()) {
                            at = next(i);
                        }
                        if let Some(i) = at.filter(|i| family(*i).is_some()) {
                            from = Some(i);
                            hi = Some(i);
                        }
                    }
                }
                match hi {
                    Some(i) => ui.data_mut(|d| {
                        d.insert_temp(hi_id, (q.clone(), i));
                    }),
                    None => ui.data_mut(|d| d.remove::<(String, usize)>(hi_id)),
                }
                let mut entered = None;
                if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let first = rows.iter().find_map(|r| match r {
                        PickRow::Family(f) => Some(f.family.clone()),
                        PickRow::Header(_) => None,
                    });
                    entered = hi.and_then(family).or(first);
                    ui.close();
                }
                Header { rows, hi, step, entered }
            },
            ROW,
            |h| h.rows.len(),
            |ui, range, h| {
                if let Some(f) = h.entered {
                    return FontPick { picked: Some(f), highlighted: None, hovered: None };
                }
                if let (Some(i), true) = (h.hi, h.step != 0) {
                    // Scrolled just enough to show the row Up or Down moved to; it may be out of
                    // the range laid out, so its rect comes from the list's top.
                    let stride = ROW + ui.spacing().item_spacing.y;
                    let top = ui.max_rect().top() - range.start as f32 * stride + i as f32 * stride;
                    ui.scroll_to_rect(Rect::from_min_size(pos2(ui.max_rect().left(), top), vec2(ui.max_rect().width(), ROW)), None);
                }
                let mut budget = 4;
                let mut picked = None;
                let mut hovered_family = None;
                let hi = h.hi;
                for (i, r) in h.rows.iter().enumerate().skip(range.start).take(range.len()) {
                    let w = ui.available_width();
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(w, ROW), if matches!(r, PickRow::Family(_)) { Sense::click() } else { Sense::hover() });
                    match r {
                        PickRow::Header(label) => {
                            ui.painter().text(pos2(rect.min.x + 8.0, rect.center().y), Align2::LEFT_CENTER, *label, semibold(10.5), t.text_secondary);
                        }
                        PickRow::Family(f) => {
                            let hovered = resp.hovered() || hi == Some(i);
                            if hovered {
                                ui.painter().rect_filled(rect, CornerRadius::same(5), t.accent);
                            }
                            let col = if hovered { t.accent_on } else { t.text };
                            if current.is_some_and(|c| c.eq_ignore_ascii_case(&f.family)) {
                                icons::paint(
                                    ui.painter(),
                                    Rect::from_center_size(pos2(rect.min.x + 10.0, rect.center().y), vec2(12.0, 12.0)),
                                    Icon::Check,
                                    col,
                                );
                            }
                            let text_min = pos2(rect.min.x + 22.0, rect.min.y);
                            match crate::font_preview::family_named(ui.ctx(), &f.family, &f.display, 14.0, &mut budget) {
                                Some(tex) => {
                                    crate::font_preview::paint(
                                        ui.painter(),
                                        &tex,
                                        pos2(text_min.x, rect.center().y),
                                        rect.max.x - text_min.x - 6.0,
                                        ROW,
                                        col,
                                    );
                                }
                                None => {
                                    ui.painter().text(pos2(text_min.x, rect.center().y), Align2::LEFT_CENTER, &f.display, regular(13.0), col);
                                }
                            }
                            if resp.clicked() {
                                picked = Some(f.family.clone());
                                ui.close();
                            }
                            if resp.hovered() {
                                hovered_family = Some(f.family.clone());
                            }
                            // A localized name tells what documents store.
                            if f.display != f.family {
                                resp.on_hover_text(&f.family);
                            }
                        }
                    }
                }
                let highlighted = hi.and_then(|i| match h.rows.get(i) {
                    Some(PickRow::Family(f)) => Some(f.family.clone()),
                    _ => None,
                });
                FontPick { highlighted, picked, hovered: hovered_family }
            },
        )
}

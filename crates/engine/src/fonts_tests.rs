use serde_json::json;

use super::*;
use crate::params::id_of;
use figcraft_doc::{Style, StyleRun, TextStyle};

fn text(s: &mut Session, family: &str) -> figcraft_doc::NodeId {
    let id = id_of(&s.execute("text.create", &json!({"text": "Hamburgefonstiv", "x": 0, "y": 0})).unwrap()["id"]).unwrap();
    if let NodeKind::Text(t) = &mut s.doc_mut().unwrap().get_mut(id).unwrap().kind {
        t.style.font_family = family.into();
    }
    id
}

fn families(s: &mut Session) -> Vec<(String, u64)> {
    let r = s.execute("text.missingFonts", &json!({})).unwrap();
    r["families"].as_array().unwrap().iter().map(|f| (f["family"].as_str().unwrap().to_string(), f["layers"].as_u64().unwrap())).collect()
}

/// An installed family (built in, or found by the system scan), if this machine has any.
fn installed() -> Option<String> {
    figcraft_text::scan_system_fonts();
    figcraft_text::family_names().into_iter().next()
}

#[test]
fn lists_families_of_layers_ranges_styles_and_variables_once_each() {
    let mut s = Session::new();
    text(&mut s, "FigCraft Missing A");
    let b = text(&mut s, "figcraft missing a");
    if let NodeKind::Text(t) = &mut s.doc_mut().unwrap().get_mut(b).unwrap().kind {
        let style = TextStyle { font_family: "FigCraft Missing B".into(), ..t.style.clone() };
        t.runs.push(StyleRun { start: 0, end: 3, style });
    }
    let doc = s.doc_mut().unwrap();
    doc.styles.push(Style {
        id: "S:1".into(),
        name: "Heading".into(),
        description: String::new(),
        kind: StyleKind::Text { style: TextStyle { font_family: "FigCraft Missing C".into(), ..TextStyle::default() } },
        bound: Default::default(),
    });
    s.execute("variable.createCollection", &json!({"name": "Type", "modes": ["Light", "Dark"]})).unwrap();
    s.execute("variable.create", &json!({"collection": "Type", "name": "Family", "type": "string", "value": "FigCraft Missing D"})).unwrap();
    s.execute("variable.set", &json!({"variable": "Family", "mode": "Dark", "value": "FigCraft Missing H"})).unwrap();
    let var = s.doc().unwrap().variables.get("Family").unwrap().id.clone();
    s.doc_mut().unwrap().get_mut(b).unwrap().bound.insert("fontFamily".into(), var);

    assert_eq!(
        families(&mut s),
        vec![
            ("FigCraft Missing A".to_string(), 2),
            ("FigCraft Missing B".to_string(), 1),
            ("FigCraft Missing C".to_string(), 0),
            ("FigCraft Missing D".to_string(), 0),
            ("FigCraft Missing H".to_string(), 0),
        ]
    );
    assert!(s.ui_requests.iter().any(|r| r.kind == "dialog.missingFonts"));
}

#[test]
fn installed_families_are_not_listed() {
    let Some(family) = installed() else {
        eprintln!("skipped: no fonts on this machine");
        return;
    };
    let mut s = Session::new();
    text(&mut s, &family);
    text(&mut s, &family.to_uppercase());
    text(&mut s, "FigCraft Missing E");
    assert_eq!(families(&mut s), vec![("FigCraft Missing E".to_string(), 1)]);
}

#[test]
fn a_family_found_only_by_the_system_scan_is_not_missing() {
    figcraft_text::scan_system_fonts();
    let embedded: Vec<String> = figcraft_text::CRAFT_FONTS.iter().map(|f| f.family.to_lowercase()).collect();
    let Some(family) = figcraft_text::family_names().into_iter().find(|f| !embedded.contains(&f.to_lowercase())) else {
        eprintln!("skipped: no system fonts on this machine");
        return;
    };
    let mut s = Session::new();
    text(&mut s, &family);
    assert!(families(&mut s).is_empty(), "{family} is installed");
}

#[test]
fn blank_families_and_aliases_are_handled() {
    let mut s = Session::new();
    text(&mut s, "  ");
    s.execute("variable.createCollection", &json!({"name": "Type"})).unwrap();
    s.execute("variable.create", &json!({"collection": "Type", "name": "Base", "type": "string", "value": "FigCraft Missing F"})).unwrap();
    let base = s.doc().unwrap().variables.get("Base").unwrap().id.clone();
    let t = text(&mut s, "FigCraft Missing G");
    let doc = s.doc_mut().unwrap();
    // A variable that only aliases another, and one aliasing itself.
    let mut alias = doc.variables.get(&base).unwrap().clone();
    alias.id = "V:alias".into();
    alias.name = "Alias".into();
    for v in alias.values.values_mut() {
        *v = VariableValue::Alias(base.clone());
    }
    let mut cycle = alias.clone();
    cycle.id = "V:cycle".into();
    cycle.name = "Cycle".into();
    for v in cycle.values.values_mut() {
        *v = VariableValue::Alias("V:cycle".into());
    }
    doc.variables.variables.push(alias);
    doc.variables.variables.push(cycle);
    doc.get_mut(t).unwrap().bound.insert("fontFamily".into(), "V:alias".into());
    let other = text(&mut s, "FigCraft Missing G");
    s.doc_mut().unwrap().get_mut(other).unwrap().bound.insert("fontFamily".into(), "V:cycle".into());
    // The bound layer takes the value its alias resolves to (bindings are applied after the next
    // command); the cycle resolves to nothing.
    let NodeKind::Text(bound) = &s.doc().unwrap().node(t).unwrap().kind else { panic!() };
    assert_eq!(bound.style.font_family, "FigCraft Missing F");
    assert_eq!(families(&mut s), vec![("FigCraft Missing F".to_string(), 1), ("FigCraft Missing G".to_string(), 1)]);
}

#[test]
fn needs_an_open_document() {
    let mut s = Session::empty();
    assert!(s.execute("text.missingFonts", &json!({})).is_err());
}

/// Make `id`'s box stale, as if it was measured in another font: `dw` wider than its content.
fn stale(s: &mut Session, id: figcraft_doc::NodeId, dw: f64) -> f64 {
    let doc = s.doc_mut().unwrap();
    let n = doc.get_mut(id).unwrap();
    let want = figcraft_text::auto_size(n).unwrap().0;
    n.width = want + dw;
    doc.take_dirty();
    want
}

/// The width of layer `id`.
fn width(s: &Session, id: figcraft_doc::NodeId) -> f64 {
    s.doc().unwrap().node(id).unwrap().width
}

/// The ids a refit reports.
fn refitted(r: &Value) -> Vec<u64> {
    r["refitted"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect()
}

/// Stale boxes fit their text again, by family, by layer or everywhere; fixed boxes keep theirs.
#[test]
fn refit_measures_stale_text_boxes_again() {
    let mut s = Session::new();
    let a = text(&mut s, "FigCraft Refit A");
    let b = text(&mut s, "FigCraft Refit B");
    let fixed = text(&mut s, "FigCraft Refit A");
    if let NodeKind::Text(t) = &mut s.doc_mut().unwrap().get_mut(fixed).unwrap().kind {
        t.auto_resize = figcraft_doc::TextAutoResize::None;
    }
    s.execute("select.none", &json!({})).unwrap();
    let want_a = stale(&mut s, a, -30.0);
    let want_b = stale(&mut s, b, 25.0);
    s.doc_mut().unwrap().get_mut(fixed).unwrap().width = 7.0;
    s.doc_mut().unwrap().take_dirty();
    // Only the families asked for.
    let only_a = super::refit(s.doc_mut().unwrap(), None, Some(&["figcraft refit a".to_string()]));
    assert_eq!(only_a, vec![a]);
    assert_eq!(width(&s, a), want_a);
    assert_eq!(width(&s, b), want_b + 25.0);
    // Only the layers asked for, then everything; a fixed box keeps its size.
    stale(&mut s, a, 12.0);
    assert!(refitted(&s.execute("text.refit", &json!({"ids": [b]})).unwrap()) == vec![b.0]);
    assert_eq!((width(&s, a), width(&s, b)), (want_a + 12.0, want_b));
    assert!(refitted(&s.execute("text.refit", &json!({})).unwrap()) == vec![a.0]);
    assert_eq!(width(&s, a), want_a);
    assert_eq!(width(&s, fixed), 7.0);
    // Nothing stale: nothing changes.
    assert!(refitted(&s.execute("text.refit", &json!({})).unwrap()).is_empty());
    assert!(s.execute("text.refit", &json!({"ids": ["nope"]})).is_err());
    // An `ids` that isn't a list is refused, not taken as the whole document.
    stale(&mut s, a, 9.0);
    assert!(s.execute("text.refit", &json!({"ids": a.0.to_string()})).is_err());
    assert_eq!(width(&s, a), want_a + 9.0);
    assert!(Session::empty().execute("text.refit", &json!({})).is_err());
}

/// Text inside an instance is refitted too.
#[test]
fn refit_reaches_text_in_instances() {
    let mut s = Session::new();
    let t = text(&mut s, "FigCraft Refit C");
    let comp = id_of(&s.execute("component.create", &json!({"ids": [t]})).unwrap()["id"]).unwrap();
    let inst = id_of(&s.execute("instance.create", &json!({"component": comp, "x": 300, "y": 0})).unwrap()["id"]).unwrap();
    let copy = s.doc().unwrap().node(inst).unwrap().children[0];
    let want = stale(&mut s, copy, -20.0);
    let r = s.execute("text.refit", &json!({"ids": [inst]})).unwrap();
    assert_eq!(refitted(&r), vec![copy.0]);
    assert_eq!(width(&s, copy), want);
}

/// `text.loadFont` refuses what isn't a font file; `text.rescanFonts` reports what it added.
#[test]
fn fonts_load_from_files_and_rescans_report_new_families() {
    let mut s = Session::new();
    assert!(s.execute("text.loadFont", &json!({})).is_err());
    assert!(s.execute("text.loadFont", &json!({"data": "bm90IGEgZm9udA=="})).is_err(), "not a font file");
    assert!(s.execute("text.loadFont", &json!({"data": "%%%"})).is_err());
    assert!(s.execute("text.loadFont", &json!({"path": "no-such-dir/x.ttf"})).is_err());
    if let Some(f) = figcraft_text::CRAFT_FONTS.first() {
        use base64::Engine as _;
        let data = base64::engine::general_purpose::STANDARD.encode(f.bytes);
        let r = s.execute("text.loadFont", &json!({"data": data})).unwrap();
        assert_eq!(r["families"].as_array().map(Vec::len), Some(1));
    }
    let r = s.execute("text.rescanFonts", &json!({})).unwrap();
    assert!(r["added"].is_array());
}

/// `text.loadFont {path}` remembers the file, which comes back at start-up (`restore_fonts`);
/// `text.fontSources` lists and forgets them.
#[test]
fn loaded_font_files_come_back_at_start_up() {
    let mut s = Session::new();
    assert!(s.execute("text.loadFont", &json!({"path": "no-such-dir/x.ttf"})).is_err());
    assert!(s.prefs.font_sources.is_empty(), "a failed load isn't remembered");
    let Some(f) = figcraft_text::CRAFT_FONTS.first() else { return };
    let dir = std::env::temp_dir().join(format!("figcraft-font-sources-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("Game.ttf");
    std::fs::write(&file, f.bytes).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = s.execute("text.loadFont", &json!({"path": path})).unwrap();
    let families = r["families"].clone();
    s.execute("text.loadFont", &json!({"path": path})).unwrap();
    assert_eq!(s.prefs.font_sources.len(), 1, "remembered once");
    let mut next = Session::new();
    next.prefs = s.prefs.clone();
    assert_eq!(json!(crate::cmd::text::restore_fonts(&next)), families);
    // Not remembered when asked; listed, removed and cleared through text.fontSources.
    let other = dir.join("Other.ttf");
    std::fs::write(&other, f.bytes).unwrap();
    s.execute("text.loadFont", &json!({"path": other.to_string_lossy(), "remember": false})).unwrap();
    assert_eq!(s.execute("text.fontSources", &json!({})).unwrap()["sources"].as_array().map(Vec::len), Some(1));
    assert_eq!(s.execute("text.fontSources", &json!({"remove": path})).unwrap()["sources"], json!([]));
    s.execute("text.loadFont", &json!({"path": path})).unwrap();
    assert_eq!(s.execute("text.fontSources", &json!({"clear": true})).unwrap()["sources"], json!([]));
    // A gone file is skipped quietly.
    let _ = std::fs::remove_dir_all(&dir);
    assert!(crate::cmd::text::restore_fonts(&next).is_empty());
}

/// `text.addFontFolder {path}` adds a folder's fonts and remembers it; it comes back at start-up
/// and `text.fontFolders` lists and forgets them. A localized family name stores the font's own.
#[test]
fn font_folders_come_back_at_start_up() {
    let mut s = Session::new();
    assert!(s.execute("text.addFontFolder", &json!({})).is_err());
    assert!(s.execute("text.addFontFolder", &json!({"path": "no-such-dir"})).is_err());
    let dir = std::env::temp_dir().join(format!("figcraft-font-folders-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let empty = dir.to_string_lossy().into_owned();
    assert!(s.execute("text.addFontFolder", &json!({"path": empty})).is_err(), "no fonts");
    assert!(s.prefs.font_folders.is_empty(), "a failed add isn't remembered");
    std::fs::write(dir.join("Studio.ttf"), figcraft_text::fontdb::named_font_bytes(&[(0x0409, "Studio Folder 5e1"), (0x0412, "스튜디오 폴더")]))
        .unwrap();
    let r = s.execute("text.addFontFolder", &json!({"path": empty})).unwrap();
    assert_eq!(r["families"], json!(["Studio Folder 5e1"]));
    s.execute("text.addFontFolder", &json!({"path": empty})).unwrap();
    assert_eq!(s.prefs.font_folders.len(), 1, "remembered once");
    let mut next = Session::new();
    next.prefs = s.prefs.clone();
    assert!(crate::cmd::text::restore_fonts(&next).contains(&"Studio Folder 5e1".to_string()));
    // The localized name stores the family's own name.
    let t = text(&mut s, "Inter");
    s.execute("text.style", &json!({"ids": [t], "fontFamily": "스튜디오 폴더"})).unwrap();
    let NodeKind::Text(td) = &s.doc().unwrap().get(t).unwrap().kind else { panic!("not text") };
    assert_eq!(td.style.font_family, "Studio Folder 5e1");
    assert_eq!(s.execute("text.fontFolders", &json!({"remove": empty})).unwrap()["folders"], json!([]));
    s.execute("text.addFontFolder", &json!({"path": empty})).unwrap();
    assert_eq!(s.execute("text.fontFolders", &json!({"clear": true})).unwrap()["folders"], json!([]));
    let _ = std::fs::remove_dir_all(&dir);
}

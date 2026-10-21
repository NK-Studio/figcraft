use figcraft_doc::color::{Color, Gradient, GradientKind, Paint};
use figcraft_doc::{
    AutoLayout, Constraints, DeviceFrame, Document, Effect, HConstraint, LayoutMode, NodeId, NodeKind, Pivot, SideInsets, Sizing, TextData,
    VConstraint,
};

use crate::*;

fn opts(minor: u8) -> CheckOptions {
    CheckOptions { version: UnityVersion::new(minor), grid_enabled: false }
}

fn screen() -> (Document, NodeId) {
    let mut d = Document::new("t");
    let p = d.first_page().unwrap();
    let f = d.create(p, NodeKind::Frame, "Main Screen", 0.0, 0.0, 400.0, 800.0).unwrap();
    (d, f)
}

/// The USS rule of layer `id` in the first document.
fn rule(e: &Export, id: NodeId) -> String {
    rule_of(&e.documents[0].uss, &format!("__{} {{", id.0))
}

fn rule_of(uss: &str, head: &str) -> String {
    let start = uss.find(head).unwrap_or_else(|| panic!("no {head} in\n{uss}"));
    let rest = &uss[start..];
    rest[..rest.find('}').unwrap() + 1].to_string()
}

fn has(rule: &str, decl: &str) -> bool {
    rule.lines().any(|l| l.trim() == decl)
}

#[test]
fn root_fills_the_panel_and_files_are_named_after_the_frame() {
    let (d, f) = screen();
    let e = export(&d, &[f], opts(0));
    let doc = &e.documents[0];
    assert_eq!((doc.stem.as_str(), doc.width, doc.height, doc.safe_area), ("Main_Screen", 400.0, 800.0, false));
    assert!(doc.uxml.starts_with("<ui:UXML xmlns:ui=\"UnityEngine.UIElements\""));
    assert!(doc.uxml.contains("<Style src=\"Main_Screen.uss\" />"));
    // Classes are BEM by document: the block from the frame's name, then the layer id.
    assert!(doc.uxml.contains(&format!("<ui:VisualElement name=\"Main Screen\" class=\"main-screen__{}\" />", f.0)), "{}", doc.uxml);
    assert!(!doc.uxml.contains("fc-") && !doc.uss.contains("fc-"), "no product prefix in generated names");
    let r = rule(&e, f);
    for decl in ["position: absolute;", "left: 0;", "top: 0;", "right: 0;", "bottom: 0;", "background-color: rgba(255, 255, 255, 1);"] {
        assert!(has(&r, decl), "{decl} in {r}");
    }
}

#[test]
fn constraints_and_pivots_become_offsets_anchors_and_translate() {
    let (mut d, f) = screen();
    let mk = |d: &mut Document, h, v| {
        let r = d.create(f, NodeKind::Rectangle, "R", 20.0, 30.0, 40.0, 50.0).unwrap();
        d.get_mut(r).unwrap().constraints = Constraints { horizontal: h, vertical: v };
        r
    };
    let tl = mk(&mut d, HConstraint::Left, VConstraint::Top);
    let br = mk(&mut d, HConstraint::Right, VConstraint::Bottom);
    let cc = mk(&mut d, HConstraint::Center, VConstraint::Center);
    let st = mk(&mut d, HConstraint::LeftRight, VConstraint::TopBottom);
    let sc = mk(&mut d, HConstraint::Scale, VConstraint::Scale);
    let pv = mk(&mut d, HConstraint::Left, VConstraint::Top);
    d.get_mut(pv).unwrap().pivot = Some(Pivot::new(0.5, 1.0));
    let e = export(&d, &[f], opts(0));
    let r = rule(&e, tl);
    for decl in ["position: absolute;", "left: 20px;", "top: 30px;", "width: 40px;", "height: 50px;"] {
        assert!(has(&r, decl), "{decl} in {r}");
    }
    let r = rule(&e, br);
    assert!(has(&r, "right: 340px;") && has(&r, "bottom: 720px;") && !r.contains("left:"), "{r}");
    // Centre: anchored at 50%, the implied pivot is the centre, the offset is a margin.
    let r = rule(&e, cc);
    for decl in ["left: 50%;", "margin-left: -160px;", "top: 50%;", "margin-top: -345px;", "translate: -50% -50%;"] {
        assert!(has(&r, decl), "{decl} in {r}");
    }
    let r = rule(&e, st);
    assert!(has(&r, "left: 20px;") && has(&r, "right: 340px;") && !r.contains("width:"), "{r}");
    let r = rule(&e, sc);
    assert!(has(&r, "left: 5%;") && has(&r, "width: 10%;") && has(&r, "top: 3.75%;") && has(&r, "height: 6.25%;"), "{r}");
    // A set pivot with left/top pins: the same place, through the pivot.
    let r = rule(&e, pv);
    for decl in ["left: 0%;", "margin-left: 40px;", "top: 0%;", "margin-top: 80px;", "translate: -50% -100%;"] {
        assert!(has(&r, decl), "{decl} in {r}");
    }
}

#[test]
fn auto_layout_becomes_flexbox_and_gap_follows_the_version() {
    let (mut d, f) = screen();
    let row = d.create(f, NodeKind::Frame, "Row", 0.0, 0.0, 300.0, 60.0).unwrap();
    d.get_mut(row).unwrap().layout = Some(AutoLayout { mode: LayoutMode::Horizontal, gap: 8.0, padding: [4.0, 6.0, 4.0, 6.0], ..Default::default() });
    let a = d.create(row, NodeKind::Rectangle, "A", 0.0, 0.0, 40.0, 40.0).unwrap();
    let b = d.create(row, NodeKind::Rectangle, "B", 0.0, 0.0, 40.0, 40.0).unwrap();
    d.get_mut(b).unwrap().child_layout.sizing_h = Sizing::Fill;
    let e = export(&d, &[f], opts(0));
    let r = rule(&e, row);
    for decl in ["flex-direction: row;", "justify-content: flex-start;", "align-items: flex-start;", "padding-top: 4px;", "padding-left: 6px;"] {
        assert!(has(&r, decl), "{decl} in {r}");
    }
    assert!(!r.contains("gap"), "{r}");
    let (ra, rb) = (rule(&e, a), rule(&e, b));
    assert!(!ra.contains("margin-left") && has(&ra, "flex-shrink: 0;") && has(&ra, "width: 40px;") && !ra.contains("position"), "{ra}");
    assert!(has(&rb, "margin-left: 8px;") && has(&rb, "flex-grow: 1;") && has(&rb, "flex-basis: 0;") && !rb.contains("width:"), "{rb}");
    let e = export(&d, &[f], opts(7));
    assert!(has(&rule(&e, row), "gap: 8px;"));
    assert!(!rule(&e, b).contains("margin-left"));
}

#[test]
fn first_on_top_reverses_before_6_7_and_uses_z_index_after() {
    let (mut d, f) = screen();
    let row = d.create(f, NodeKind::Frame, "Row", 0.0, 0.0, 300.0, 60.0).unwrap();
    d.get_mut(row).unwrap().layout = Some(AutoLayout { mode: LayoutMode::Horizontal, reverse_z: true, ..Default::default() });
    let a = d.create(row, NodeKind::Rectangle, "A", 0.0, 0.0, 40.0, 40.0).unwrap();
    let b = d.create(row, NodeKind::Rectangle, "B", 0.0, 0.0, 40.0, 40.0).unwrap();
    let e = export(&d, &[f], opts(0));
    let x = &e.documents[0].uxml;
    assert!(x.find("name=\"B\"").unwrap() < x.find("name=\"A\"").unwrap(), "{x}");
    assert!(has(&rule(&e, row), "flex-direction: row-reverse;") && has(&rule(&e, row), "justify-content: flex-end;"));
    let e = export(&d, &[f], opts(7));
    assert!(has(&rule(&e, a), "z-index: 2;") && has(&rule(&e, b), "z-index: 1;"));
}

#[test]
fn text_is_a_label() {
    let (mut d, f) = screen();
    let mut t = TextData::new("Hello <world> & \"you\"\nnext");
    t.style.font_size = 18.0;
    t.style.font_style = "Bold Italic".into();
    let id = d.create(f, NodeKind::Text(t), "Title", 10.0, 10.0, 200.0, 40.0).unwrap();
    let e = export(&d, &[f], opts(0));
    assert!(e.documents[0].uxml.contains("<ui:Label name=\"Title\" class=\"main-screen__"), "{}", e.documents[0].uxml);
    assert!(e.documents[0].uxml.contains("text=\"Hello &lt;world&gt; &amp; &quot;you&quot;&#10;next\""));
    let r = rule(&e, id);
    for decl in
        ["font-size: 18px;", "-unity-font-style: bold-and-italic;", "white-space: nowrap;", "-unity-text-align: upper-left;", "padding-left: 0;"]
    {
        assert!(has(&r, decl), "{decl} in {r}");
    }
    // Auto-width text sizes itself.
    assert!(!r.contains("width:") && !r.contains("height:"), "{r}");
}

#[test]
fn device_frames_get_a_safe_area_container() {
    let (mut d, f) = screen();
    d.get_mut(f).unwrap().device = Some(DeviceFrame {
        width: 400.0,
        height: 800.0,
        safe_area: SideInsets { top: 50.0, bottom: 30.0, ..Default::default() },
        ..Default::default()
    });
    let bg = d.create(f, NodeKind::Rectangle, "Background", 0.0, 0.0, 400.0, 800.0).unwrap();
    let title = d.create(f, NodeKind::Rectangle, "Title", 20.0, 60.0, 100.0, 20.0).unwrap();
    let e = export(&d, &[f], opts(0));
    let doc = &e.documents[0];
    assert!(doc.safe_area);
    let x = &doc.uxml;
    let sa = x.find("<FigCraft.PanelSafeArea name=\"PanelSafeArea\"").unwrap();
    assert!(x.contains("design-inset-top=\"50\" design-inset-right=\"0\" design-inset-bottom=\"30\" design-inset-left=\"0\""), "{x}");
    assert!(x.contains("</FigCraft.PanelSafeArea>"), "{x}");
    assert!(x.find("name=\"Background\"").unwrap() < sa && x.find("name=\"Title\"").unwrap() > sa, "{x}");
    let r = rule_of(&doc.uss, &format!("__{}-safe-area {{", f.0));
    assert!(has(&r, "left: 0px;") && has(&r, "top: 50px;") && has(&r, "bottom: 30px;"), "{r}");
    // Inside the container, coordinates start at the safe area.
    assert!(has(&rule(&e, title), "top: 10px;") && has(&rule(&e, title), "left: 20px;"));
    assert!(has(&rule(&e, bg), "top: 0px;"));
}

#[test]
fn baking_follows_the_version() {
    let (mut d, f) = screen();
    let dot = d.create(f, NodeKind::Ellipse { start: 0.0, sweep: std::f64::consts::TAU, inner: 0.0 }, "Dot", 10.0, 10.0, 20.0, 20.0).unwrap();
    let card = d.create(f, NodeKind::Rectangle, "Card", 50.0, 50.0, 100.0, 60.0).unwrap();
    let stops = vec![
        figcraft_doc::color::ColorStop { position: 0.0, color: Color::BLACK },
        figcraft_doc::color::ColorStop { position: 1.0, color: Color::WHITE },
    ];
    d.get_mut(card).unwrap().fills =
        vec![Paint::gradient(Gradient { kind: GradientKind::Linear, stops, from: (0.5, 0.0), to: (0.5, 1.0), width: (1.0, 0.0) })];
    let e = export(&d, &[f], opts(0));
    let baked: Vec<NodeId> = e.bakes.iter().map(|b| b.id).collect();
    assert_eq!(baked, vec![dot, card]);
    assert!(rule(&e, dot).contains(&format!("url(\"images/baked-{}.png\")", dot.0)));
    let e = export(&d, &[f], opts(7));
    assert_eq!(e.bakes.iter().map(|b| b.id).collect::<Vec<_>>(), vec![dot]);
    let r = rule(&e, card);
    assert!(has(&r, "background-image: linear-gradient(180deg, rgba(0, 0, 0, 1) 0%, rgba(255, 255, 255, 1) 100%);"), "{r}");
}

#[test]
fn shadows_are_filters_from_6_6_and_baked_with_an_overlay_before() {
    let (mut d, f) = screen();
    let card = d.create(f, NodeKind::Rectangle, "Card", 50.0, 50.0, 100.0, 60.0).unwrap();
    d.get_mut(card).unwrap().effects = vec![Effect::drop_shadow()];
    let e = export(&d, &[f], opts(6));
    assert!(rule(&e, card).contains("filter: drop-shadow("), "{}", rule(&e, card));
    assert!(e.bakes.is_empty());
    let e = export(&d, &[f], opts(0));
    assert_eq!(e.bakes.len(), 1);
    // The PNG reaches past the box: an overlay child draws it.
    let over = rule_of(&e.documents[0].uss, &format!("__{}-baked {{", card.0));
    assert!(over.contains("background-image") && over.contains("left: -"), "{over}");
    assert!(e.documents[0].uxml.contains("name=\"Card (baked)\""));
}

#[test]
fn stems_are_unique_and_non_frames_skipped() {
    let (mut d, f) = screen();
    let p = d.first_page().unwrap();
    let g = d.create(p, NodeKind::Frame, "Main Screen", 500.0, 0.0, 10.0, 10.0).unwrap();
    let h = d.create(p, NodeKind::Frame, "!!!", 600.0, 0.0, 10.0, 10.0).unwrap();
    let r = d.create(p, NodeKind::Rectangle, "R", 0.0, 0.0, 10.0, 10.0).unwrap();
    let e = export(&d, &[f, g, h, r, f, NodeId(999_999)], opts(0));
    let stems: Vec<&str> = e.documents.iter().map(|d| d.stem.as_str()).collect();
    assert_eq!(stems, vec!["Main_Screen", "Main_Screen-2", "Frame"]);
}

#[test]
fn junk_values_never_panic() {
    let (mut d, f) = screen();
    let r = d.create(f, NodeKind::Rectangle, "R", 0.0, 0.0, 10.0, 10.0).unwrap();
    {
        let n = d.get_mut(r).unwrap();
        n.width = f64::NAN;
        n.opacity = f32::NAN;
        n.corners.radii = [f64::INFINITY, -1.0, f64::NAN, 1e308];
        n.pivot = Some(Pivot::new(f64::NAN, 9.0));
        n.fills = vec![Paint::solid(Color { r: f32::NAN, g: 2.0, b: -1.0, a: 1.0 })];
    }
    d.get_mut(f).unwrap().layout = Some(AutoLayout { gap: f64::NAN, padding: [f64::NAN; 4], ..Default::default() });
    let e = export(&d, &[f], opts(7));
    assert!(!e.documents[0].uss.contains("NaN") && !e.documents[0].uss.contains("inf"), "{}", e.documents[0].uss);
}

#[test]
fn fixed_text_boxes_keep_figcrafts_size() {
    let (mut d, f) = screen();
    let free = d.create(f, NodeKind::Text(TextData::new("Free")), "Free", 10.0, 10.0, 46.0, 24.0).unwrap();
    let row = d.create(f, NodeKind::Frame, "Row", 0.0, 100.0, 300.0, 40.0).unwrap();
    d.get_mut(row).unwrap().layout = Some(AutoLayout { mode: LayoutMode::Horizontal, ..Default::default() });
    let hug = d.create(row, NodeKind::Text(TextData::new("Hug")), "Hug", 0.0, 0.0, 30.0, 24.0).unwrap();
    {
        let n = d.get_mut(hug).unwrap();
        n.child_layout.sizing_h = Sizing::Hug;
        n.child_layout.sizing_v = Sizing::Hug;
    }
    let fill = d.create(row, NodeKind::Text(TextData::new("Fill")), "Fill", 0.0, 0.0, 30.0, 24.0).unwrap();
    d.get_mut(fill).unwrap().child_layout.sizing_h = Sizing::Fill;
    // Auto: the labels size themselves.
    let e = export(&d, &[f], opts(6));
    assert!(!rule(&e, free).contains("width:") && !rule(&e, hug).contains("width:"));
    // Fixed: FigCraft's box, except along a fill axis.
    let e = export(&d, &[f], ExportOptions { check: opts(6), fixed_text_boxes: true });
    for id in [free, hug] {
        let r = rule(&e, id);
        assert!(r.contains("width: ") && has(&r, "height: 24px;"), "{r}");
    }
    let r = rule(&e, fill);
    assert!(!r.contains("width:") && has(&r, "flex-grow: 1;") && has(&r, "height: 24px;"), "{r}");
}

#[test]
fn block_names_are_kebab_case_and_start_with_a_letter() {
    let (mut d, _) = screen();
    let p = d.first_page().unwrap();
    let mut out = Vec::new();
    for name in ["BunnyMagicCardShow", "12 Cards", "!!!", "iPhone_12 mini"] {
        let f = d.create(p, NodeKind::Frame, name, 0.0, 0.0, 10.0, 10.0).unwrap();
        let e = export(&d, &[f], opts(0));
        let first = e.documents[0].uss.lines().find(|l| l.starts_with('.')).unwrap().to_string();
        out.push(first.trim_start_matches('.').split("__").next().unwrap().to_string());
    }
    assert_eq!(out, vec!["bunny-magic-card-show", "screen-12-cards", "frame", "i-phone-12-mini"]);
}

#[test]
fn lengths_follow_the_frames_ui_scaling() {
    let (mut d, f) = screen();
    let card = d.create(f, NodeKind::Rectangle, "Card", 10.0, 20.0, 100.0, 60.0).unwrap();
    // No setting: the short side, 1 px = 1 pt.
    let e = export(&d, &[f], opts(0));
    let ps = &e.documents[0].panel_settings;
    assert_eq!((ps.scale_mode, ps.screen_match_mode, ps.source), ("ScaleWithScreenSize", "Expand", "shortSide"));
    assert_eq!(ps.reference_resolution.x, ps.reference_resolution.y);
    assert_eq!(ps.units_per_point, 1.0);
    assert!(has(&rule(&e, card), "width: 100px;"));
    assert!(!e.issues.iter().any(|i| i.rule == "ui-scaling"));
    // 1080 × 1920 with Expand on a 375-wide screen: 1080/375 units per point.
    let (w, h) = (d.node(f).unwrap().width, d.node(f).unwrap().height);
    d.get_mut(f).unwrap().canvas_scaler =
        Some(figcraft_doc::CanvasScaler { screen_match_mode: figcraft_doc::ScreenMatchMode::Expand, ..Default::default() });
    let k = figcraft_doc::CanvasScaler { screen_match_mode: figcraft_doc::ScreenMatchMode::Expand, ..Default::default() }
        .points_per_unit(w, h, figcraft_doc::ScreenDensity::default())
        .map(|u| 1.0 / u)
        .unwrap();
    let e = export(&d, &[f], opts(0));
    let ps = &e.documents[0].panel_settings;
    assert_eq!((ps.reference_resolution.x, ps.reference_resolution.y, ps.source), (1080, 1920, "frame"));
    assert!((ps.units_per_point - k).abs() < 1e-9);
    let r = rule(&e, card);
    let want = |v: f64| format!("{}px", (v * k * 1000.0).round() / 1000.0);
    assert!(has(&r, &format!("width: {};", want(100.0))) && has(&r, &format!("height: {};", want(60.0))), "{r}");
    assert!(!e.issues.iter().any(|i| i.rule == "ui-scaling"));
    // Constant Pixel Size without a device: no density to work units out from.
    d.get_mut(f).unwrap().canvas_scaler =
        Some(figcraft_doc::CanvasScaler { scale_mode: figcraft_doc::UiScaleMode::ConstantPixelSize, ..Default::default() });
    let e = export(&d, &[f], opts(0));
    assert_eq!(e.documents[0].panel_settings.scale_mode, "ConstantPixelSize");
    assert!(has(&rule(&e, card), "width: 100px;"));
    assert!(e.issues.iter().any(|i| i.rule == "ui-scaling" && i.severity == crate::Severity::Warning));
}

#[test]
fn scale_lengths_skips_quotes_and_words() {
    assert_eq!(super::export::scale_lengths_for_tests("10px -2.5px 0px", 2.0), "20px -5px 0px");
    assert_eq!(
        super::export::scale_lengths_for_tests("drop-shadow(1px 2px 3px rgba(0, 0, 0, 0.5))", 2.0),
        "drop-shadow(2px 4px 6px rgba(0, 0, 0, 0.5))"
    );
    assert_eq!(super::export::scale_lengths_for_tests("\"Font 12px\" -50% h1px", 2.0), "\"Font 12px\" -50% h1px");
    assert_eq!(super::export::scale_lengths_for_tests("url(\"images/1px.png\") 4pxa", 2.0), "url(\"images/1px.png\") 4pxa");
}

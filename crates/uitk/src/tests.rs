use figcraft_doc::color::{BlendMode, Color, Gradient, GradientKind, Paint};
use figcraft_doc::{
    AutoLayout, CrossAlign, Document, Effect, EffectKind, LayoutMode, LineHeight, MainAlign, NodeId, NodeKind, StrokeAlign, TextData,
};

use crate::*;

fn v(minor: u8) -> CheckOptions {
    CheckOptions { version: UnityVersion::new(minor), grid_enabled: false }
}

fn doc() -> (Document, NodeId) {
    let d = Document::new("t");
    let p = d.first_page().unwrap();
    (d, p)
}

fn rules(issues: &[Issue]) -> Vec<(&'static str, Severity)> {
    issues.iter().map(|i| (i.rule, i.severity)).collect()
}

fn gradient(kind: GradientKind) -> Paint {
    Paint::gradient(Gradient { kind, stops: vec![], from: (0.0, 0.0), to: (1.0, 1.0), width: (1.0, 0.0) })
}

#[test]
fn versions_parse_and_gate_features() {
    for (s, minor) in [("6", 0), ("6.0", 0), ("6.3", 3), ("6000.7", 7), ("6000.3.0f1", 3), ("Unity 6.6", 6), (" 6.12 ", 12)] {
        assert_eq!(UnityVersion::parse(s), Some(UnityVersion::new(minor)), "{s}");
    }
    for s in ["", "5.6", "2022.3", "7.0", "6.x", "6.100", "6.-1", "abc", "6..3"] {
        assert_eq!(UnityVersion::parse(s), None, "{s}");
    }
    assert_eq!(UnityVersion::new(3).to_string(), "6.3");
    assert!(!UnityVersion::new(6).has(Feature::Gap));
    assert!(UnityVersion::new(7).has(Feature::Gap));
    assert!(UnityVersion::new(3).has(Feature::Filter) && !UnityVersion::new(2).has(Feature::Filter));
    assert!(UnityVersion::new(6).has(Feature::DropShadow) && !UnityVersion::new(5).has(Feature::DropShadow));
}

#[test]
fn a_plain_layout_has_nothing_to_report() {
    let (mut d, p) = doc();
    let f = d.create(p, NodeKind::Frame, "Screen", 0.0, 0.0, 400.0, 800.0).unwrap();
    d.get_mut(f).unwrap().layout = Some(AutoLayout { mode: LayoutMode::Vertical, gap: 0.0, ..Default::default() });
    d.create(f, NodeKind::Rectangle, "Box", 0.0, 0.0, 100.0, 40.0).unwrap();
    d.create(f, NodeKind::Text(TextData::new("Hello")), "Label", 0.0, 0.0, 100.0, 20.0).unwrap();
    assert_eq!(check(&d, &[p], v(0)), vec![]);
}

#[test]
fn gap_is_converted_before_6_7() {
    let (mut d, p) = doc();
    let f = d.create(p, NodeKind::Frame, "Row", 0.0, 0.0, 400.0, 100.0).unwrap();
    d.get_mut(f).unwrap().layout = Some(AutoLayout { mode: LayoutMode::Horizontal, gap: 8.0, ..Default::default() });
    let r = check(&d, &[f], v(6));
    assert_eq!(rules(&r), vec![("gap", Severity::Info)]);
    assert_eq!(r[0].since, Some(UnityVersion::new(7)));
    assert_eq!(check(&d, &[f], v(7)), vec![]);
    // Wrapping needs a negative container margin: approximated.
    d.get_mut(f).unwrap().layout = Some(AutoLayout { mode: LayoutMode::Horizontal, gap: 8.0, wrap: true, cross_gap: 4.0, ..Default::default() });
    assert_eq!(rules(&check(&d, &[f], v(0))), vec![("gap", Severity::Warning)]);
    // Auto gaps are justify-content, not a gap.
    d.get_mut(f).unwrap().layout =
        Some(AutoLayout { mode: LayoutMode::Horizontal, gap: 8.0, main_align: MainAlign::SpaceBetween, ..Default::default() });
    assert_eq!(check(&d, &[f], v(0)), vec![]);
}

#[test]
fn grid_needs_6_7_and_the_experimental_setting() {
    let (mut d, p) = doc();
    let f = d.create(p, NodeKind::Frame, "Grid", 0.0, 0.0, 400.0, 400.0).unwrap();
    d.get_mut(f).unwrap().layout = Some(AutoLayout { mode: LayoutMode::Grid, gap: 8.0, ..Default::default() });
    assert_eq!(rules(&check(&d, &[f], v(6))), vec![("grid-layout", Severity::Error)]);
    assert_eq!(rules(&check(&d, &[f], v(7))), vec![("grid-layout", Severity::Error)]);
    let on = CheckOptions { version: UnityVersion::new(7), grid_enabled: true };
    assert_eq!(rules(&check(&d, &[f], on)), vec![("grid-layout", Severity::Info)]);
}

#[test]
fn layout_rules() {
    let (mut d, p) = doc();
    let f = d.create(p, NodeKind::Frame, "F", 0.0, 0.0, 400.0, 400.0).unwrap();
    {
        let n = d.get_mut(f).unwrap();
        n.layout = Some(AutoLayout { cross_align: CrossAlign::Baseline, strokes_included: false, reverse_z: true, ..Default::default() });
        n.strokes = vec![Paint::solid(Color::BLACK)];
        n.stroke.weight = 2.0;
    }
    let r = rules(&check(&d, &[f], v(0)));
    assert!(r.contains(&("baseline-align", Severity::Error)), "{r:?}");
    assert!(r.contains(&("strokes-in-layout", Severity::Warning)), "{r:?}");
    assert!(r.contains(&("z-order", Severity::Info)), "{r:?}");
}

#[test]
fn effects_follow_the_version() {
    let (mut d, p) = doc();
    let f = d.create(p, NodeKind::Rectangle, "Card", 0.0, 0.0, 100.0, 100.0).unwrap();
    d.get_mut(f).unwrap().effects = vec![Effect::drop_shadow(), Effect::layer_blur(), Effect::background_blur()];
    let at = |d: &Document, m| rules(&check(d, &[f], v(m)));
    assert_eq!(at(&d, 0), vec![("drop-shadow", Severity::Warning), ("layer-blur", Severity::Warning), ("background-blur", Severity::Warning)]);
    assert_eq!(at(&d, 3), vec![("drop-shadow", Severity::Warning), ("background-blur", Severity::Warning)]);
    assert_eq!(at(&d, 6), vec![]);
    // A spread shadow can't be a drop-shadow() filter exactly.
    if let EffectKind::DropShadow(s) = &mut d.get_mut(f).unwrap().effects[0].kind {
        s.spread = 4.0;
    }
    assert_eq!(rules(&check(&d, &[f], v(6))), vec![("drop-shadow", Severity::Warning)]);
    d.get_mut(f).unwrap().effects = vec![Effect::inner_shadow(), Effect::noise(), Effect::glass()];
    assert_eq!(at(&d, 7), vec![("inner-shadow", Severity::Error), ("effect-other", Severity::Error), ("effect-other", Severity::Error)]);
    // Hidden effects don't count.
    d.get_mut(f).unwrap().effects.iter_mut().for_each(|e| e.visible = false);
    assert_eq!(at(&d, 0), vec![]);
}

#[test]
fn fills_and_strokes() {
    let (mut d, p) = doc();
    let r = d.create(p, NodeKind::Rectangle, "R", 0.0, 0.0, 100.0, 100.0).unwrap();
    d.get_mut(r).unwrap().fills = vec![gradient(GradientKind::Linear)];
    assert_eq!(rules(&check(&d, &[r], v(6))), vec![("gradient", Severity::Warning)]);
    assert_eq!(check(&d, &[r], v(7)), vec![]);
    d.get_mut(r).unwrap().fills = vec![gradient(GradientKind::Angular), Paint::solid(Color::WHITE)];
    assert_eq!(rules(&check(&d, &[r], v(7))), vec![("fill-count", Severity::Warning), ("gradient", Severity::Warning)]);
    {
        let n = d.get_mut(r).unwrap();
        n.fills = vec![];
        n.strokes = vec![Paint::solid(Color::BLACK)];
        n.stroke.weight = 1.0;
        n.stroke.align = StrokeAlign::Outside;
        n.stroke.dashes = vec![4.0, 2.0];
        n.corners.smoothing = 0.6;
        n.blend = BlendMode::Multiply;
    }
    let got = rules(&check(&d, &[r], v(7)));
    for want in [
        ("stroke-align", Severity::Warning),
        ("stroke-style", Severity::Error),
        ("corner-smoothing", Severity::Warning),
        ("blend", Severity::Warning),
    ] {
        assert!(got.contains(&want), "{want:?} in {got:?}");
    }
    // A stroke with no width isn't a border.
    d.get_mut(r).unwrap().stroke.weight = 0.0;
    assert!(!rules(&check(&d, &[r], v(7))).iter().any(|(rule, _)| rule.starts_with("stroke")));
}

#[test]
fn vectors_are_one_image() {
    let (mut d, p) = doc();
    let e = d.create(p, NodeKind::Ellipse { start: 0.0, sweep: std::f64::consts::TAU, inner: 0.0 }, "Dot", 0.0, 0.0, 10.0, 10.0).unwrap();
    d.get_mut(e).unwrap().stroke.dashes = vec![1.0];
    assert_eq!(rules(&check(&d, &[e], v(0))), vec![("vector", Severity::Warning)]);
    assert_eq!(rules(&check(&d, &[e], v(3))), vec![("vector", Severity::Info)]);
}

#[test]
fn text_rules() {
    let (mut d, p) = doc();
    let mut t = TextData::new("Title");
    t.style.line_height = LineHeight::Pixels(30.0);
    t.paragraph_indent = 12.0;
    let id = d.create(p, NodeKind::Text(t), "Title", 0.0, 0.0, 100.0, 30.0).unwrap();
    d.get_mut(id).unwrap().fills = vec![gradient(GradientKind::Linear)];
    let got = rules(&check(&d, &[id], v(7)));
    assert_eq!(got, vec![("text-line-height", Severity::Warning), ("text-features", Severity::Warning), ("text-fill", Severity::Warning)]);
}

#[test]
fn hidden_layers_masks_and_tree_order() {
    let (mut d, p) = doc();
    let f = d.create(p, NodeKind::Frame, "F", 0.0, 0.0, 400.0, 400.0).unwrap();
    let a = d.create(f, NodeKind::Rectangle, "A", 0.0, 0.0, 10.0, 10.0).unwrap();
    let b = d.create(f, NodeKind::Rectangle, "B", 0.0, 0.0, 10.0, 10.0).unwrap();
    d.get_mut(a).unwrap().is_mask = true;
    d.get_mut(b).unwrap().effects = vec![Effect::inner_shadow()];
    let r = check(&d, &[p], v(0));
    assert_eq!(r.iter().map(|i| (i.id, i.rule)).collect::<Vec<_>>(), vec![(a, "mask"), (b, "inner-shadow")]);
    assert_eq!(r[0].name, "A");
    d.get_mut(f).unwrap().visible = false;
    assert_eq!(check(&d, &[p], v(0)), vec![]);
    // The same layer given twice, and unknown ids, are fine.
    d.get_mut(f).unwrap().visible = true;
    assert_eq!(check(&d, &[b, b, NodeId(999_999)], v(0)).len(), 1);
}

#[test]
fn issues_serialize_for_agents() {
    let (mut d, p) = doc();
    let f = d.create(p, NodeKind::Frame, "Row", 0.0, 0.0, 400.0, 100.0).unwrap();
    d.get_mut(f).unwrap().layout = Some(AutoLayout { gap: 8.0, ..Default::default() });
    let v = serde_json::to_value(check(&d, &[f], v(0))).unwrap();
    assert_eq!(v[0]["severity"], "info");
    assert_eq!(v[0]["rule"], "gap");
    assert_eq!(v[0]["since"], "6.7");
}

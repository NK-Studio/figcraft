//! UXML + USS for top-level frames (`docs/unity-uitk.md` › Export): one document per frame, each
//! layer an element with its own class (`fc-<id>`) and one USS rule. What the target version can't
//! draw is baked: the export lists those layers ([`Bake`]) and the caller renders them to PNG.
//!
//! Units: 1 USS px = 1 FigCraft point (the panel-scale rule). Device frames get a SafeArea
//! container (`figcraft-safe-area`) holding the layers inside their safe area; Unity moves it onto
//! `Screen.safeArea` at run time (the generated `FigcraftSafeArea` script).

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use figcraft_doc::color::{Color, GradientKind, ImageScaleMode, Paint, PaintKind};
use figcraft_doc::geom::{Point, Rect, rotation_degrees, translation};
use figcraft_doc::{
    CrossAlign, Document, EffectKind, HConstraint, LayoutMode, LetterSpacing, LineHeight, MainAlign, Node, NodeId, NodeKind, Pivot, Sizing,
    TextAlignH, TextAlignV, TextAutoResize, VConstraint,
};
use serde::Serialize;

use crate::{CheckOptions, Feature, Issue};

/// Elements written per document at most.
const MAX_ELEMENTS: usize = 50_000;
/// Nesting followed at most.
const MAX_DEPTH: usize = 200;

/// A layer to render to a PNG: `area` (page coordinates) into `file`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Bake {
    pub id: NodeId,
    pub file: String,
    #[serde(skip)]
    pub area: Rect,
}

/// An image fill's bytes to write (`doc.images[hash]`) into `file`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImageFile {
    pub hash: String,
    pub file: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UitkDocument {
    pub frame: NodeId,
    pub name: String,
    /// The file name without its extension (`<stem>.uxml`, `<stem>.uss`).
    pub stem: String,
    pub uxml: String,
    pub uss: String,
    /// The frame's size in points: the panel-scale rule's design width is `width` (portrait).
    pub width: f64,
    pub height: f64,
    /// The frame is a device frame: its document has a SafeArea container.
    pub safe_area: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Export {
    pub documents: Vec<UitkDocument>,
    pub bakes: Vec<Bake>,
    pub images: Vec<ImageFile>,
    pub issues: Vec<Issue>,
}

/// How to export: the target (version, grid) and the text box rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    pub check: CheckOptions,
    /// Labels keep FigCraft's text box size (width and height in px) instead of sizing themselves
    /// with Unity's font metrics.
    pub fixed_text_boxes: bool,
}

impl From<CheckOptions> for ExportOptions {
    fn from(check: CheckOptions) -> Self {
        Self { check, fixed_text_boxes: false }
    }
}

/// Export `frames` (top-level frames; other layers are skipped) for `opts`' Unity version.
pub fn export(doc: &Document, frames: &[NodeId], opts: impl Into<ExportOptions>) -> Export {
    let ExportOptions { check: opts, fixed_text_boxes } = opts.into();
    let mut out = Export::default();
    let mut stems = HashSet::new();
    let mut images: BTreeMap<String, String> = BTreeMap::new();
    let mut seen = HashSet::new();
    for &id in frames {
        let Some(n) = doc.node(id) else { continue };
        if !n.kind.is_frame_like() || !seen.insert(id) {
            continue;
        }
        let stem = unique_stem(&n.name, &mut stems);
        let mut w = Writer { doc, opts, fixed_text_boxes, uxml: String::new(), uss: String::new(), count: 0, bakes: Vec::new(), images: &mut images };
        let safe_area = n.device.is_some();
        w.root(n);
        let uxml = format!(
            "<ui:UXML xmlns:ui=\"UnityEngine.UIElements\" editor-extension-mode=\"False\">\n    <!-- Exported by FigCraft for Unity {}. 1 px = 1 pt; see FigcraftPanelScale and FigcraftSafeArea. -->\n    <Style src=\"{stem}.uss\" />\n{}</ui:UXML>\n",
            opts.version, w.uxml
        );
        let uss = format!("/* Exported by FigCraft for Unity {} from \"{}\". */\n{}", opts.version, n.name.replace("*/", "* /"), w.uss);
        out.bakes.append(&mut w.bakes);
        out.documents.push(UitkDocument { frame: id, name: n.name.clone(), stem, uxml, uss, width: n.width, height: n.height, safe_area });
    }
    out.images = images.into_iter().map(|(hash, file)| ImageFile { hash, file }).collect();
    let roots: Vec<NodeId> = out.documents.iter().map(|d| d.frame).collect();
    out.issues = crate::check(doc, &roots, opts);
    out
}

/// A file-safe stem from a layer name (ASCII letters, digits, `-` and `_`), unique in `taken`.
fn unique_stem(name: &str, taken: &mut HashSet<String>) -> String {
    let mut s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(64).collect();
    s = s.trim_matches('_').to_string();
    if s.is_empty() {
        s = "Frame".into();
    }
    let mut stem = s.clone();
    let mut k = 2;
    while !taken.insert(stem.to_ascii_lowercase()) {
        stem = format!("{s}-{k}");
        k += 1;
    }
    stem
}

/// How an element is placed in its parent.
#[derive(Clone, Copy)]
enum Place {
    /// The document's root: fills the panel.
    Root,
    /// On its own: `left`/`top`… in a containing box `size`, the layer's coordinates shifted by
    /// `offset` (the parent's border, a SafeArea's insets).
    Free { size: (f64, f64), offset: (f64, f64) },
    /// In an auto layout parent's flow (`row`: the main axis is horizontal), with the margin a gap
    /// becomes before UITK has `gap`.
    Flow { row: bool, margin: Option<(&'static str, f64)> },
}

struct Writer<'a> {
    doc: &'a Document,
    opts: CheckOptions,
    fixed_text_boxes: bool,
    uxml: String,
    uss: String,
    count: usize,
    bakes: Vec<Bake>,
    images: &'a mut BTreeMap<String, String>,
}

/// A USS length in px.
fn px(v: f64) -> String {
    format!("{}px", num(v))
}

/// A number with at most 3 decimals, `0` for non-finite values, kept within ±10⁷.
fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v.clamp(-1e7, 1e7) * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

fn pct(v: f64) -> String {
    format!("{}%", num(v * 100.0))
}

fn rgba(c: Color, opacity: f32) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let a = (c.a * opacity).clamp(0.0, 1.0);
    format!("rgba({}, {}, {}, {})", b(c.r), b(c.g), b(c.b), num(f64::from(a)))
}

fn xml(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\n' => o.push_str("&#10;"),
            '\r' => {}
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => o.push(c),
        }
    }
    o
}

fn is_vector(k: &NodeKind) -> bool {
    matches!(
        k,
        NodeKind::Ellipse { .. }
            | NodeKind::Polygon { .. }
            | NodeKind::Star { .. }
            | NodeKind::Line
            | NodeKind::Vector { .. }
            | NodeKind::BooleanOp { .. }
    )
}

/// The layer's unrotated box in its parent: top-left, and the rotation (degrees, clockwise on
/// screen as USS's `rotate` turns).
fn local_box(n: &Node) -> (Point, f64) {
    let deg = rotation_degrees(n.transform);
    if deg.abs() < 1e-6 {
        return (translation(n.transform), 0.0);
    }
    let c = n.transform * Point::new(n.width / 2.0, n.height / 2.0);
    (Point::new(c.x - n.width / 2.0, c.y - n.height / 2.0), -deg)
}

fn border_widths(n: &Node) -> [f64; 4] {
    let visible = n.strokes.iter().any(|p| p.visible);
    if !visible || matches!(n.kind, NodeKind::Text(_)) || is_vector(&n.kind) {
        return [0.0; 4];
    }
    let w = n.stroke.sides.unwrap_or([n.stroke.weight; 4]);
    w.map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 })
}

impl Writer<'_> {
    fn version_has(&self, f: Feature) -> bool {
        self.opts.version.has(f)
    }

    fn root(&mut self, n: &Node) {
        self.element(n, Place::Root, 1);
    }

    /// Write layer `n` and what's in it.
    fn element(&mut self, n: &Node, place: Place, depth: usize) {
        if depth > MAX_DEPTH || self.count >= MAX_ELEMENTS || matches!(n.kind, NodeKind::Slice) {
            return;
        }
        self.count += 1;
        let bake = self.bake_area(n);
        let mut st: Vec<(String, String)> = Vec::new();
        let mut set = |k: &str, v: String| st.push((k.to_string(), v));
        if !n.visible {
            set("display", "none".into());
        }
        let text = matches!(n.kind, NodeKind::Text(_));
        if text {
            set("margin-left", "0".into());
            set("margin-top", "0".into());
            set("margin-right", "0".into());
            set("margin-bottom", "0".into());
            set("padding-left", "0".into());
            set("padding-top", "0".into());
            set("padding-right", "0".into());
            set("padding-bottom", "0".into());
        }
        self.placement(n, place, &mut st);
        if let Some(area) = bake.filter(|_| !text) {
            self.baked_look(n, area, &mut st);
        } else {
            self.look(n, &mut st);
        }
        if let NodeKind::Text(t) = &n.kind {
            self.text_style(n, t, &mut st);
        }
        let border = border_widths(n);
        let device_root = matches!(place, Place::Root) && n.device.is_some();
        if n.layout.is_some() && !device_root {
            self.flex_container(n, border, &mut st);
        }
        // The element.
        let indent = "    ".repeat(depth);
        let class = format!("fc-{}", n.id.0);
        let tag = if text { "ui:Label" } else { "ui:VisualElement" };
        let mut attrs = format!("name=\"{}\" class=\"{class}\"", xml(&n.name));
        if let NodeKind::Text(t) = &n.kind {
            let _ = write!(attrs, " text=\"{}\"", xml(&label_text(t)));
        }
        if n.locked {
            attrs.push_str(" picking-mode=\"Ignore\"");
        }
        self.rule(&format!(".{class}"), &st);
        let kids = self.children_of(n, bake.is_some());
        let baked_child = bake.filter(|_| !text).and_then(|area| self.baked_overlay(n, area));
        if kids.is_empty() && baked_child.is_none() && !device_root {
            let _ = writeln!(self.uxml, "{indent}<{tag} {attrs} />");
            return;
        }
        let _ = writeln!(self.uxml, "{indent}<{tag} {attrs}>");
        if let Some((cls, st)) = baked_child {
            self.rule(&format!(".{cls}"), &st);
            let _ = writeln!(self.uxml, "{indent}    <ui:VisualElement name=\"{} (baked)\" class=\"{cls}\" picking-mode=\"Ignore\" />", xml(&n.name));
        }
        if device_root {
            self.device_children(n, &kids, depth + 1);
        } else {
            self.write_children(n, &kids, border, (0.0, 0.0), (n.width, n.height), depth + 1);
        }
        let _ = writeln!(self.uxml, "{indent}</{tag}>");
    }

    /// The children UITK draws, in UXML order (bottom first; "first on top" before 6.7 reverses
    /// them, with a `-reverse` direction).
    fn children_of(&self, n: &Node, baked: bool) -> Vec<NodeId> {
        if is_vector(&n.kind) || (baked && n.children.is_empty()) || matches!(n.kind, NodeKind::Text(_)) {
            return Vec::new();
        }
        let mut kids = n.children.clone();
        if n.layout.as_ref().is_some_and(|al| al.reverse_z) && !self.version_has(Feature::ZIndex) {
            kids.reverse();
        }
        kids
    }

    /// Write `kids` of `n` into a box `size` whose layer coordinates are shifted by `shift` (the
    /// parent's border, or a SafeArea's insets).
    fn write_children(&mut self, n: &Node, kids: &[NodeId], border: [f64; 4], shift: (f64, f64), size: (f64, f64), depth: usize) {
        let al = n.layout.as_ref();
        let row = al.is_some_and(|al| !matches!(al.mode, LayoutMode::Vertical));
        let mut first_flow = true;
        let total = kids.len();
        for (i, k) in kids.iter().enumerate() {
            let Some(c) = self.doc.node(*k) else { continue };
            let in_flow = al.is_some() && !c.child_layout.absolute;
            let place = if in_flow {
                let margin = al.and_then(|al| {
                    let auto = matches!(al.main_align, MainAlign::SpaceBetween | MainAlign::SpaceAround | MainAlign::SpaceEvenly);
                    if self.version_has(Feature::Gap) || auto || al.gap == 0.0 || first_flow || !c.visible {
                        return None;
                    }
                    Some((if row { "margin-left" } else { "margin-top" }, al.gap))
                });
                if c.visible {
                    first_flow = false;
                }
                Place::Flow { row, margin }
            } else {
                let inner = (size.0 - border[3] - border[1], size.1 - border[0] - border[2]);
                Place::Free { size: inner, offset: (border[3] + shift.0, border[0] + shift.1) }
            };
            let before = self.uss.len();
            self.element(c, place, depth);
            // 6.7+: "first on top" is a z-index (the first child highest).
            if al.is_some_and(|al| al.reverse_z) && self.version_has(Feature::ZIndex) {
                let class = format!(".fc-{} {{", c.id.0);
                if let Some(at) = self.uss.get(before..).and_then(|s| s.find(&class)).map(|p| p + before + class.len()) {
                    self.uss.insert_str(at, &format!("\n    z-index: {};", total - i));
                }
            }
        }
    }

    /// A device frame's children: the ones inside the safe area go into the SafeArea container
    /// (with an auto layout frame: every child in the flow), the rest stay on the root.
    fn device_children(&mut self, n: &Node, kids: &[NodeId], depth: usize) {
        let Some(d) = &n.device else { return };
        let i = d.insets_for(n.width, n.height);
        let inside = |c: &Node| {
            let (p, _) = local_box(c);
            d.holds(n.width, n.height, p.x, p.y, c.width, c.height)
        };
        let flow = n.layout.is_some();
        let (mut outside, mut within) = (Vec::new(), Vec::new());
        for k in kids {
            let Some(c) = self.doc.node(*k) else { continue };
            if (flow && !c.child_layout.absolute) || inside(c) {
                within.push(*k);
            } else {
                outside.push(*k);
            }
        }
        let border = border_widths(n);
        self.write_children(n, &outside, border, (0.0, 0.0), (n.width, n.height), depth);
        // The SafeArea container: placed on the frame's safe area here; Unity moves it onto
        // Screen.safeArea (FigcraftSafeArea).
        let class = format!("fc-{}-safe-area", n.id.0);
        let mut st = vec![
            ("position".to_string(), "absolute".to_string()),
            ("left".into(), px(i.left - border[3])),
            ("top".into(), px(i.top - border[0])),
            ("right".into(), px(i.right - border[1])),
            ("bottom".into(), px(i.bottom - border[2])),
        ];
        if flow {
            // The frame's padding already keeps clear of the unsafe bands: what's left of it.
            let mut shrunk = n.clone();
            if let Some(al) = &mut shrunk.layout {
                let [t, r, b, l] = al.padding;
                al.padding = [(t - i.top).max(0.0), (r - i.right).max(0.0), (b - i.bottom).max(0.0), (l - i.left).max(0.0)];
            }
            self.flex_container(&shrunk, [0.0; 4], &mut st);
        }
        self.rule(&format!(".{class}"), &st);
        let indent = "    ".repeat(depth);
        if within.is_empty() {
            let _ =
                writeln!(self.uxml, "{indent}<ui:VisualElement name=\"SafeArea\" class=\"figcraft-safe-area {class}\" picking-mode=\"Ignore\" />");
            return;
        }
        let _ = writeln!(self.uxml, "{indent}<ui:VisualElement name=\"SafeArea\" class=\"figcraft-safe-area {class}\" picking-mode=\"Ignore\">");
        let inner = (n.width - i.left - i.right, n.height - i.top - i.bottom);
        // Children of the container: coordinates from its top-left (the safe area's).
        let mut holder = n.clone();
        if let Some(al) = &mut holder.layout {
            let [t, r, b, l] = al.padding;
            al.padding = [(t - i.top).max(0.0), (r - i.right).max(0.0), (b - i.bottom).max(0.0), (l - i.left).max(0.0)];
        }
        self.write_children(&holder, &within, [0.0; 4], (i.left, i.top), inner, depth + 1);
        let _ = writeln!(self.uxml, "{indent}</ui:VisualElement>");
    }

    fn placement(&self, n: &Node, place: Place, st: &mut Vec<(String, String)>) {
        let mut set = |k: &str, v: String| st.push((k.to_string(), v));
        let (pos, rot) = local_box(n);
        match place {
            Place::Root => {
                set("position", "absolute".into());
                for k in ["left", "top", "right", "bottom"] {
                    set(k, "0".into());
                }
                return;
            }
            Place::Free { size: (cw, ch), offset: (ox, oy) } => {
                set("position", "absolute".into());
                let (x, y) = (pos.x - ox, pos.y - oy);
                let pivot = n.pivot.map(Pivot::clamped).unwrap_or_else(|| crate::implied_pivot(&n.constraints));
                let (h_size, h_axis) = axis(n.constraints.horizontal.into(), x, n.width, cw, pivot.x, "left", "right", "margin-left", "width");
                let (v_size, v_axis) = axis(n.constraints.vertical.into(), y, n.height, ch, pivot.y, "top", "bottom", "margin-top", "height");
                for (k, v) in h_axis.into_iter().chain(v_axis) {
                    set(k, v);
                }
                let tx = if h_size.translate { pivot.x } else { 0.0 };
                let ty = if v_size.translate { pivot.y } else { 0.0 };
                if tx != 0.0 || ty != 0.0 {
                    set("translate", format!("{} {}", pct(-tx), pct(-ty)));
                }
                self.size(n, h_size.own, v_size.own, st);
            }
            Place::Flow { row, margin } => {
                let cl = &n.child_layout;
                let (main, cross) = if row { (cl.sizing_h, cl.sizing_v) } else { (cl.sizing_v, cl.sizing_h) };
                let mut own = (true, true);
                // Fixed text boxes keep FigCraft's measured size where the text hugs.
                let keep = self.fixed_text_boxes && matches!(n.kind, NodeKind::Text(_));
                match main {
                    Sizing::Fill => {
                        set("flex-grow", "1".into());
                        set("flex-shrink", "1".into());
                        set("flex-basis", "0".into());
                        if row { own.0 = false } else { own.1 = false }
                    }
                    Sizing::Fixed | Sizing::Hug => {
                        set("flex-shrink", "0".into());
                        if main == Sizing::Hug && !keep {
                            if row { own.0 = false } else { own.1 = false }
                        }
                    }
                }
                match cross {
                    Sizing::Fill => {
                        set("align-self", "stretch".into());
                        if row { own.1 = false } else { own.0 = false }
                    }
                    Sizing::Hug if !keep => {
                        if row {
                            own.1 = false
                        } else {
                            own.0 = false
                        }
                    }
                    Sizing::Hug | Sizing::Fixed => {}
                }
                for (k, v) in [("min-width", cl.min_w), ("max-width", cl.max_w), ("min-height", cl.min_h), ("max-height", cl.max_h)] {
                    if let Some(v) = v {
                        set(k, px(v));
                    }
                }
                if let Some((k, v)) = margin {
                    set(k, px(v));
                }
                self.size(n, own.0, own.1, st);
            }
        }
        if rot != 0.0 {
            st.push(("rotate".into(), format!("{}deg", num(rot))));
        }
    }

    /// `width`/`height` when the element keeps its own size on that axis (text that sizes itself
    /// doesn't, unless text boxes are fixed).
    fn size(&self, n: &Node, w: bool, h: bool, st: &mut Vec<(String, String)>) {
        let (mut w, mut h) = (w, h);
        if let NodeKind::Text(t) = &n.kind
            && !self.fixed_text_boxes
        {
            match t.auto_resize {
                TextAutoResize::WidthAndHeight => (w, h) = (false, false),
                TextAutoResize::Height => h = false,
                TextAutoResize::None | TextAutoResize::Truncate => {}
            }
        }
        if n.layout.is_some() {
            if n.child_layout.sizing_h == Sizing::Hug {
                w = false;
            }
            if n.child_layout.sizing_v == Sizing::Hug {
                h = false;
            }
        }
        if w {
            st.push(("width".into(), px(n.width)));
        }
        if h {
            st.push(("height".into(), px(n.height)));
        }
    }

    fn flex_container(&self, n: &Node, border: [f64; 4], st: &mut Vec<(String, String)>) {
        let Some(al) = &n.layout else { return };
        let mut set = |k: &str, v: String| st.push((k.to_string(), v));
        let reversed = al.reverse_z && !self.version_has(Feature::ZIndex);
        let row = !matches!(al.mode, LayoutMode::Vertical);
        let dir = match (row, reversed) {
            (true, false) => "row",
            (true, true) => "row-reverse",
            (false, false) => "column",
            (false, true) => "column-reverse",
        };
        set("flex-direction", dir.into());
        // A reversed direction packs from the other end: swap start and end to keep the look.
        let (start, end) = if reversed { ("flex-end", "flex-start") } else { ("flex-start", "flex-end") };
        let justify = match al.main_align {
            MainAlign::Min => start,
            MainAlign::Center => "center",
            MainAlign::Max => end,
            MainAlign::SpaceBetween => "space-between",
            MainAlign::SpaceAround => "space-around",
            MainAlign::SpaceEvenly => "space-evenly",
        };
        set("justify-content", justify.into());
        let align = match al.cross_align {
            CrossAlign::Min | CrossAlign::Baseline => "flex-start",
            CrossAlign::Center => "center",
            CrossAlign::Max => "flex-end",
        };
        set("align-items", align.into());
        let wrap = al.wrap || al.mode == LayoutMode::Grid;
        if wrap {
            set("flex-wrap", "wrap".into());
            let content = if al.content_align == figcraft_doc::ContentAlign::SpaceBetween { "space-between" } else { align };
            set("align-content", content.into());
        }
        if self.version_has(Feature::Gap) {
            let auto = matches!(al.main_align, MainAlign::SpaceBetween | MainAlign::SpaceAround | MainAlign::SpaceEvenly);
            let main = if auto { 0.0 } else { al.gap };
            let cross = if wrap { al.cross_gap } else { 0.0 };
            if !wrap && main != 0.0 {
                set("gap", px(main));
            } else if main != 0.0 || cross != 0.0 {
                let (rg, cg) = if row { (cross, main) } else { (main, cross) };
                if rg == cg {
                    set("gap", px(rg));
                } else {
                    set("row-gap", px(rg));
                    set("column-gap", px(cg));
                }
            }
        }
        // UITK borders always take space: without "strokes included", the padding gives it back.
        let [t, r, b, l] = al.padding;
        let pad = |p: f64, bw: f64| if al.strokes_included { p } else { (p - bw).max(0.0) };
        set("padding-top", px(pad(t, border[0])));
        set("padding-right", px(pad(r, border[1])));
        set("padding-bottom", px(pad(b, border[2])));
        set("padding-left", px(pad(l, border[3])));
    }

    /// The page area to bake `n` from, when UITK can't draw it at the target version (vector
    /// shapes, and leaf layers whose paint or effects need it). Text stays live.
    fn bake_area(&self, n: &Node) -> Option<Rect> {
        if matches!(n.kind, NodeKind::Text(_)) {
            return None;
        }
        let leaf = n.children.is_empty();
        let fills: Vec<&Paint> = n.fills.iter().filter(|p| p.visible).collect();
        let paint_needs = |p: &Paint| match &p.kind {
            PaintKind::Gradient(g) => !(self.version_has(Feature::Gradient) && matches!(g.kind, GradientKind::Linear | GradientKind::Radial)),
            PaintKind::Image(img) => matches!(img.scale_mode, ImageScaleMode::Crop | ImageScaleMode::Tile),
            PaintKind::Pattern(_) => true,
            PaintKind::Solid { .. } => false,
        };
        let effect_needs = n.effects.iter().filter(|e| e.visible).any(|e| match &e.kind {
            EffectKind::DropShadow(_) => !self.version_has(Feature::DropShadow),
            EffectKind::LayerBlur(_) => !self.version_has(Feature::Filter),
            EffectKind::InnerShadow(_) | EffectKind::Noise(_) | EffectKind::Texture(_) | EffectKind::Glass(_) => true,
            EffectKind::BackgroundBlur(_) => false,
        });
        let stroke_needs = n.strokes.iter().any(|p| p.visible && !matches!(p.kind, PaintKind::Solid { .. })) || !n.stroke.dashes.is_empty();
        let needs = is_vector(&n.kind) || (leaf && (fills.len() > 1 || fills.iter().any(|p| paint_needs(p)) || effect_needs || stroke_needs));
        if !needs {
            return None;
        }
        self.doc.world_render_bounds(n.id).filter(|r| r.width() > 0.0 && r.height() > 0.0 && r.width() <= 16_384.0 && r.height() <= 16_384.0)
    }

    /// A baked layer's look: its PNG as the background when it covers just the layer, else drawn by
    /// an overlay child ([`Self::baked_overlay`]).
    fn baked_look(&mut self, n: &Node, area: Rect, st: &mut Vec<(String, String)>) {
        let file = format!("images/baked-{}.png", n.id.0);
        self.bakes.push(Bake { id: n.id, file: file.clone(), area });
        if self.overlay_offset(n, area).is_none() {
            st.push(("background-image".into(), format!("url(\"{file}\")")));
            st.push(("-unity-background-scale-mode".into(), "stretch-to-fill".into()));
        }
    }

    /// Where a baked PNG reaching past the layer (shadows, outside strokes) sits relative to the
    /// layer's box; `None` when it covers the box.
    fn overlay_offset(&self, n: &Node, area: Rect) -> Option<Rect> {
        let b = self.doc.world_bounds(n.id)?;
        let same = (area.x0 - b.x0).abs() < 0.5 && (area.y0 - b.y0).abs() < 0.5 && (area.x1 - b.x1).abs() < 0.5 && (area.y1 - b.y1).abs() < 0.5;
        (!same).then(|| Rect::new(area.x0 - b.x0, area.y0 - b.y0, area.x1 - b.x0, area.y1 - b.y0))
    }

    fn baked_overlay(&self, n: &Node, area: Rect) -> Option<(String, Vec<(String, String)>)> {
        let off = self.overlay_offset(n, area)?;
        let cls = format!("fc-{}-baked", n.id.0);
        let st = vec![
            ("position".to_string(), "absolute".to_string()),
            ("left".into(), px(off.x0)),
            ("top".into(), px(off.y0)),
            ("width".into(), px(off.width())),
            ("height".into(), px(off.height())),
            ("background-image".into(), format!("url(\"images/baked-{}.png\")", n.id.0)),
            ("-unity-background-scale-mode".into(), "stretch-to-fill".into()),
        ];
        Some((cls, st))
    }

    /// Fills, border, corners, opacity, clipping and effects UITK draws itself.
    fn look(&mut self, n: &Node, st: &mut Vec<(String, String)>) {
        let text = matches!(n.kind, NodeKind::Text(_));
        if !text {
            let fills: Vec<&Paint> = n.fills.iter().filter(|p| p.visible).collect();
            // The bottom solid colour, and the top image or gradient.
            if let Some((c, o)) = fills.iter().find_map(|p| match &p.kind {
                PaintKind::Solid { color } => Some((*color, p.opacity)),
                _ => None,
            }) {
                st.push(("background-color".into(), rgba(c, o)));
            }
            for p in fills.iter().rev() {
                match &p.kind {
                    PaintKind::Image(img) => {
                        let file = self.image_file(&img.image);
                        let mode = if img.scale_mode == ImageScaleMode::Fit { "scale-to-fit" } else { "scale-and-crop" };
                        st.push(("background-image".into(), format!("url(\"{file}\")")));
                        st.push(("-unity-background-scale-mode".into(), mode.into()));
                        break;
                    }
                    PaintKind::Gradient(g)
                        if !g.stops.is_empty()
                            && self.version_has(Feature::Gradient)
                            && matches!(g.kind, GradientKind::Linear | GradientKind::Radial) =>
                    {
                        st.push(("background-image".into(), gradient_css(g, p.opacity, n.width, n.height)));
                        break;
                    }
                    PaintKind::Gradient(g) if !st.iter().any(|(k, _)| k == "background-color") => {
                        // A container can't be baked (its children would be in the PNG): the first stop.
                        if let Some(s) = g.stops.first() {
                            st.push(("background-color".into(), rgba(s.color, p.opacity)));
                        }
                    }
                    _ => {}
                }
            }
            let bw = border_widths(n);
            if bw.iter().any(|w| *w > 0.0) {
                if let Some((c, o)) = n.strokes.iter().filter(|p| p.visible).find_map(|p| match &p.kind {
                    PaintKind::Solid { color } => Some((*color, p.opacity)),
                    _ => None,
                }) {
                    st.push(("border-color".into(), rgba(c, o)));
                }
                for (k, w) in ["border-top-width", "border-right-width", "border-bottom-width", "border-left-width"].into_iter().zip(bw) {
                    st.push((k.into(), px(w)));
                }
            }
            let [tl, tr, br, bl] = n.corners.radii;
            for (k, r) in [
                ("border-top-left-radius", tl),
                ("border-top-right-radius", tr),
                ("border-bottom-right-radius", br),
                ("border-bottom-left-radius", bl),
            ] {
                if r > 0.0 {
                    st.push((k.into(), px(r)));
                }
            }
            if n.clip && n.kind.is_frame_like() {
                st.push(("overflow".into(), "hidden".into()));
            }
        }
        if n.opacity < 1.0 {
            st.push(("opacity".into(), num(f64::from(n.opacity.clamp(0.0, 1.0)))));
        }
        let mut filters = Vec::new();
        for e in n.effects.iter().filter(|e| e.visible) {
            match &e.kind {
                EffectKind::DropShadow(s) if self.version_has(Feature::DropShadow) => {
                    filters.push(format!("drop-shadow({} {} {} {})", px(s.offset.0), px(s.offset.1), px(s.radius), rgba(s.color, 1.0)));
                }
                EffectKind::LayerBlur(b) if self.version_has(Feature::Filter) => filters.push(format!("blur({})", px(b.radius / 2.0))),
                EffectKind::BackgroundBlur(b) if self.version_has(Feature::BackdropFilter) => {
                    st.push(("backdrop-filter".into(), format!("blur({})", px(b.radius / 2.0))));
                }
                _ => {}
            }
        }
        if !filters.is_empty() {
            st.push(("filter".into(), filters.join(" ")));
        }
    }

    fn text_style(&self, n: &Node, t: &figcraft_doc::TextData, st: &mut Vec<(String, String)>) {
        let mut set = |k: &str, v: String| st.push((k.to_string(), v));
        let colour = n.fills.iter().filter(|p| p.visible).find_map(|p| match &p.kind {
            PaintKind::Solid { color } => Some(rgba(*color, p.opacity)),
            PaintKind::Gradient(g) => g.stops.first().map(|s| rgba(s.color, p.opacity)),
            _ => None,
        });
        set("color", colour.unwrap_or_else(|| "rgba(0, 0, 0, 1)".into()));
        set("font-size", px(t.style.font_size));
        let style = t.style.font_style.to_ascii_lowercase();
        let bold = ["bold", "black", "heavy", "semibold", "semi bold", "extrabold", "extra bold"].iter().any(|w| style.contains(w));
        let italic = style.contains("italic") || style.contains("oblique");
        let fs = match (bold, italic) {
            (false, false) => "normal",
            (true, false) => "bold",
            (false, true) => "italic",
            (true, true) => "bold-and-italic",
        };
        set("-unity-font-style", fs.into());
        // The font asset is the project's: the family and style travel as custom properties.
        set("--fc-font-family", format!("\"{}\"", t.style.font_family.replace('"', "'")));
        set("--fc-font-style", format!("\"{}\"", t.style.font_style.replace('"', "'")));
        match t.style.letter_spacing {
            LetterSpacing::Pixels(v) if v != 0.0 => set("letter-spacing", px(v)),
            LetterSpacing::Percent(p) if p != 0.0 => set("letter-spacing", px(p / 100.0 * t.style.font_size)),
            _ => {}
        }
        let v = match t.align_v {
            TextAlignV::Top => "upper",
            TextAlignV::Middle => "middle",
            TextAlignV::Bottom => "lower",
        };
        let h = match t.align_h {
            TextAlignH::Left | TextAlignH::Justified => "left",
            TextAlignH::Center => "center",
            TextAlignH::Right => "right",
        };
        set("-unity-text-align", format!("{v}-{h}"));
        let wraps = t.auto_resize != TextAutoResize::WidthAndHeight;
        set("white-space", if wraps { "normal" } else { "nowrap" }.into());
        if t.auto_resize == TextAutoResize::Truncate {
            set("text-overflow", "ellipsis".into());
            set("overflow", "hidden".into());
        }
    }

    fn image_file(&mut self, hash: &str) -> String {
        if let Some(f) = self.images.get(hash) {
            return f.clone();
        }
        let ext = self.doc.images.get(hash).map_or("png", |d| image_ext(&d.bytes));
        let safe: String = hash.chars().filter(|c| c.is_ascii_alphanumeric()).take(40).collect();
        let file = format!("images/{}.{ext}", if safe.is_empty() { "image" } else { &safe });
        self.images.insert(hash.to_string(), file.clone());
        file
    }

    fn rule(&mut self, selector: &str, st: &[(String, String)]) {
        if st.is_empty() {
            return;
        }
        let _ = writeln!(self.uss, "\n{selector} {{");
        for (k, v) in st {
            let _ = writeln!(self.uss, "    {k}: {v};");
        }
        self.uss.push_str("}\n");
    }
}

/// An image's file extension from its first bytes.
fn image_ext(b: &[u8]) -> &'static str {
    if b.starts_with(b"\x89PNG") {
        "png"
    } else if b.starts_with(&[0xFF, 0xD8]) {
        "jpg"
    } else if b.starts_with(b"GIF8") {
        "gif"
    } else if b.len() >= 12 && b.get(8..12) == Some(b"WEBP") {
        "webp"
    } else {
        "png"
    }
}

/// The text a Label shows: the characters, with the line height as a rich text tag (USS has none).
fn label_text(t: &figcraft_doc::TextData) -> String {
    let lh = match t.style.line_height {
        LineHeight::Auto => None,
        LineHeight::Pixels(v) => Some(v),
        LineHeight::Percent(p) => Some(p / 100.0 * t.style.font_size),
    };
    match lh.filter(|v| v.is_finite() && *v > 0.0) {
        Some(v) => format!("<line-height={}px>{}", num(v), t.characters),
        None => t.characters.clone(),
    }
}

/// A gradient as USS `linear-gradient()` / `radial-gradient()` (6.7+), its angle from the handles
/// on a `w × h` box.
fn gradient_css(g: &figcraft_doc::color::Gradient, opacity: f32, w: f64, h: f64) -> String {
    let stops: Vec<String> = g.stops.iter().map(|s| format!("{} {}", rgba(s.color, opacity), pct(f64::from(s.position)))).collect();
    match g.kind {
        GradientKind::Radial => format!("radial-gradient({})", stops.join(", ")),
        _ => {
            let (dx, dy) = ((g.to.0 - g.from.0) * w, (g.to.1 - g.from.1) * h);
            let deg = if dx == 0.0 && dy == 0.0 { 180.0 } else { dx.atan2(-dy).to_degrees() };
            format!("linear-gradient({}deg, {})", num(deg), stops.join(", "))
        }
    }
}

/// One axis's constraint.
#[derive(Clone, Copy)]
enum Pin {
    Start,
    End,
    Center,
    Stretch,
    Scale,
}

impl From<HConstraint> for Pin {
    fn from(c: HConstraint) -> Self {
        match c {
            HConstraint::Left => Pin::Start,
            HConstraint::Right => Pin::End,
            HConstraint::Center => Pin::Center,
            HConstraint::LeftRight => Pin::Stretch,
            HConstraint::Scale => Pin::Scale,
        }
    }
}

impl From<VConstraint> for Pin {
    fn from(c: VConstraint) -> Self {
        match c {
            VConstraint::Top => Pin::Start,
            VConstraint::Bottom => Pin::End,
            VConstraint::Center => Pin::Center,
            VConstraint::TopBottom => Pin::Stretch,
            VConstraint::Scale => Pin::Scale,
        }
    }
}

struct AxisOut {
    /// The element keeps its own size on this axis (`width`/`height` in px).
    own: bool,
    /// The pivot's `translate` applies on this axis.
    translate: bool,
}

/// The USS for one axis of an absolute layer at `pos` (size `len`) in a box `box_len`, pinned by
/// `pin`, turning about `pivot` (0–1). Start/end use the plain offsets when the pivot is that edge;
/// otherwise the anchor is a percentage, the pivot a `translate` and the rest a margin.
#[allow(clippy::too_many_arguments)]
fn axis(
    pin: Pin,
    pos: f64,
    len: f64,
    box_len: f64,
    pivot: f64,
    start: &'static str,
    end: &'static str,
    margin: &'static str,
    size: &'static str,
) -> (AxisOut, Vec<(&'static str, String)>) {
    let anchor = match pin {
        Pin::Start => 0.0,
        Pin::End => 1.0,
        Pin::Center => 0.5,
        Pin::Stretch => return (AxisOut { own: false, translate: false }, vec![(start, px(pos)), (end, px(box_len - pos - len))]),
        Pin::Scale => {
            let share = |v: f64| if box_len > 0.0 { v / box_len } else { 0.0 };
            return (AxisOut { own: false, translate: false }, vec![(start, pct(share(pos))), (size, pct(share(len)))]);
        }
    };
    if anchor == 0.0 && pivot == 0.0 {
        return (AxisOut { own: true, translate: false }, vec![(start, px(pos))]);
    }
    if anchor == 1.0 && pivot == 1.0 {
        return (AxisOut { own: true, translate: false }, vec![(end, px(box_len - pos - len))]);
    }
    let offset = pos + pivot * len - anchor * box_len;
    let mut v = vec![(start, pct(anchor))];
    if offset.abs() > 1e-9 {
        v.push((margin, px(offset)));
    }
    (AxisOut { own: true, translate: pivot != 0.0 }, v)
}

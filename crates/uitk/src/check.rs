//! The compatibility check: what a design uses that the target Unity version's UXML/USS can't
//! represent exactly (rules and severities: `docs/unity-uitk.md` › Compatibility check).

use std::collections::HashSet;

use figcraft_doc::color::{BlendMode, GradientKind, ImageScaleMode, PaintKind};
use figcraft_doc::{CrossAlign, Document, EffectKind, LayoutMode, LineHeight, ListStyle, MainAlign, Node, NodeId, NodeKind, StrokeAlign};
use serde::Serialize;

use crate::{Feature, UnityVersion};

/// Issues reported at most (a huge file still answers quickly).
const MAX_ISSUES: usize = 10_000;
/// Layers walked at most.
const MAX_NODES: usize = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Can't be represented: the export drops or flattens it.
    Error,
    /// Approximated or baked into an image.
    Warning,
    /// Converted to an equivalent construct.
    Info,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: NodeId,
    pub name: String,
    pub severity: Severity,
    /// The rule's id (`gap`, `drop-shadow`, …).
    pub rule: &'static str,
    pub message: String,
    /// The first Unity version that represents it natively, when one does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<UnityVersion>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckOptions {
    pub version: UnityVersion,
    /// The project turns on UITK's experimental grid layout (6.7+).
    pub grid_enabled: bool,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self { version: UnityVersion::MIN, grid_enabled: false }
    }
}

/// Check `roots` and everything visible under them (a page: its layers), in layer order.
pub fn check(doc: &Document, roots: &[NodeId], opts: CheckOptions) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        let start: Vec<NodeId> = match doc.node(*root) {
            Some(n) if matches!(n.kind, NodeKind::Page { .. }) => n.children.clone(),
            Some(_) => vec![*root],
            None => continue,
        };
        // Pre-order, children in layer order.
        let mut stack: Vec<NodeId> = start.into_iter().rev().collect();
        while let Some(id) = stack.pop() {
            if seen.len() >= MAX_NODES || out.len() >= MAX_ISSUES {
                out.truncate(MAX_ISSUES);
                return out;
            }
            if !seen.insert(id) {
                continue;
            }
            let Some(n) = doc.node(id) else { continue };
            // Hidden layers are exported as `display: none`; what they look like doesn't matter.
            if !n.visible {
                continue;
            }
            Checker { doc, n, opts, out: &mut out }.node();
            // A vector or boolean group is one image; its operands aren't elements of their own.
            if !is_vector(&n.kind) {
                stack.extend(n.children.iter().rev().copied());
            }
        }
    }
    out.truncate(MAX_ISSUES);
    out
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

fn is_plain_blend(b: BlendMode) -> bool {
    matches!(b, BlendMode::Normal | BlendMode::PassThrough)
}

struct Checker<'a> {
    doc: &'a Document,
    n: &'a Node,
    opts: CheckOptions,
    out: &'a mut Vec<Issue>,
}

impl Checker<'_> {
    fn push(&mut self, severity: Severity, rule: &'static str, message: impl Into<String>, since: Option<UnityVersion>) {
        self.out.push(Issue { id: self.n.id, name: self.n.name.clone(), severity, rule, message: message.into(), since });
    }

    fn v(&self) -> UnityVersion {
        self.opts.version
    }

    fn node(&mut self) {
        let n = self.n;
        if matches!(n.kind, NodeKind::Slice) {
            return;
        }
        if is_vector(&n.kind) {
            let since = Feature::VectorImage.since();
            if self.v().has(Feature::VectorImage) {
                self.push(Severity::Info, "vector", "exported as a Vector Image (SVG)", Some(since));
            } else {
                self.push(Severity::Warning, "vector", format!("vector shapes are baked into an image before Unity {since}"), Some(since));
            }
        }
        if n.pivot.is_some() && crate::layer_info(self.doc, n.id).is_some_and(|i| i.position != crate::Position::Absolute) {
            self.push(Severity::Warning, "pivot", "the pivot only places absolute layers: this one is placed by its parent", None);
        }
        if n.is_mask {
            self.push(Severity::Error, "mask", "masks have no UITK equivalent (only clip content: `overflow: hidden`)", None);
        }
        if !is_plain_blend(n.blend) {
            self.push(Severity::Warning, "blend", "UITK has no blend modes: the layer is drawn normally", None);
        }
        if let NodeKind::Text(t) = &n.kind {
            self.text(t);
        } else if !is_vector(&n.kind) {
            self.fills();
            self.strokes();
            if n.corners.smoothing > 0.0 {
                self.push(Severity::Warning, "corner-smoothing", "corner smoothing becomes plain rounded corners", None);
            }
        }
        self.layout();
        self.effects();
    }

    fn layout(&mut self) {
        let n = self.n;
        let Some(al) = &n.layout else { return };
        let v = self.v();
        if al.mode == LayoutMode::Grid {
            let since = Feature::Grid.since();
            if !v.has(Feature::Grid) {
                self.push(Severity::Error, "grid-layout", format!("grid auto layout needs Unity {since} (experimental)"), Some(since));
            } else if !self.opts.grid_enabled {
                self.push(
                    Severity::Error,
                    "grid-layout",
                    "grid auto layout needs Enable Grid Layout in Project Settings ▸ UI Toolkit (experimental)",
                    Some(since),
                );
            } else {
                self.push(Severity::Info, "grid-layout", "exported as `display: grid` (experimental)", Some(since));
            }
            return;
        }
        let auto_gap = matches!(al.main_align, MainAlign::SpaceBetween | MainAlign::SpaceAround | MainAlign::SpaceEvenly);
        let gap = (!auto_gap && al.gap != 0.0) || (al.wrap && al.cross_gap != 0.0);
        if gap && !v.has(Feature::Gap) {
            let since = Feature::Gap.since();
            if al.wrap {
                self.push(
                    Severity::Warning,
                    "gap",
                    format!("gaps in a wrapping layout become margins (and a negative container margin) before Unity {since}"),
                    Some(since),
                );
            } else {
                self.push(Severity::Info, "gap", format!("the gap becomes margins on the children before Unity {since}"), Some(since));
            }
        }
        if al.cross_align == CrossAlign::Baseline {
            self.push(Severity::Error, "baseline-align", "UITK can't align children by their text baseline", None);
        }
        if !al.strokes_included && self.has_stroke() {
            self.push(
                Severity::Warning,
                "strokes-in-layout",
                "UITK borders always take space in the layout: the content moves in by the stroke width",
                None,
            );
        }
        if al.reverse_z {
            let since = Feature::ZIndex.since();
            let how = if v.has(Feature::ZIndex) { "`z-index`" } else { "reversed children with a `-reverse` direction" };
            self.push(Severity::Info, "z-order", format!("\"first on top\" stacking is exported as {how}"), Some(since));
        }
    }

    fn has_stroke(&self) -> bool {
        let s = &self.n.stroke;
        let weight = s.sides.map_or(s.weight, |w| w.iter().copied().fold(0.0, f64::max));
        weight > 0.0 && self.n.strokes.iter().any(|p| p.visible)
    }

    fn fills(&mut self) {
        let fills: Vec<_> = self.n.fills.iter().filter(|p| p.visible).cloned().collect();
        if fills.len() > 1 {
            self.push(Severity::Warning, "fill-count", "UITK has one background colour and one image: extra fills are baked", None);
        }
        for p in &fills {
            if !is_plain_blend(p.blend) {
                self.push(Severity::Warning, "blend", "fill blend modes aren't supported: drawn normally", None);
            }
            match &p.kind {
                PaintKind::Gradient(g) => self.gradient(g.kind),
                PaintKind::Image(img) if matches!(img.scale_mode, ImageScaleMode::Crop | ImageScaleMode::Tile) => {
                    self.push(Severity::Warning, "image-mode", "crop and tile image fills are baked (UITK scales to fill or fit)", None);
                }
                PaintKind::Pattern(_) => self.push(Severity::Warning, "image-mode", "pattern fills are baked into an image", None),
                _ => {}
            }
        }
    }

    fn gradient(&mut self, kind: GradientKind) {
        let since = Feature::Gradient.since();
        match kind {
            GradientKind::Linear | GradientKind::Radial if self.v().has(Feature::Gradient) => {}
            GradientKind::Linear | GradientKind::Radial => {
                self.push(Severity::Warning, "gradient", format!("gradients are baked into an image before Unity {since}"), Some(since));
            }
            GradientKind::Angular | GradientKind::Diamond => {
                self.push(Severity::Warning, "gradient", "angular and diamond gradients are baked into an image (UITK has linear and radial)", None);
            }
        }
    }

    fn strokes(&mut self) {
        if !self.has_stroke() {
            return;
        }
        let s = &self.n.stroke;
        if s.align != StrokeAlign::Inside {
            self.push(Severity::Warning, "stroke-align", "UITK borders are inside the box: centre and outside strokes move inside", None);
        }
        if !s.dashes.is_empty() || s.width_profile.is_some() || s.brush.is_some() {
            self.push(Severity::Error, "stroke-style", "dashed, variable-width and brush strokes have no UITK border equivalent", None);
        }
        if matches!(self.n.kind, NodeKind::Group | NodeKind::Section { .. }) {
            return;
        }
        for p in self.n.strokes.iter().filter(|p| p.visible) {
            if !matches!(p.kind, PaintKind::Solid { .. }) {
                self.push(Severity::Warning, "stroke-paint", "UITK borders are one colour: gradient and image strokes are baked", None);
            }
        }
    }

    fn effects(&mut self) {
        let v = self.v();
        for e in self.n.effects.iter().filter(|e| e.visible) {
            match &e.kind {
                EffectKind::DropShadow(sh) => {
                    let since = Feature::DropShadow.since();
                    if !v.has(Feature::DropShadow) {
                        self.push(
                            Severity::Warning,
                            "drop-shadow",
                            format!("drop shadows are baked into an image before Unity {since}"),
                            Some(since),
                        );
                    } else if sh.spread != 0.0 || sh.show_behind {
                        self.push(
                            Severity::Warning,
                            "drop-shadow",
                            "`drop-shadow()` follows the layer's outline: spread and \"show behind\" are dropped",
                            Some(since),
                        );
                    }
                    if !is_plain_blend(sh.blend) {
                        self.push(Severity::Warning, "blend", "shadow blend modes aren't supported: drawn normally", None);
                    }
                }
                EffectKind::InnerShadow(_) => {
                    self.push(Severity::Error, "inner-shadow", "UITK has no inner shadow", None);
                }
                EffectKind::LayerBlur(_) if !v.has(Feature::Filter) => {
                    let since = Feature::Filter.since();
                    self.push(
                        Severity::Warning,
                        "layer-blur",
                        format!("layer blur needs `filter: blur()` (Unity {since}); baked before"),
                        Some(since),
                    );
                }
                EffectKind::BackgroundBlur(_) if !v.has(Feature::BackdropFilter) => {
                    let since = Feature::BackdropFilter.since();
                    self.push(Severity::Warning, "background-blur", format!("background blur needs `backdrop-filter` (Unity {since})"), Some(since));
                }
                EffectKind::Noise(_) | EffectKind::Texture(_) | EffectKind::Glass(_) => {
                    self.push(Severity::Error, "effect-other", "noise, texture and glass have no UITK equivalent", None);
                }
                _ => {}
            }
        }
    }

    fn text(&mut self, t: &figcraft_doc::TextData) {
        if t.style.line_height != LineHeight::Auto || t.runs.iter().any(|r| r.style.line_height != LineHeight::Auto) {
            self.push(Severity::Warning, "text-line-height", "USS has no line height: approximated with the `<line-height>` rich text tag", None);
        }
        if t.list != ListStyle::None || !t.paragraphs.is_empty() || t.paragraph_indent != 0.0 {
            self.push(Severity::Warning, "text-features", "lists and paragraph indents have no USS equivalent", None);
        }
        if t.style.decoration_look.is_some() {
            self.push(
                Severity::Warning,
                "text-features",
                "decoration styles (thickness, offset, colour) are drawn as plain underline/strikethrough",
                None,
            );
        }
        if !t.runs.is_empty() {
            self.push(Severity::Info, "text-runs", "mixed styles are exported as rich text tags", None);
        }
        let solid = |fills: &[figcraft_doc::color::Paint]| fills.iter().filter(|p| p.visible).all(|p| matches!(p.kind, PaintKind::Solid { .. }));
        if !solid(&self.n.fills) || t.runs.iter().any(|r| r.style.fills.as_deref().is_some_and(|f| !solid(f))) {
            self.push(Severity::Warning, "text-fill", "text colour is one solid colour in UITK: gradient and image text fills are baked", None);
        }
    }
}

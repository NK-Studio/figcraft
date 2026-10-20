//! How a layer is placed in UI Toolkit: in its parent's flow (`position: relative`) or on its own
//! (`absolute`), and its pivot — stored on the layer or implied by its constraints
//! (`docs/unity-uitk.md` › Pivot).

use figcraft_doc::{Constraints, Document, HConstraint, NodeId, NodeKind, Pivot, VConstraint};
use serde::Serialize;

use crate::{CheckOptions, UnityVersion};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    /// A top-level frame (on the page or in a section): the panel's root, placed by Unity.
    Root,
    /// In its auto layout parent's flow.
    Relative,
    /// Placed on its own: outside auto layout, or set to ignore it.
    Absolute,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerInfo {
    pub position: Position,
    /// Whether the parent has auto layout (the position can be switched then).
    pub in_auto_layout: bool,
    /// The pivot used: the layer's own, else the one its constraints imply.
    pub pivot: Pivot,
    /// The layer sets its own pivot.
    pub pivot_set: bool,
    /// The anchors its constraints give: "left", "right", "center", "stretch" or "scale" across,
    /// "top", "bottom", "center", "stretch" or "scale" down.
    pub anchor_h: &'static str,
    pub anchor_v: &'static str,
}

/// The pivot a layer's constraints imply: the edge it's pinned to, or its centre.
pub fn implied_pivot(c: &Constraints) -> Pivot {
    let x = match c.horizontal {
        HConstraint::Right => 1.0,
        HConstraint::Center => 0.5,
        HConstraint::Left | HConstraint::LeftRight | HConstraint::Scale => 0.0,
    };
    let y = match c.vertical {
        VConstraint::Bottom => 1.0,
        VConstraint::Center => 0.5,
        VConstraint::Top | VConstraint::TopBottom | VConstraint::Scale => 0.0,
    };
    Pivot::new(x, y)
}

/// Where layer `id` sits and pivots in UI Toolkit; `None` for unknown ids and pages.
pub fn layer_info(doc: &Document, id: NodeId) -> Option<LayerInfo> {
    let n = doc.node(id)?;
    if matches!(n.kind, NodeKind::Page { .. }) {
        return None;
    }
    let parent = n.parent.and_then(|p| doc.node(p));
    let in_auto_layout = parent.is_some_and(|p| p.layout.is_some());
    let position = match parent.map(|p| &p.kind) {
        None | Some(NodeKind::Page { .. }) | Some(NodeKind::Section { .. }) => Position::Root,
        _ if in_auto_layout && !n.child_layout.absolute => Position::Relative,
        _ => Position::Absolute,
    };
    let anchor_h = match n.constraints.horizontal {
        HConstraint::Left => "left",
        HConstraint::Right => "right",
        HConstraint::Center => "center",
        HConstraint::LeftRight => "stretch",
        HConstraint::Scale => "scale",
    };
    let anchor_v = match n.constraints.vertical {
        VConstraint::Top => "top",
        VConstraint::Bottom => "bottom",
        VConstraint::Center => "center",
        VConstraint::TopBottom => "stretch",
        VConstraint::Scale => "scale",
    };
    Some(LayerInfo {
        position,
        in_auto_layout,
        pivot: n.pivot.map(Pivot::clamped).unwrap_or_else(|| implied_pivot(&n.constraints)),
        pivot_set: n.pivot.is_some(),
        anchor_h,
        anchor_v,
    })
}

/// The check options a file asks for (its Unity settings; Unity 6.0 without grid by default, or
/// when the stored version can't be read).
pub fn file_options(doc: &Document) -> CheckOptions {
    match &doc.unity {
        Some(u) => CheckOptions { version: UnityVersion::parse(&u.version).unwrap_or(UnityVersion::MIN), grid_enabled: u.grid },
        None => CheckOptions::default(),
    }
}

//! A frame's UI scaling as Unity's PanelSettings (`docs/unity-uitk.md` › UI scaling). The export
//! writes it next to the frame's UXML as `<stem>.panelsettings.json`; the FigCraft Unity package
//! (`com.nkstudio.figcraft`) writes that into the PanelSettings asset when it is imported, so the
//! scaling is Unity's own, in edit mode as at run time, with no script.

use figcraft_doc::{CanvasScaler, DeviceFrame, Node, ScreenMatchMode, UiScaleMode};
use serde::Serialize;

/// A frame's UI scaling: its own setting, or the short side of the frame when it has none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelScaling {
    pub setting: CanvasScaler,
    /// The frame has a setting of its own (`Node::canvas_scaler`).
    pub from_frame: bool,
    /// Panel units per point on the frame's own screen (its size and device), when that can be
    /// worked out: what the export multiplies lengths by.
    pub units_per_point: Option<f64>,
}

/// The UI scaling frame `n` exports with. Unity keeps the reference resolution in whole units.
pub fn panel_scaling(n: &Node) -> PanelScaling {
    let (mut setting, from_frame) = match n.canvas_scaler {
        Some(c) => (c, true),
        None => (CanvasScaler::short_side(n.width.min(n.height)), false),
    };
    let whole = |v: f64| if v.is_finite() { v.round().clamp(1.0, 100_000.0) } else { 1.0 };
    setting.reference_width = whole(setting.reference_width);
    setting.reference_height = whole(setting.reference_height);
    let density = n.device.as_ref().map(DeviceFrame::density).unwrap_or_default();
    let units_per_point = setting.points_per_unit(n.width, n.height, density).map(|u| 1.0 / u).filter(|k| k.is_finite() && *k > 0.0);
    PanelScaling { setting, from_frame, units_per_point }
}

/// `<stem>.panelsettings.json`: the fields of Unity's PanelSettings it sets, with Unity's names.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelSettingsFile {
    /// `PanelScaleMode`: ConstantPixelSize, ConstantPhysicalSize or ScaleWithScreenSize.
    pub scale_mode: &'static str,
    pub scale: f64,
    pub reference_resolution: WholeSize,
    /// `PanelScreenMatchMode`: MatchWidthOrHeight, Shrink or Expand.
    pub screen_match_mode: &'static str,
    #[serde(rename = "match")]
    pub match_width_or_height: f64,
    pub reference_dpi: f64,
    pub fallback_dpi: f64,
    /// Panel units per point on the design screen (information: the export's lengths use it).
    pub units_per_point: f64,
    /// "frame" (the frame's setting) or "shortSide" (no setting: the frame's short side).
    pub source: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct WholeSize {
    pub x: i32,
    pub y: i32,
}

impl PanelScaling {
    pub fn file(&self) -> PanelSettingsFile {
        let c = &self.setting;
        let finite = |v: f64, or: f64| if v.is_finite() && v > 0.0 { v } else { or };
        PanelSettingsFile {
            scale_mode: match c.scale_mode {
                UiScaleMode::ConstantPixelSize => "ConstantPixelSize",
                UiScaleMode::ScaleWithScreenSize => "ScaleWithScreenSize",
                UiScaleMode::ConstantPhysicalSize => "ConstantPhysicalSize",
            },
            scale: finite(c.scale_factor, 1.0),
            // Whole and within 1–100000 (`panel_scaling`), so the casts are exact.
            reference_resolution: WholeSize { x: c.reference_width as i32, y: c.reference_height as i32 },
            screen_match_mode: match c.screen_match_mode {
                ScreenMatchMode::MatchWidthOrHeight => "MatchWidthOrHeight",
                ScreenMatchMode::Expand => "Expand",
                ScreenMatchMode::Shrink => "Shrink",
            },
            match_width_or_height: if c.match_width_or_height.is_finite() { c.match_width_or_height.clamp(0.0, 1.0) } else { 0.0 },
            reference_dpi: finite(c.reference_dpi, 96.0),
            fallback_dpi: finite(c.fallback_dpi, 96.0),
            units_per_point: self.units_per_point.unwrap_or(1.0),
            source: if self.from_frame { "frame" } else { "shortSide" },
        }
    }
}

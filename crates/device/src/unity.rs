//! Unity Device Simulator `.device` files (the JSON the Device Simulator reads; the format is read
//! from the user's own files, never bundled).
//!
//! Unity measures in physical pixels with a **bottom-left** origin in each orientation; profiles
//! use a top-left origin, so the safe area becomes insets (`top = H - (y + height)`, `bottom = y`)
//! and each cutout's `y` becomes `H - (y + height)`. Unity has no scale and its `dpi` is not
//! reliable for iOS devices, so the scale is inferred ([`infer_scale`]) unless the caller gives one.

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::{
    DeviceError, DeviceProfile, Insets, Orientation, OrientationInfo, Overlay, ParseHint, Platform, PxRect, Result, ScaleSource, Screen, Source, slug,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UDevice {
    friendly_name: String,
    #[serde(default)]
    screens: Vec<UScreen>,
    #[serde(default)]
    system_info: Option<USystem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UScreen {
    width: f64,
    height: f64,
    #[serde(default)]
    navigation_bar_height: f64,
    #[serde(default)]
    dpi: Option<f64>,
    #[serde(default)]
    orientations: Vec<UOrientation>,
    #[serde(default)]
    presentation: Option<UPresentation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UOrientation {
    orientation: i64,
    safe_area: URect,
    #[serde(default)]
    cutouts: Vec<URect>,
}

#[derive(Deserialize, Clone, Copy)]
struct URect {
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    height: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UPresentation {
    #[serde(default)]
    overlay_path: String,
    #[serde(default)]
    border_size: Option<UVec4>,
    #[serde(default)]
    corner_radius: f64,
}

/// Unity's `Vector4` border: x left, y top, z right, w bottom.
#[derive(Deserialize, Default)]
struct UVec4 {
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    z: f64,
    #[serde(default)]
    w: f64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct USystem {
    #[serde(default)]
    device_model: String,
    #[serde(default)]
    operating_system: String,
}

/// The platform a device runs, from its operating system, else its model or name.
pub fn platform_of(os: &str, model: &str, name: &str) -> Platform {
    let os = os.trim().to_ascii_lowercase();
    if os.starts_with("ios") || os.starts_with("ipados") {
        return Platform::Ios;
    }
    if os.starts_with("android") {
        return Platform::Android;
    }
    let hay = format!("{model} {name}").to_ascii_lowercase();
    if ["iphone", "ipad", "ipod"].iter().any(|k| hay.contains(k)) { Platform::Ios } else { Platform::Other }
}

/// Pixels per point for a `w × h` px screen (see `docs/device-profiles.md` › Scale).
pub fn infer_scale(platform: Platform, w: f64, h: f64, dpi: Option<f64>) -> (f64, ScaleSource) {
    match platform {
        Platform::Ios => {
            let (short, long) = (w.min(h), w.max(h));
            let s = if short >= 1536.0 || short <= 828.0 {
                2.0
            } else if short == 1080.0 && long == 1920.0 {
                // The "Plus" panels: a 414 × 736 pt layout drawn at 3x and scaled down.
                1080.0 / 414.0
            } else {
                3.0
            };
            (s, ScaleSource::InferredIos)
        }
        Platform::Android => match dpi {
            Some(d) if d.is_finite() && (80.0..=1000.0).contains(&d) => (d / 160.0, ScaleSource::InferredAndroid),
            _ => (1.0, ScaleSource::Default),
        },
        Platform::Other => (1.0, ScaleSource::Default),
    }
}

/// A profile from a parsed `.device` file (validated by the caller, [`crate::parse`]).
pub fn from_value(v: Value, hint: &ParseHint) -> Result<DeviceProfile> {
    let d: UDevice = serde_json::from_value(v).map_err(|e| DeviceError::Format(format!("Unity device: {e}")))?;
    if d.screens.len() > crate::MAX_SCREENS {
        return Err(crate::invalid(format!("more than {} screens", crate::MAX_SCREENS)));
    }
    let sys = d.system_info.unwrap_or_default();
    let platform = platform_of(&sys.operating_system, &sys.device_model, &d.friendly_name);
    let mut screens = Vec::new();
    for s in d.screens {
        screens.push(screen(s, platform, hint)?);
    }
    let some = |s: String| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    Ok(DeviceProfile {
        id: format!("unity/{}", slug(&d.friendly_name)),
        name: d.friendly_name,
        platform,
        source: Source::UnityDevice { file: hint.file.clone() },
        model: some(sys.device_model),
        os: some(sys.operating_system),
        screens,
    })
}

fn screen(s: UScreen, platform: Platform, hint: &ParseHint) -> Result<Screen> {
    if s.orientations.len() > crate::MAX_ORIENTATIONS {
        return Err(crate::invalid(format!("more than {} orientations", crate::MAX_ORIENTATIONS)));
    }
    let (scale, scale_source) = match hint.scale {
        Some(k) => (k, ScaleSource::Override),
        None => infer_scale(platform, s.width, s.height, s.dpi),
    };
    let mut orientations = Vec::new();
    for o in &s.orientations {
        // Values other than 1–4 (Unity's auto-rotation markers) aren't screen orientations.
        let Some(orientation) = Orientation::from_unity(o.orientation) else { continue };
        let (w, h) = if orientation.is_landscape() { (s.height, s.width) } else { (s.width, s.height) };
        let a = o.safe_area;
        let safe_area = Insets { left: a.x, bottom: a.y, right: w - (a.x + a.width), top: h - (a.y + a.height) };
        let cutouts = o.cutouts.iter().map(|c| PxRect { x: c.x, y: h - (c.y + c.height), width: c.width, height: c.height }).collect();
        orientations.push(OrientationInfo { orientation, safe_area, cutouts });
    }
    let (overlay, corner_radius_px) = match s.presentation {
        Some(p) => {
            let overlay = (!p.overlay_path.trim().is_empty()).then(|| {
                let b = p.border_size.unwrap_or_default();
                let path = match &hint.dir {
                    Some(dir) => Path::new(dir).join(p.overlay_path.trim()).to_string_lossy().into_owned(),
                    None => p.overlay_path.trim().to_string(),
                };
                Overlay { path, border_px: Insets { left: b.x, top: b.y, right: b.z, bottom: b.w } }
            });
            (overlay, p.corner_radius)
        }
        None => (None, 0.0),
    };
    Ok(Screen {
        width_px: s.width,
        height_px: s.height,
        dpi: s.dpi.filter(|d| *d > 0.0),
        scale,
        scale_source,
        navigation_bar_px: s.navigation_bar_height,
        corner_radius_px,
        orientations,
        overlay,
    })
}

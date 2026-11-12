//! FigCraft's built-in devices (`docs/device-profiles.md` › Built-in devices): a few generic
//! phones, tablets and a foldable, so a design can be checked on a device before any file is
//! imported.
//!
//! They are **generic** devices written for FigCraft: common screen sizes with round safe areas
//! and cutouts of the usual kinds (a notch, a punch hole in a corner or the middle, a pill, a home
//! indicator). The numbers follow what the platforms publish about their screens; none is read or
//! derived from another product's device files, and none claims to be a particular model. For a
//! real device's exact numbers, import its file (`device.import`).

use crate::{DeviceProfile, Insets, Orientation, OrientationInfo, Platform, PxRect, ScaleSource, Screen, Source};

/// The id prefix of the built-in devices.
pub const PREFIX: &str = "figcraft/";

/// One screen of a built-in device, in pixels of its portrait orientation.
struct ScreenSpec {
    size: (f64, f64),
    dpi: f64,
    /// Pixels per point.
    scale: f64,
    corner: f64,
    /// The safe area in from the top and the bottom in portrait.
    portrait: (f64, f64),
    /// In landscape: in from the side the top of the device is on, from the other side, and
    /// from the bottom.
    landscape: (f64, f64, f64),
    /// The display cutout in portrait: x, y, width, height.
    cutout: Option<(f64, f64, f64, f64)>,
    /// Tablets and unfolded screens also turn upside down.
    upside_down: bool,
}

struct Spec {
    slug: &'static str,
    name: &'static str,
    platform: Platform,
    screens: &'static [ScreenSpec],
}

/// No safe area and no cutout: a screen with a body round it.
const fn plain(size: (f64, f64), dpi: f64, scale: f64, upside_down: bool) -> ScreenSpec {
    ScreenSpec { size, dpi, scale, corner: 0.0, portrait: (0.0, 0.0), landscape: (0.0, 0.0, 0.0), cutout: None, upside_down }
}

/// A phone with a notch and a home indicator: the safe area is in from both sides in landscape.
const fn notch(size: (f64, f64), dpi: f64, scale: f64, corner: f64, (top, bottom, landscape_bottom): (f64, f64, f64), notch_w: f64) -> ScreenSpec {
    ScreenSpec {
        size,
        dpi,
        scale,
        corner,
        portrait: (top, bottom),
        landscape: (top, top, landscape_bottom),
        cutout: Some(((size.0 - notch_w) / 2.0, 0.0, notch_w, 96.0)),
        upside_down: false,
    }
}

/// A screen with a hole or pill in its top edge: the safe area starts under it, and in landscape
/// only on the side it is on.
const fn hole(size: (f64, f64), dpi: f64, corner: f64, inset: f64, cutout: (f64, f64, f64, f64), upside_down: bool) -> ScreenSpec {
    ScreenSpec { size, dpi, scale: dpi / 160.0, corner, portrait: (inset, 0.0), landscape: (inset, 0.0, 0.0), cutout: Some(cutout), upside_down }
}

const SPECS: [Spec; 11] = [
    Spec {
        slug: "notch-phone",
        name: "Notch Phone",
        platform: Platform::Ios,
        screens: &[notch((1170.0, 2532.0), 460.0, 3.0, 142.0, (141.0, 102.0, 63.0), 486.0)],
    },
    Spec {
        slug: "notch-phone-large",
        name: "Notch Phone Large",
        platform: Platform::Ios,
        screens: &[notch((1284.0, 2778.0), 458.0, 3.0, 160.0, (141.0, 102.0, 63.0), 486.0)],
    },
    Spec {
        slug: "notch-phone-small",
        name: "Notch Phone Small",
        platform: Platform::Ios,
        screens: &[notch((1080.0, 2340.0), 476.0, 2.88, 127.0, (144.0, 98.0, 60.0), 468.0)],
    },
    Spec { slug: "classic-phone", name: "Classic Phone", platform: Platform::Ios, screens: &[plain((750.0, 1334.0), 326.0, 2.0, false)] },
    Spec {
        slug: "punch-hole-phone-left",
        name: "Punch Hole Phone (Left)",
        platform: Platform::Android,
        screens: &[hole((1080.0, 2340.0), 420.0, 84.0, 120.0, (48.0, 36.0, 84.0, 84.0), false)],
    },
    Spec {
        slug: "punch-hole-phone-center",
        name: "Punch Hole Phone (Center)",
        platform: Platform::Android,
        screens: &[hole((1440.0, 3088.0), 500.0, 110.0, 140.0, (670.0, 40.0, 100.0, 100.0), false)],
    },
    Spec {
        slug: "punch-hole-phone-right",
        name: "Punch Hole Phone (Right)",
        platform: Platform::Android,
        screens: &[hole((1080.0, 2280.0), 420.0, 84.0, 120.0, (948.0, 36.0, 84.0, 84.0), false)],
    },
    Spec {
        slug: "pill-cutout-phone",
        name: "Pill Cutout Phone",
        platform: Platform::Android,
        screens: &[hole((1200.0, 2640.0), 450.0, 110.0, 140.0, (420.0, 40.0, 360.0, 100.0), false)],
    },
    Spec {
        slug: "large-tablet",
        name: "Large Tablet",
        platform: Platform::Ios,
        // A home indicator along the bottom whichever way it is held.
        screens: &[ScreenSpec {
            size: (2048.0, 2732.0),
            dpi: 264.0,
            scale: 2.0,
            corner: 36.0,
            portrait: (0.0, 40.0),
            landscape: (0.0, 0.0, 40.0),
            cutout: None,
            upside_down: true,
        }],
    },
    Spec { slug: "small-tablet", name: "Small Tablet", platform: Platform::Ios, screens: &[plain((1536.0, 2048.0), 264.0, 2.0, true)] },
    Spec {
        slug: "foldable-phone",
        name: "Foldable Phone",
        platform: Platform::Android,
        // Folded (the cover screen), then unfolded (the inner screen).
        screens: &[
            hole((1080.0, 2520.0), 420.0, 84.0, 120.0, (498.0, 36.0, 84.0, 84.0), false),
            hole((1840.0, 2208.0), 380.0, 60.0, 120.0, (1708.0, 36.0, 84.0, 84.0), true),
        ],
    },
];

/// A portrait rectangle of a `w × h` screen as it lies in orientation `o`.
fn turned(o: Orientation, (w, h): (f64, f64), (x, y, cw, ch): (f64, f64, f64, f64)) -> PxRect {
    match o {
        Orientation::Portrait => PxRect { x, y, width: cw, height: ch },
        Orientation::PortraitUpsideDown => PxRect { x: w - x - cw, y: h - y - ch, width: cw, height: ch },
        // The top of the device is on the left.
        Orientation::LandscapeLeft => PxRect { x: y, y: w - x - cw, width: ch, height: cw },
        // The top of the device is on the right.
        Orientation::LandscapeRight => PxRect { x: h - y - ch, y: x, width: ch, height: cw },
    }
}

fn screen(s: &ScreenSpec) -> Screen {
    let (top, bottom) = s.portrait;
    let (near, far, low) = s.landscape;
    let mut wanted = vec![Orientation::Portrait];
    if s.upside_down {
        wanted.push(Orientation::PortraitUpsideDown);
    }
    wanted.extend([Orientation::LandscapeLeft, Orientation::LandscapeRight]);
    let orientations = wanted
        .into_iter()
        .map(|o| OrientationInfo {
            orientation: o,
            safe_area: match o {
                Orientation::Portrait => Insets { top, bottom, ..Insets::default() },
                // A home indicator stays at the bottom; a cutout turns with the device.
                Orientation::PortraitUpsideDown if s.cutout.is_some() => Insets { top: bottom, bottom: top, ..Insets::default() },
                Orientation::PortraitUpsideDown => Insets { top, bottom, ..Insets::default() },
                Orientation::LandscapeLeft => Insets { left: near, right: far, bottom: low, top: 0.0 },
                Orientation::LandscapeRight => Insets { left: far, right: near, bottom: low, top: 0.0 },
            },
            cutouts: s.cutout.map(|c| turned(o, s.size, c)).into_iter().collect(),
        })
        .collect();
    Screen {
        width_px: s.size.0,
        height_px: s.size.1,
        dpi: Some(s.dpi),
        scale: s.scale,
        scale_source: ScaleSource::File,
        navigation_bar_px: 0.0,
        corner_radius_px: s.corner,
        orientations,
        overlay: None,
    }
}

/// The built-in devices, in the order the pickers list them. Each is checked like an imported
/// file; one that didn't pass would be left out.
pub fn builtin() -> Vec<DeviceProfile> {
    SPECS
        .iter()
        .filter_map(|spec| {
            let mut p = DeviceProfile {
                id: format!("{PREFIX}{}", spec.slug),
                name: spec.name.to_string(),
                platform: spec.platform,
                source: Source::BuiltIn,
                model: None,
                os: None,
                screens: spec.screens.iter().map(screen).collect(),
            };
            crate::validate(&mut p).ok().map(|()| p)
        })
        .collect()
}

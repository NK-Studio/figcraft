//! FigCraft and Unity UI Toolkit (`docs/unity-uitk.md`): the Unity versions a design targets
//! ([`UnityVersion`], [`Feature`]) and the compatibility check ([`check`]) that reports, layer by
//! layer, what the target version's UXML/USS can't represent exactly.

mod check;
mod layer;
#[cfg(test)]
mod tests;

pub use check::{CheckOptions, Issue, Severity, check};
pub use layer::{LayerInfo, Position, file_options, implied_pivot, layer_info};

use serde::{Serialize, Serializer};

/// A Unity 6 release (6.0, 6.1, …): the oldest supported is 6.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnityVersion {
    pub minor: u8,
}

impl UnityVersion {
    /// The oldest version supported, and the default target.
    pub const MIN: UnityVersion = UnityVersion { minor: 0 };
    /// The newest version the feature table knows (later ones are checked as this one).
    pub const LATEST_KNOWN: UnityVersion = UnityVersion { minor: 7 };

    /// The versions from 6.0 to [`Self::LATEST_KNOWN`], for pickers.
    pub fn known() -> impl Iterator<Item = UnityVersion> {
        (Self::MIN.minor..=Self::LATEST_KNOWN.minor).map(Self::new)
    }

    pub const fn new(minor: u8) -> Self {
        Self { minor }
    }

    /// A version from "6.3", "6", "6000.3", "6000.3.0f1" or "Unity 6.3"; versions before 6.0 and
    /// unreadable text are `None`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.strip_prefix("Unity").or_else(|| s.strip_prefix("unity")).unwrap_or(s).trim();
        let mut parts = s.split('.');
        let major = parts.next()?.trim();
        let minor = parts.next().map(str::trim);
        let digits = |p: &str| !p.is_empty() && p.len() <= 4 && p.bytes().all(|b| b.is_ascii_digit());
        if !digits(major) {
            return None;
        }
        // "6000.3.0f1": Unity's long form; the patch part isn't needed.
        if major != "6" && major != "6000" {
            return None;
        }
        let minor = match minor {
            None => 0,
            Some(m) if digits(m) => m.parse::<u8>().ok().filter(|m| *m <= 99)?,
            Some(_) => return None,
        };
        Some(Self { minor })
    }

    /// Whether this version has `f` natively.
    pub fn has(self, f: Feature) -> bool {
        self >= f.since()
    }
}

impl std::fmt::Display for UnityVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "6.{}", self.minor)
    }
}

impl Serialize for UnityVersion {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// UITK features that came after 6.0 (the table in `docs/unity-uitk.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    /// `filter` (blur, …).
    Filter,
    AspectRatio,
    /// SVG as a core module (Vector Images).
    VectorImage,
    /// `filter: drop-shadow()`.
    DropShadow,
    BackdropFilter,
    /// `gap` between flex children.
    Gap,
    /// `linear-gradient()` / `radial-gradient()`.
    Gradient,
    ZIndex,
    /// CSS Grid (experimental: behind a project setting).
    Grid,
}

impl Feature {
    /// The first version with the feature.
    pub fn since(self) -> UnityVersion {
        match self {
            Self::Filter | Self::AspectRatio | Self::VectorImage => UnityVersion::new(3),
            Self::DropShadow | Self::BackdropFilter => UnityVersion::new(6),
            Self::Gap | Self::Gradient | Self::ZIndex | Self::Grid => UnityVersion::new(7),
        }
    }
}

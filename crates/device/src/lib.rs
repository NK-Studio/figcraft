//! Device profiles: a device's screens with their size, scale (pixels per point), safe area and
//! display cutouts in each orientation it supports (see `docs/device-profiles.md`).
//!
//! Profiles are read from Unity Device Simulator `.device` files the user has ([`unity`]) or from
//! FigCraft's own `figcraft-device` JSON ([`native`]); [`parse`] tells them apart. Pixels are stored
//! with a top-left origin and points derived from them. Nothing here touches the file system or
//! panics: every input is validated ([`validate`]) and refused with a [`DeviceError`].

pub mod native;
pub mod unity;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

/// The largest device file read.
pub const MAX_FILE_BYTES: u64 = 1 << 20;
/// Screens per device (foldables have two).
pub const MAX_SCREENS: usize = 8;
/// Orientations per screen.
pub const MAX_ORIENTATIONS: usize = 8;
/// Cutouts per orientation.
pub const MAX_CUTOUTS: usize = 64;
/// The longest screen side in pixels.
pub const MAX_SIDE_PX: f64 = 32768.0;
/// Profiles a [`DeviceRegistry`] holds.
pub const MAX_PROFILES: usize = 1024;
/// How far (px) a safe area or cutout may stick out of the screen before it is refused.
const TOLERANCE_PX: f64 = 1.0;
const MAX_NAME_CHARS: usize = 200;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DeviceError {
    #[error("not a device file: {0}")]
    Format(String),
    #[error("invalid device: {0}")]
    Invalid(String),
    #[error("device file larger than {MAX_FILE_BYTES} bytes")]
    TooLarge,
    #[error("too many devices (at most {MAX_PROFILES})")]
    Full,
}

pub type Result<T> = std::result::Result<T, DeviceError>;

fn invalid(msg: impl Into<String>) -> DeviceError {
    DeviceError::Invalid(msg.into())
}

/// A screen orientation (Unity's `ScreenOrientation` 1–4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    Portrait,
    PortraitUpsideDown,
    LandscapeLeft,
    LandscapeRight,
}

impl Orientation {
    pub const ALL: [Orientation; 4] = [Self::Portrait, Self::PortraitUpsideDown, Self::LandscapeLeft, Self::LandscapeRight];

    pub fn is_landscape(self) -> bool {
        matches!(self, Self::LandscapeLeft | Self::LandscapeRight)
    }

    /// The orientation of Unity's `ScreenOrientation` value.
    pub fn from_unity(v: i64) -> Option<Self> {
        match v {
            1 => Some(Self::Portrait),
            2 => Some(Self::PortraitUpsideDown),
            3 => Some(Self::LandscapeLeft),
            4 => Some(Self::LandscapeRight),
            _ => None,
        }
    }

    pub fn unity(self) -> u8 {
        match self {
            Self::Portrait => 1,
            Self::PortraitUpsideDown => 2,
            Self::LandscapeLeft => 3,
            Self::LandscapeRight => 4,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Portrait => "portrait",
            Self::PortraitUpsideDown => "portraitUpsideDown",
            Self::LandscapeLeft => "landscapeLeft",
            Self::LandscapeRight => "landscapeRight",
        }
    }

    /// An orientation from a name ("portrait", "landscape-left", "upsideDown", "landscape" for
    /// landscape left, …; case and separators don't matter) or Unity's number "1"–"4".
    pub fn parse(s: &str) -> Option<Self> {
        let key: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        match key.as_str() {
            "portrait" | "1" => Some(Self::Portrait),
            "portraitupsidedown" | "upsidedown" | "2" => Some(Self::PortraitUpsideDown),
            "landscapeleft" | "landscape" | "3" => Some(Self::LandscapeLeft),
            "landscaperight" | "4" => Some(Self::LandscapeRight),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Ios,
    Android,
    #[default]
    Other,
}

impl Platform {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ios => "ios",
            Self::Android => "android",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ios" | "ipados" => Some(Self::Ios),
            "android" => Some(Self::Android),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

/// Where a screen's scale came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ScaleSource {
    /// Written in a `figcraft-device` file.
    #[default]
    File,
    /// Given by the caller of the import.
    Override,
    InferredIos,
    InferredAndroid,
    /// Nothing to go on: 1.
    Default,
}

impl ScaleSource {
    pub fn name(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Override => "override",
            Self::InferredIos => "inferred-ios",
            Self::InferredAndroid => "inferred-android",
            Self::Default => "default",
        }
    }
}

/// Distances in from each edge.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Insets {
    pub fn scaled(self, k: f64) -> Self {
        Self { top: self.top * k, right: self.right * k, bottom: self.bottom * k, left: self.left * k }
    }

    fn finite(&self) -> bool {
        [self.top, self.right, self.bottom, self.left].iter().all(|v| v.is_finite())
    }
}

/// A rectangle with a top-left origin.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PxRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl PxRect {
    pub fn scaled(self, k: f64) -> Self {
        Self { x: self.x * k, y: self.y * k, width: self.width * k, height: self.height * k }
    }

    fn finite(&self) -> bool {
        [self.x, self.y, self.width, self.height].iter().all(|v| v.is_finite())
    }
}

/// A picture of the device drawn round the screen: a reference to the user's own file only
/// (never bundled; see the doc's licensing section).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overlay {
    pub path: String,
    #[serde(default)]
    pub border_px: Insets,
}

/// One orientation a screen supports.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrientationInfo {
    pub orientation: Orientation,
    /// Pixels in from each edge of the screen in this orientation.
    #[serde(default)]
    pub safe_area: Insets,
    /// Display cutouts in pixels, top-left origin, in this orientation.
    #[serde(default)]
    pub cutouts: Vec<PxRect>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    /// Portrait (natural) width and height in pixels.
    pub width_px: f64,
    pub height_px: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi: Option<f64>,
    /// Pixels per point.
    pub scale: f64,
    #[serde(default)]
    pub scale_source: ScaleSource,
    /// Android's navigation bar (Unity's `navigationBarHeight`), pixels.
    #[serde(default)]
    pub navigation_bar_px: f64,
    #[serde(default)]
    pub corner_radius_px: f64,
    #[serde(default)]
    pub orientations: Vec<OrientationInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<Overlay>,
}

impl Screen {
    /// The screen's width and height in pixels in orientation `o` (landscape swaps them).
    pub fn size_px(&self, o: Orientation) -> (f64, f64) {
        if o.is_landscape() { (self.height_px, self.width_px) } else { (self.width_px, self.height_px) }
    }

    pub fn size_pt(&self, o: Orientation) -> (f64, f64) {
        let (w, h) = self.size_px(o);
        (w / self.scale, h / self.scale)
    }

    pub fn info(&self, o: Orientation) -> Option<&OrientationInfo> {
        self.orientations.iter().find(|i| i.orientation == o)
    }

    /// The orientations supported, in the order listed.
    pub fn supported(&self) -> Vec<Orientation> {
        self.orientations.iter().map(|i| i.orientation).collect()
    }

    pub fn insets_px(&self, o: Orientation) -> Option<Insets> {
        self.info(o).map(|i| i.safe_area)
    }

    pub fn insets_pt(&self, o: Orientation) -> Option<Insets> {
        self.insets_px(o).map(|i| i.scaled(1.0 / self.scale))
    }

    /// The safe area as a rectangle in pixels (top-left origin).
    pub fn safe_area_px(&self, o: Orientation) -> Option<PxRect> {
        let i = self.insets_px(o)?;
        let (w, h) = self.size_px(o);
        Some(PxRect { x: i.left, y: i.top, width: (w - i.left - i.right).max(0.0), height: (h - i.top - i.bottom).max(0.0) })
    }

    pub fn safe_area_pt(&self, o: Orientation) -> Option<PxRect> {
        self.safe_area_px(o).map(|r| r.scaled(1.0 / self.scale))
    }

    pub fn cutouts_pt(&self, o: Orientation) -> Vec<PxRect> {
        self.info(o).map(|i| i.cutouts.iter().map(|c| c.scaled(1.0 / self.scale)).collect()).unwrap_or_default()
    }
}

/// Where a profile was read from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Source {
    /// A Unity Device Simulator `.device` file.
    UnityDevice {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<String>,
    },
    /// A `figcraft-device` file.
    FigCraft {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<String>,
    },
    /// Given as text, not a file.
    #[default]
    Inline,
}

impl Source {
    pub fn name(&self) -> &'static str {
        match self {
            Self::UnityDevice { .. } => "unity",
            Self::FigCraft { .. } => "figcraft",
            Self::Inline => "inline",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfile {
    /// Unique in a registry: slug segments joined by `/` ("unity/apple-iphone-12").
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub platform: Platform,
    #[serde(default)]
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    pub screens: Vec<Screen>,
}

impl DeviceProfile {
    pub fn screen(&self, i: usize) -> Option<&Screen> {
        self.screens.get(i)
    }
}

/// What the caller knows about the bytes being parsed.
#[derive(Clone, Debug, Default)]
pub struct ParseHint {
    /// The file the bytes came from (recorded in [`Source`]).
    pub file: Option<String>,
    /// The folder relative overlay paths are resolved against.
    pub dir: Option<String>,
    /// Pixels per point for every screen, instead of what the file says or the importer infers.
    pub scale: Option<f64>,
}

/// A device profile from a Unity `.device` file or a `figcraft-device` file, validated.
pub fn parse(bytes: &[u8], hint: &ParseHint) -> Result<DeviceProfile> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(DeviceError::TooLarge);
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| DeviceError::Format(e.to_string()))?;
    if let Some(s) = hint.scale
        && !(s.is_finite() && (0.1..=10.0).contains(&s))
    {
        return Err(invalid("scale must be between 0.1 and 10"));
    }
    let mut p = if v.get("format").and_then(|f| f.as_str()) == Some(native::FORMAT) {
        native::from_value(v, hint)?
    } else if v.get("friendlyName").is_some() && v.get("screens").is_some() {
        unity::from_value(v, hint)?
    } else {
        return Err(DeviceError::Format("expected a Unity `.device` file or a `figcraft-device` file".into()));
    };
    validate(&mut p)?;
    Ok(p)
}

/// A slug: lowercase ASCII letters and digits with single `-` between words, at most 64
/// characters; `device` when nothing is left.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 64 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "device".into() } else { out }
}

/// An id made of slug segments joined by `/` (at most 4 segments).
pub fn clean_id(s: &str) -> String {
    // Segments with nothing to keep (`..`, blanks) are dropped, not turned into `device`.
    let parts: Vec<String> = s.split('/').filter(|p| p.chars().any(|c| c.is_ascii_alphanumeric())).take(4).map(slug).collect();
    if parts.is_empty() { "device".into() } else { parts.join("/") }
}

fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Check every number and size, clamp safe areas and cutouts into the screen (within a 1 px
/// tolerance), and tidy the id and name.
pub fn validate(p: &mut DeviceProfile) -> Result<()> {
    p.id = clean_id(&p.id);
    p.name = truncate_chars(p.name.trim(), MAX_NAME_CHARS);
    if p.name.is_empty() {
        p.name = p.id.clone();
    }
    p.model = p.model.as_deref().map(|m| truncate_chars(m, MAX_NAME_CHARS));
    p.os = p.os.as_deref().map(|m| truncate_chars(m, MAX_NAME_CHARS));
    if p.screens.is_empty() {
        return Err(invalid("no screens"));
    }
    if p.screens.len() > MAX_SCREENS {
        return Err(invalid(format!("more than {MAX_SCREENS} screens")));
    }
    for (si, s) in p.screens.iter_mut().enumerate() {
        validate_screen(s).map_err(|e| match e {
            DeviceError::Invalid(m) => invalid(format!("screen {si}: {m}")),
            other => other,
        })?;
    }
    Ok(())
}

fn validate_screen(s: &mut Screen) -> Result<()> {
    for (what, v) in [("width", s.width_px), ("height", s.height_px)] {
        if !(v.is_finite() && (1.0..=MAX_SIDE_PX).contains(&v)) {
            return Err(invalid(format!("{what} must be 1 to {MAX_SIDE_PX} pixels")));
        }
    }
    s.width_px = s.width_px.round();
    s.height_px = s.height_px.round();
    if !(s.scale.is_finite() && (0.1..=10.0).contains(&s.scale)) {
        return Err(invalid("scale must be between 0.1 and 10"));
    }
    if let Some(d) = s.dpi
        && !(d.is_finite() && d >= 0.0)
    {
        s.dpi = None;
    }
    if !s.navigation_bar_px.is_finite() || s.navigation_bar_px < 0.0 {
        return Err(invalid("navigation bar must be a positive number"));
    }
    s.navigation_bar_px = s.navigation_bar_px.min(s.width_px.max(s.height_px));
    if !s.corner_radius_px.is_finite() || s.corner_radius_px < 0.0 {
        return Err(invalid("corner radius must be a positive number"));
    }
    s.corner_radius_px = s.corner_radius_px.min(s.width_px.min(s.height_px) / 2.0);
    if s.orientations.len() > MAX_ORIENTATIONS {
        return Err(invalid(format!("more than {MAX_ORIENTATIONS} orientations")));
    }
    if let Some(o) = &mut s.overlay {
        if !o.border_px.finite() {
            return Err(invalid("overlay border must be finite"));
        }
        o.border_px = clamp_insets(o.border_px, MAX_SIDE_PX);
        o.path = truncate_chars(&o.path, 4096);
    }
    let mut seen = Vec::new();
    let (wp, hp) = (s.width_px, s.height_px);
    for info in &mut s.orientations {
        let o = info.orientation;
        if seen.contains(&o) {
            return Err(invalid(format!("orientation {} listed twice", o.name())));
        }
        seen.push(o);
        let (w, h) = if o.is_landscape() { (hp, wp) } else { (wp, hp) };
        let i = info.safe_area;
        if !i.finite() || [i.top, i.right, i.bottom, i.left].iter().any(|v| *v < -TOLERANCE_PX) {
            return Err(invalid(format!("{}: safe area outside the screen", o.name())));
        }
        if i.left + i.right > w + TOLERANCE_PX || i.top + i.bottom > h + TOLERANCE_PX {
            return Err(invalid(format!("{}: safe area larger than the screen", o.name())));
        }
        let mut i = clamp_insets(i, f64::MAX);
        i.right = i.right.min((w - i.left).max(0.0));
        i.bottom = i.bottom.min((h - i.top).max(0.0));
        info.safe_area = i;
        if info.cutouts.len() > MAX_CUTOUTS {
            return Err(invalid(format!("{}: more than {MAX_CUTOUTS} cutouts", o.name())));
        }
        for c in &mut info.cutouts {
            let fits = c.finite()
                && c.width >= 0.0
                && c.height >= 0.0
                && c.x >= -TOLERANCE_PX
                && c.y >= -TOLERANCE_PX
                && c.x + c.width <= w + TOLERANCE_PX
                && c.y + c.height <= h + TOLERANCE_PX;
            if !fits {
                return Err(invalid(format!("{}: cutout outside the screen", o.name())));
            }
            c.x = c.x.clamp(0.0, w);
            c.y = c.y.clamp(0.0, h);
            c.width = c.width.min(w - c.x);
            c.height = c.height.min(h - c.y);
        }
    }
    Ok(())
}

fn clamp_insets(i: Insets, max: f64) -> Insets {
    Insets { top: i.top.clamp(0.0, max), right: i.right.clamp(0.0, max), bottom: i.bottom.clamp(0.0, max), left: i.left.clamp(0.0, max) }
}

/// The devices known to a session, in the order they were added.
#[derive(Clone, Debug, Default)]
pub struct DeviceRegistry {
    profiles: Vec<DeviceProfile>,
}

impl DeviceRegistry {
    /// Add `p`, replacing the profile with the same id when `replace` (otherwise that is an
    /// error). Returns whether one was replaced.
    pub fn insert(&mut self, p: DeviceProfile, replace: bool) -> Result<bool> {
        if let Some(slot) = self.profiles.iter_mut().find(|q| q.id == p.id) {
            if !replace {
                return Err(invalid(format!("`{}` is already imported", p.id)));
            }
            *slot = p;
            return Ok(true);
        }
        if self.profiles.len() >= MAX_PROFILES {
            return Err(DeviceError::Full);
        }
        self.profiles.push(p);
        Ok(false)
    }

    pub fn get(&self, id: &str) -> Option<&DeviceProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let n = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        self.profiles.len() != n
    }

    pub fn clear(&mut self) -> usize {
        let n = self.profiles.len();
        self.profiles.clear();
        n
    }

    pub fn iter(&self) -> impl Iterator<Item = &DeviceProfile> {
        self.profiles.iter()
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

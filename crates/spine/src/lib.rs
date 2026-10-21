//! FigCraft and Spine skeletons (`docs/spine.md`): load a Spine 4.3 export (skeleton JSON or binary
//! `.skel`, plus its `.atlas`) as an [`Asset`], list its animations and skins ([`Info`]) and pose it
//! at a time into textured triangles ([`Frame`]) for the renderer to draw.
//!
//! Coordinates are FigCraft's: y grows down (Spine's y-up is flipped here), units are skeleton
//! units. UVs are normalised to the atlas page, v down.
//!
//! The runtime is the `spine2d` crate behind the `runtime` feature. Nothing of it is exposed, so it
//! can be swapped. Every call into it is guarded: a panic inside it becomes
//! [`SpineError::Internal`].

pub mod fixture;
mod host;
#[cfg(feature = "runtime")]
mod runtime;
#[cfg(test)]
mod tests;

use serde::Serialize;

/// The largest skeleton file accepted.
pub const MAX_SKELETON_BYTES: usize = 64 << 20;
/// The largest atlas text accepted.
pub const MAX_ATLAS_BYTES: usize = 4 << 20;
/// Times past this (seconds) are clamped.
pub const MAX_TIME: f32 = 1.0e6;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SpineError {
    #[error("invalid Spine data: {0}")]
    Invalid(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("Spine runtime failed: {0}")]
    Internal(String),
}

/// How a batch blends with what is under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Blend {
    #[default]
    Normal,
    Additive,
    Multiply,
    Screen,
}

/// Triangles drawn from one atlas page with one blend mode.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Batch {
    /// The atlas page's image file name, as the atlas names it (see [`Info::pages`]).
    pub page: String,
    pub blend: Blend,
    /// The page's colours are premultiplied by alpha.
    pub premultiplied: bool,
    pub positions: Vec<[f32; 2]>,
    pub uvs: Vec<[f32; 2]>,
    /// Per-vertex tint (RGBA, 0..1).
    pub colors: Vec<[f32; 4]>,
    /// Triangle list into the vertices above.
    pub indices: Vec<u32>,
}

/// A posed skeleton.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub batches: Vec<Batch>,
}

impl Frame {
    /// The bounding box of every vertex (min x, min y, max x, max y), or `None` when empty.
    pub fn bounds(&self) -> Option<[f32; 4]> {
        let mut b: Option<[f32; 4]> = None;
        for p in self.batches.iter().flat_map(|b| b.positions.iter()) {
            if !(p[0].is_finite() && p[1].is_finite()) {
                continue;
            }
            b = Some(match b {
                None => [p[0], p[1], p[0], p[1]],
                Some(r) => [r[0].min(p[0]), r[1].min(p[1]), r[2].max(p[0]), r[3].max(p[1])],
            });
        }
        b
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AnimationInfo {
    pub name: String,
    /// Seconds.
    pub duration: f32,
}

/// What a skeleton holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Info {
    /// The Spine version that exported it.
    pub version: Option<String>,
    pub animations: Vec<AnimationInfo>,
    pub skins: Vec<String>,
    /// The atlas page image file names, in atlas order.
    pub pages: Vec<String>,
    /// The setup pose's bounds (min x, min y, max x, max y), y down.
    pub setup_bounds: Option<[f32; 4]>,
}

impl Info {
    pub fn animation(&self, name: &str) -> Option<&AnimationInfo> {
        self.animations.iter().find(|a| a.name == name)
    }
}

/// Which pose to evaluate.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose<'a> {
    /// `None`: the setup pose.
    pub animation: Option<&'a str>,
    /// `None`: the skeleton's default skin.
    pub skin: Option<&'a str>,
    /// Seconds into the animation.
    pub time: f32,
    /// Wrap past the animation's end (otherwise hold the last frame).
    pub looping: bool,
}

/// Which runtime poses a skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Runtime {
    /// The official spine-cpp runtime of the file's version in a host process when one is
    /// installed; otherwise the built-in runtime (4.3 only).
    #[default]
    Auto,
    /// The built-in runtime (`spine2d`, Spine 4.3), in this process.
    BuiltIn,
    /// The official spine-cpp runtime in a host process (Spine 4.2 or 4.3).
    Host,
}

/// The Spine versions a host can be built for.
pub const HOST_VERSIONS: [&str; 2] = ["4.2", "4.3"];

enum Backend {
    #[cfg(feature = "runtime")]
    BuiltIn(runtime::Loaded),
    Host(host::Hosted),
}

/// The last pose asked for: animation, skin, time bits, looping.
type PoseKey = (Option<String>, Option<String>, u32, bool);

/// A loaded skeleton and atlas.
pub struct Asset {
    backend: Backend,
    info: Info,
    /// The last pose and its frame: a canvas draws a layer once per tile, and a host answers
    /// over a pipe.
    last: std::sync::Mutex<Option<(PoseKey, Frame)>>,
}

impl std::fmt::Debug for Asset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Asset").field("runtime", &self.runtime_name()).field("info", &self.info).finish_non_exhaustive()
    }
}

impl Asset {
    /// Loads a skeleton (`binary`: a `.skel` file, else JSON) and its atlas text with
    /// [`Runtime::Auto`].
    pub fn load(skeleton: &[u8], binary: bool, atlas: &str) -> Result<Asset, SpineError> {
        Asset::load_with(skeleton, binary, atlas, Runtime::Auto)
    }

    /// Loads a skeleton with the runtime `runtime`.
    pub fn load_with(skeleton: &[u8], binary: bool, atlas: &str, runtime: Runtime) -> Result<Asset, SpineError> {
        if skeleton.len() > MAX_SKELETON_BYTES {
            return Err(SpineError::Invalid("skeleton file is too large".into()));
        }
        if atlas.len() > MAX_ATLAS_BYTES {
            return Err(SpineError::Invalid("atlas file is too large".into()));
        }
        let version = skeleton_version(skeleton, binary);
        let minor = version.as_deref().and_then(|v| HOST_VERSIONS.iter().find(|h| v.starts_with(*h))).copied();
        let host = minor.and_then(host::host_path);
        let use_host = match runtime {
            Runtime::Host => true,
            Runtime::BuiltIn => false,
            Runtime::Auto => host.is_some() || minor != Some("4.3"),
        };
        if use_host {
            let Some(minor) = minor else {
                return Err(SpineError::Invalid(format!(
                    "Spine {} isn't supported (export it for Spine 4.2 or 4.3)",
                    version.as_deref().unwrap_or("of an unknown version")
                )));
            };
            let exe = host.ok_or_else(|| {
                SpineError::Unavailable(format!(
                    "Spine {minor} skeletons need the Spine host (figcraft-spine-host-{minor}; `cargo xtask spine-host`)"
                ))
            })?;
            let (h, info) = host::Hosted::load(exe, skeleton, binary, atlas)?;
            return Ok(Asset { backend: Backend::Host(h), info, last: Default::default() });
        }
        #[cfg(feature = "runtime")]
        {
            let inner = runtime::Loaded::load(skeleton, binary, atlas)?;
            let info = inner.info()?;
            Ok(Asset { backend: Backend::BuiltIn(inner), info, last: Default::default() })
        }
        #[cfg(not(feature = "runtime"))]
        {
            Err(SpineError::Unavailable("this build has no built-in Spine runtime".into()))
        }
    }

    pub fn info(&self) -> &Info {
        &self.info
    }

    /// The runtime posing it (`"spine-cpp"` in a host, or `"spine2d"`).
    pub fn runtime_name(&self) -> &'static str {
        match &self.backend {
            #[cfg(feature = "runtime")]
            Backend::BuiltIn(_) => "spine2d",
            Backend::Host(_) => "spine-cpp",
        }
    }

    /// Poses the skeleton. An unknown animation or skin is an error.
    pub fn pose(&self, pose: Pose<'_>) -> Result<Frame, SpineError> {
        if let Some(a) = pose.animation
            && self.info.animation(a).is_none()
        {
            return Err(SpineError::Invalid(format!("no animation named {a:?}")));
        }
        if let Some(s) = pose.skin
            && !self.info.skins.iter().any(|k| k == s)
        {
            return Err(SpineError::Invalid(format!("no skin named {s:?}")));
        }
        let time = if pose.time.is_finite() { pose.time.clamp(0.0, MAX_TIME) } else { 0.0 };
        let pose = Pose { time, ..pose };
        let key: PoseKey = (pose.animation.map(str::to_string), pose.skin.map(str::to_string), time.to_bits(), pose.looping);
        let mut last = self.last.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((k, frame)) = last.as_ref()
            && *k == key
        {
            return Ok(frame.clone());
        }
        let frame = match &self.backend {
            #[cfg(feature = "runtime")]
            Backend::BuiltIn(inner) => inner.pose(pose),
            Backend::Host(h) => h.pose(pose),
        }?;
        *last = Some((key, frame.clone()));
        Ok(frame)
    }
}

/// The Spine version that exported a skeleton (`"4.2.43"`), read from its header: the
/// `skeleton.spine` field of JSON, or the string after the 8-byte hash of a binary `.skel`.
pub fn skeleton_version(skeleton: &[u8], binary: bool) -> Option<String> {
    if binary {
        // A varint: the string's length plus one (0: none).
        let mut len = 0usize;
        let mut at = 8;
        for shift in [0, 7, 14, 21] {
            let b = *skeleton.get(at)?;
            at += 1;
            len |= usize::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                break;
            }
        }
        let len = len.checked_sub(1).filter(|l| *l <= 32)?;
        let v = std::str::from_utf8(skeleton.get(at..at + len)?).ok()?;
        return Some(v.to_string());
    }
    #[derive(serde::Deserialize)]
    struct Head {
        skeleton: Option<Skel>,
    }
    #[derive(serde::Deserialize)]
    struct Skel {
        spine: Option<String>,
    }
    serde_json::from_slice::<Head>(skeleton).ok()?.skeleton?.spine
}

/// Whether JSON text looks like a Spine skeleton export (a top-level `skeleton` object next to
/// `bones`), as opposed to a FigCraft or Figma REST document.
pub fn is_skeleton_json(bytes: &[u8]) -> bool {
    #[derive(serde::Deserialize)]
    struct Probe {
        skeleton: Option<serde::de::IgnoredAny>,
        bones: Option<serde::de::IgnoredAny>,
    }
    let Ok(text) = std::str::from_utf8(bytes) else { return false };
    let head = text.trim_start();
    if !head.starts_with('{') || !head.contains("\"skeleton\"") {
        return false;
    }
    matches!(serde_json::from_str::<Probe>(text), Ok(Probe { skeleton: Some(_), bones: Some(_) }))
}

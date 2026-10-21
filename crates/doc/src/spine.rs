//! Spine skeletons (`docs/spine.md`): a [`SpineData`] node shows one [`SpineAsset`] posed at an
//! animation and time. The skeleton files themselves are kept once per file in
//! [`Document::spine`](crate::Document::spine); the atlas page images go to the image store.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A Spine export kept in the document: the skeleton file, its atlas and the atlas pages.
#[derive(Clone, Debug, PartialEq)]
pub struct SpineAsset {
    /// The skeleton's file name without extension (`hero` for `hero.json`).
    pub name: String,
    /// The skeleton file: JSON, or a binary `.skel` when `binary`.
    pub skeleton: Arc<Vec<u8>>,
    pub binary: bool,
    /// The `.atlas` text.
    pub atlas: String,
    /// Atlas page file name (as the atlas names it) → image hash in the image store.
    pub pages: BTreeMap<String, String>,
}

/// A Spine node's content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpineData {
    /// The key of the skeleton in [`Document::spine`](crate::Document::spine).
    pub asset: String,
    /// The animation shown; `None`: the setup pose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<String>,
    /// The skin; `None`: the skeleton's default skin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,
    /// Seconds into the animation shown while not playing (and where playback starts).
    #[serde(default)]
    pub time: f64,
    #[serde(default = "yes")]
    pub looping: bool,
    /// Plays the animation (in presentation and on the canvas).
    #[serde(default)]
    pub playing: bool,
    /// Playback speed (1 = as exported).
    #[serde(default = "one")]
    pub speed: f64,
    /// The skeleton-space rectangle (x, y, width, height; y down) mapped onto the node's box: the
    /// setup pose's bounds when placed.
    pub frame: [f64; 4],
}

fn yes() -> bool {
    true
}
fn one() -> f64 {
    1.0
}

impl SpineData {
    /// The animation time at `clock` seconds of playback.
    pub fn time_at(&self, clock: f64) -> f64 {
        let speed = if self.speed.is_finite() { self.speed } else { 1.0 };
        let t = if self.playing && clock.is_finite() { self.time + clock * speed } else { self.time };
        if t.is_finite() { t.max(0.0) } else { 0.0 }
    }
}

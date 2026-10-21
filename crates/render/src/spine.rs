//! Spine skeleton nodes (`docs/spine.md`): posed by `figcraft-spine` at the node's animation and
//! time, drawn as textured triangles from the atlas pages in the image store.
//!
//! Each triangle is filled with its page as a pattern under the affine map that takes its UVs to
//! its corners; neighbours under the same map fill as one path, and triangles grow by half a
//! device pixel so anti-aliased edges between the rest don't leave seams. Vertex tint is applied as opacity only (no RGB tint yet).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use figcraft_color::ImageFilters;
use figcraft_doc::{Document, Node, SpineData};
use figcraft_geom::{Affine, Point};
use figcraft_spine::{Asset, Blend, Pose};
use tiny_skia::{BlendMode, FillRule, FilterQuality, Paint, PathBuilder, Pixmap, SpreadMode, Transform};

/// Loaded skeletons by key (`None`: it failed to load). Cleared when it holds more than this.
const MAX_CACHED: usize = 64;

fn cache() -> &'static Mutex<HashMap<String, Option<Arc<Asset>>>> {
    static C: OnceLock<Mutex<HashMap<String, Option<Arc<Asset>>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The loaded skeleton `key` of `doc`, cached (`None` when it's missing or doesn't load).
pub fn asset(doc: &Document, key: &str) -> Option<Arc<Asset>> {
    let data = doc.spine.get(key)?;
    if let Some(a) = cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(key) {
        return a.clone();
    }
    let loaded = match Asset::load(&data.skeleton, data.binary, &data.atlas) {
        Ok(a) => Some(Arc::new(a)),
        Err(e) => {
            log::warn!("Spine skeleton {}: {e}", data.name);
            None
        }
    };
    let mut c = cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if c.len() >= MAX_CACHED {
        c.clear();
    }
    c.insert(key.to_string(), loaded.clone());
    loaded
}

/// The pose `s` shows `clock` seconds into playback.
pub fn pose_of(s: &SpineData, clock: f64) -> Pose<'_> {
    Pose { animation: s.animation.as_deref(), skin: s.skin.as_deref(), time: s.time_at(clock) as f32, looping: s.looping }
}

/// The map from skeleton space onto the node's box.
pub fn skeleton_to_local(n: &Node, s: &SpineData) -> Option<Affine> {
    let [x, y, w, h] = s.frame;
    if !(w.is_finite() && h.is_finite() && x.is_finite() && y.is_finite()) || w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some(Affine::scale_non_uniform(n.width / w, n.height / h) * Affine::translate((-x, -y)))
}

/// Draws Spine node `n` under `xf` (local → device) `clock` seconds into playback.
pub(crate) fn draw(doc: &Document, pix: &mut Pixmap, n: &Node, s: &SpineData, xf: Affine, clock: f64) {
    let Some(asset) = asset(doc, &s.asset) else { return };
    let Some(local) = skeleton_to_local(n, s) else { return };
    let frame = match asset.pose(pose_of(s, clock)) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("Spine node {}: {e}", n.name);
            return;
        }
    };
    let Some(data) = doc.spine.get(&s.asset) else { return };
    let to_device = xf * local;
    for b in &frame.batches {
        let Some(hash) = data.pages.get(&b.page) else { continue };
        let Some(page) = crate::images::get(hash, doc.images.get(hash), &ImageFilters::default(), doc.color_profile) else { continue };
        let blend = match b.blend {
            Blend::Normal => BlendMode::SourceOver,
            Blend::Additive => BlendMode::Plus,
            Blend::Multiply => BlendMode::Multiply,
            Blend::Screen => BlendMode::Screen,
        };
        let (pw, ph) = (f64::from(page.width()), f64::from(page.height()));
        // Neighbouring triangles under the same UV map (a region's quad, flat mesh parts) fill as
        // one path, so their shared edges leave no seam.
        let mut run: Option<Run> = None;
        for tri in b.indices.chunks_exact(3) {
            let corner = |i: u32| -> Option<(Point, Point, f32)> {
                let p = b.positions.get(i as usize)?;
                let uv = b.uvs.get(i as usize)?;
                let alpha = b.colors.get(i as usize).map_or(1.0, |c| c[3]);
                Some((to_device * Point::new(f64::from(p[0]), f64::from(p[1])), Point::new(f64::from(uv[0]) * pw, f64::from(uv[1]) * ph), alpha))
            };
            let (Some(a), Some(c1), Some(c2)) = (corner(tri[0]), corner(tri[1]), corner(tri[2])) else { continue };
            let opacity = ((a.2 + c1.2 + c2.2) / 3.0).clamp(0.0, 1.0);
            let d = [a.0, c1.0, c2.0];
            let Some(m) = texel_to_device([a.1, c1.1, c2.1], d) else { continue };
            if opacity <= 0.0 {
                continue;
            }
            match &mut run {
                Some(r) if r.matches(m, opacity) => r.tris.push(d),
                _ => {
                    if let Some(r) = run.take() {
                        r.fill(pix, &page, blend);
                    }
                    run = Some(Run { map: m, opacity, tris: vec![d] });
                }
            }
        }
        if let Some(r) = run {
            r.fill(pix, &page, blend);
        }
    }
}

/// Triangles drawn with one UV map and opacity.
struct Run {
    map: Affine,
    opacity: f32,
    tris: Vec<[Point; 3]>,
}

impl Run {
    fn matches(&self, m: Affine, opacity: f32) -> bool {
        (self.opacity - opacity).abs() < 1e-4 && self.map.as_coeffs().iter().zip(m.as_coeffs()).all(|(a, b)| (a - b).abs() <= 1e-4 * (1.0 + a.abs()))
    }

    /// Fills the triangles, each grown half a device pixel from its centroid so they also overlap
    /// the anti-aliased edges of neighbours in other runs. Every triangle is wound the same way so
    /// overlaps add up instead of cancelling.
    fn fill(&self, pix: &mut Pixmap, page: &Pixmap, blend: BlendMode) {
        let mut pb = PathBuilder::new();
        for d in &self.tris {
            let centre = Point::new((d[0].x + d[1].x + d[2].x) / 3.0, (d[0].y + d[1].y + d[2].y) / 3.0);
            let grow = |p: Point| {
                let v = p - centre;
                let len = v.hypot();
                if len > 1e-6 { p + v * (0.5 / len) } else { p }
            };
            let [a, mut b, mut c] = d.map(grow);
            if (b - a).cross(c - a) < 0.0 {
                std::mem::swap(&mut b, &mut c);
            }
            pb.move_to(a.x as f32, a.y as f32);
            pb.line_to(b.x as f32, b.y as f32);
            pb.line_to(c.x as f32, c.y as f32);
            pb.close();
        }
        let Some(path) = pb.finish() else { return };
        let c = self.map.as_coeffs();
        let transform = Transform::from_row(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32, c[4] as f32, c[5] as f32);
        let paint = Paint {
            shader: tiny_skia::Pattern::new(page.as_ref(), SpreadMode::Pad, FilterQuality::Bilinear, self.opacity, transform),
            anti_alias: true,
            blend_mode: blend,
            ..Paint::default()
        };
        pix.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

/// The affine map taking texel points `t` onto device points `d`, or `None` for a degenerate
/// triangle.
pub(crate) fn texel_to_device(t: [Point; 3], d: [Point; 3]) -> Option<Affine> {
    let (t1, t2) = (t[1] - t[0], t[2] - t[0]);
    let det = t1.x * t2.y - t2.x * t1.y;
    if det.abs() < 1e-9 || !det.is_finite() {
        return None;
    }
    // M · [t1 t2] = [d1 d2], so M = [d1 d2] · [t1 t2]⁻¹.
    let (d1, d2) = (d[1] - d[0], d[2] - d[0]);
    let inv = [t2.y / det, -t1.y / det, -t2.x / det, t1.x / det];
    let m = [d1.x * inv[0] + d2.x * inv[1], d1.y * inv[0] + d2.y * inv[1], d1.x * inv[2] + d2.x * inv[3], d1.y * inv[2] + d2.y * inv[3]];
    let lin = Affine::new([m[0], m[1], m[2], m[3], 0.0, 0.0]);
    let off = d[0] - lin * t[0];
    let out = Affine::new([m[0], m[1], m[2], m[3], off.x, off.y]);
    out.as_coeffs().iter().all(|c| c.is_finite()).then_some(out)
}

/// Where playing Spine skeletons are `t` seconds in, at 60 frames a second: a frame number (a
/// cache key) and the seconds until the next frame. `None` when none is playing.
pub(crate) fn clock(n: &Node, t: f64) -> Option<(u64, f64)> {
    let figcraft_doc::NodeKind::Spine(s) = &n.kind else { return None };
    if !s.playing || !t.is_finite() {
        return None;
    }
    const STEP: f64 = 1.0 / 60.0;
    let frame = (t.max(0.0) / STEP).floor();
    Some((frame as u64, ((frame + 1.0) * STEP - t).max(1e-3)))
}

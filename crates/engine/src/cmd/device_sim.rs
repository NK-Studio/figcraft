//! The device preview (`docs/device-profiles.md` › Device preview): a frame as it would be on a
//! device, worked out on a copy of the document by the steps `frame.device.set` takes, with the
//! layers that cross the safe area or leave the screen there. `device.simulate` answers it as
//! data (and a PNG), `device.preview` asks the UI for the floating pane. FigCraft extensions like
//! the other `device.*` ids; none edits the document.

use figcraft_doc::proto::Overflow;
use figcraft_doc::{BoxRect, DeviceFrame, Document, NodeId, NodeKind};
use figcraft_geom::Point;
use serde_json::{Value, json};

use super::device::{frame_snapshot, put_on_device, snapshot_param};
use crate::cmd::{CommandSpec, ok};
use crate::params::{self, get};
use crate::{Result, Session, bad, file_access, spec};

/// Warnings one simulation reports at most (a file can hold any number of layers).
pub const MAX_WARNINGS: usize = 200;
/// Layers one simulation looks at for warnings at most.
const MAX_VISITED: usize = 20_000;
/// Half a point of slack, as the safe area's own tests have (`DeviceFrame::holds`).
const SLACK: f64 = 0.5;
/// The sides in the order the checks below use.
const EDGES: [&str; 4] = ["left", "top", "right", "bottom"];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "device.simulate",
            "Simulate Device",
            "{frame? (a frame or a layer in one; default the selection's frame, else the page's first), device (an id from device.list), orientation? (default the device's first), screen? (default 0), path? (a `.png` file to draw the result into), scale? (the PNG's pixels per point, 0.1–4, default 1)} → {frame, device, name, orientation, screen, width, height, safeArea: {top, right, bottom, left}, cutouts, warnings: [{id, name, kind (\"safeArea\" | \"offscreen\"), edges, box}], path?}: the frame as `frame.device.set` would leave it on that device, worked out on a copy (the file is untouched)",
            false,
            device_simulate
        ),
        spec!(
            "device.preview",
            "Open device preview",
            "{frame? (default as device.simulate), device? (an id from device.list; default the one shown last, else the first), orientation?, screen? (with `device`)} → {frame, device?, orientation?, screen?}: a floating pane over the canvas showing the frame on a device, following edits live",
            false,
            device_preview
        ),
        spec!("device.closePreview", "Close device preview", "{}", false, |s, _| {
            s.request_ui("devicePreview.close", json!({}));
            ok()
        }),
    ]
}

/// The snapshot a frame keeps of the imported `device`'s `screen` turned to `orientation` (by
/// name; the screen's first when it doesn't support that one). `None` when there is no such
/// device or screen.
pub fn snapshot(s: &Session, device: &str, screen: usize, orientation: &str) -> Option<DeviceFrame> {
    let d = s.devices.get(device)?;
    let sc = d.screen(screen)?;
    let supported = sc.supported();
    let o = figcraft_device::Orientation::parse(orientation).filter(|o| supported.contains(o)).or_else(|| supported.first().copied())?;
    Some(frame_snapshot(d, sc, o))
}

/// `doc` with `frame` put on the device screen `snap`, as `frame.device.set` would leave it: the
/// frame takes the screen's size in points, its layers follow their constraints, the safe area
/// and the frame's UI scaling, and auto layout flows again. `doc` itself is untouched.
pub fn simulate(doc: &Document, frame: NodeId, snap: &DeviceFrame) -> Result<Document> {
    if !doc.node(frame).is_some_and(|n| n.kind.is_frame_like()) {
        return Err(bad("device.simulate", format!("{frame} is not a frame")));
    }
    let mut sim = doc.clone();
    sim.take_dirty();
    put_on_device(&mut sim, &[frame], snap, true)?;
    crate::normalize(&mut sim, doc);
    Ok(sim)
}

/// What a [`SimWarning`] is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarningKind {
    /// The layer crosses the safe area's edge without reaching the screen's (a background does).
    SafeArea,
    /// The layer sticks out of the screen on a side where it was inside the frame as designed.
    Offscreen,
}

impl WarningKind {
    pub fn id(self) -> &'static str {
        match self {
            WarningKind::SafeArea => "safeArea",
            WarningKind::Offscreen => "offscreen",
        }
    }
}

/// A layer of a simulated frame that needs a look on that device.
#[derive(Clone, Debug, PartialEq)]
pub struct SimWarning {
    pub id: NodeId,
    pub name: String,
    pub kind: WarningKind,
    /// The sides it crosses: `left`, `top`, `right`, `bottom`.
    pub edges: Vec<&'static str>,
    /// The layer's box in the frame (points, top-left origin).
    pub rect: BoxRect,
}

impl SimWarning {
    pub fn to_json(&self) -> Value {
        let r = self.rect;
        json!({
            "id": self.id, "name": self.name, "kind": self.kind.id(), "edges": self.edges,
            "box": {"x": r4(r.x), "y": r4(r.y), "width": r4(r.width), "height": r4(r.height)},
        })
    }
}

fn r4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// `id`'s box in `frame`'s own coordinates: `[x0, y0, x1, y1]` round its four corners (a turned
/// layer's bounding box). `None` when a number isn't finite.
fn box_in(doc: &Document, frame: NodeId, id: NodeId) -> Option<[f64; 4]> {
    let n = doc.node(id)?;
    let m = doc.world_transform(frame).inverse() * doc.world_transform(id);
    let corners = [(0.0, 0.0), (n.width, 0.0), (n.width, n.height), (0.0, n.height)].map(|(x, y)| m * Point::new(x, y));
    let b = corners.iter().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, p| {
        [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)]
    });
    b.iter().all(|v| v.is_finite()).then_some(b)
}

/// The sides of a `w × h` box that `b` sticks out of.
fn past(b: [f64; 4], w: f64, h: f64) -> [bool; 4] {
    [b[0] < -SLACK, b[1] < -SLACK, b[2] > w + SLACK, b[3] > h + SLACK]
}

/// The layers of `frame` in `sim` (a [`simulate`]d copy of `source`) that need a look on its
/// device, top-most parents first:
///
/// - **off screen**: a layer sticks out of the screen on a side where it was inside the frame as
///   designed (not along an axis the frame scrolls);
/// - **safe area**: a layer crosses the safe area's edge without reaching the screen's edge on
///   that side. One that reaches it is a background (it bleeds, as `DeviceFrame::holds` reads
///   it), and the layers in it are checked instead; text never counts as a background.
///
/// A layer with a warning stands for what's in it. At most [`MAX_WARNINGS`].
pub fn warnings(source: &Document, sim: &Document, frame: NodeId) -> Vec<SimWarning> {
    let (Some(f), Some(src)) = (sim.node(frame), source.node(frame)) else { return Vec::new() };
    let (w, h) = (f.width, f.height);
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Vec::new();
    }
    let i = f.device.as_ref().map(|d| d.insets_for(w, h)).unwrap_or_default();
    let safe = [i.left, i.top, w - i.right, h - i.bottom];
    let inset = [i.left > SLACK, i.top > SLACK, i.right > SLACK, i.bottom > SLACK];
    let (scroll_x, scroll_y) =
        (matches!(f.overflow, Overflow::Horizontal | Overflow::Both), matches!(f.overflow, Overflow::Vertical | Overflow::Both));
    let scrolls = [scroll_x, scroll_y, scroll_x, scroll_y];
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = f.children.iter().rev().copied().collect();
    let mut visited = 0usize;
    while let Some(id) = stack.pop() {
        visited += 1;
        if visited > MAX_VISITED || out.len() >= MAX_WARNINGS {
            break;
        }
        let Some(n) = sim.node(id).filter(|n| n.visible) else { continue };
        let Some(b) = box_in(sim, frame, id) else { continue };
        let rect = BoxRect { x: b[0], y: b[1], width: b[2] - b[0], height: b[3] - b[1] };
        let mut warn = |kind, edges: Vec<&'static str>| out.push(SimWarning { id, name: n.name.clone(), kind, edges, rect });
        let now = past(b, w, h);
        let was = box_in(source, frame, id).map_or([false; 4], |b| past(b, src.width, src.height));
        let off: Vec<&str> = (0..4).filter(|k| now[*k] && !was[*k] && !scrolls[*k]).map(|k| EDGES[k]).collect();
        if !off.is_empty() {
            warn(WarningKind::Offscreen, off);
            continue;
        }
        let crosses = [
            inset[0] && b[0] < safe[0] - SLACK,
            inset[1] && b[1] < safe[1] - SLACK,
            inset[2] && b[2] > safe[2] + SLACK,
            inset[3] && b[3] > safe[3] + SLACK,
        ];
        let bleeds = [b[0] <= SLACK, b[1] <= SLACK, b[2] >= w - SLACK, b[3] >= h - SLACK];
        let text = matches!(n.kind, NodeKind::Text(_));
        let hit: Vec<&str> = (0..4).filter(|k| crosses[*k] && (text || !bleeds[*k])).map(|k| EDGES[k]).collect();
        if !hit.is_empty() {
            warn(WarningKind::SafeArea, hit);
        } else if crosses.contains(&true) {
            // A background: what's in it may still sit under the notch or the home bar.
            stack.extend(n.children.iter().rev().copied());
        }
    }
    out
}

/// The frame a command previews: `frame` (or the top-level frame it is in), else the selection's,
/// else the page's first.
fn frame_param(cmd: &str, s: &Session, p: &Value) -> Result<NodeId> {
    let ds = s.ds()?;
    if get(p, "frame").is_some_and(|v| !v.is_null()) {
        let f = params::req_id(cmd, p, "frame")?;
        return ds.doc.top_level_frame(f).ok_or_else(|| bad(cmd, "`frame` isn't a frame or inside one"));
    }
    ds.selection
        .first()
        .and_then(|i| ds.doc.top_level_frame(*i))
        .or_else(|| ds.doc.top_frames(ds.page).first().copied())
        .ok_or_else(|| bad(cmd, "no frame to preview"))
}

fn device_simulate(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "device.simulate";
    let frame = frame_param(CMD, s, p)?;
    let (snap, screen) = snapshot_param(CMD, s, p)?;
    let scale = match get(p, "scale") {
        None => 1.0,
        Some(v) => params::f64_of(v).filter(|k| (0.1..=4.0).contains(k)).ok_or_else(|| bad(CMD, "`scale` must be from 0.1 to 4"))?,
    };
    let path = match get(p, "path") {
        None => None,
        Some(v) => Some(v.as_str().filter(|p| p.to_ascii_lowercase().ends_with(".png")).ok_or_else(|| bad(CMD, "`path` must be a `.png` file"))?),
    };
    let doc = s.doc()?;
    let sim = simulate(doc, frame, &snap)?;
    let found = warnings(doc, &sim, frame);
    let n = sim.get(frame)?;
    let i = snap.insets_for(n.width, n.height);
    let cutouts: Vec<Value> = snap
        .cutouts_for(n.width, n.height)
        .iter()
        .map(|c| json!({"x": r4(c.x), "y": r4(c.y), "width": r4(c.width), "height": r4(c.height)}))
        .collect();
    let mut out = json!({
        "frame": frame, "device": snap.id, "name": snap.name, "orientation": snap.orientation, "screen": screen,
        "width": r4(n.width), "height": r4(n.height),
        "safeArea": {"top": r4(i.top), "right": r4(i.right), "bottom": r4(i.bottom), "left": r4(i.left)},
        "cutouts": cutouts,
        "warnings": found.iter().map(SimWarning::to_json).collect::<Vec<_>>(),
    });
    if let Some(path) = path {
        let area = sim.world_bounds(frame).ok_or_else(|| bad(CMD, "the frame has no bounds"))?;
        let pix = figcraft_render::render_area(&sim, &[frame], area, scale)
            .ok_or_else(|| bad(CMD, format!("the frame is too large to draw at scale {scale}")))?;
        let png = figcraft_render::encode_png(&pix).map_err(|e| bad(CMD, e))?;
        file_access::write(path, &png)?;
        if let Some(o) = out.as_object_mut() {
            o.insert("path".into(), json!(path));
        }
    }
    Ok(out)
}

fn device_preview(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "device.preview";
    let frame = frame_param(CMD, s, p)?;
    let mut data = serde_json::Map::new();
    data.insert("frame".into(), json!(frame));
    if get(p, "device").is_some_and(|v| !v.is_null()) {
        let (snap, screen) = snapshot_param(CMD, s, p)?;
        data.insert("device".into(), json!(snap.id));
        data.insert("orientation".into(), json!(snap.orientation));
        data.insert("screen".into(), json!(screen));
    } else if get(p, "orientation").is_some() || get(p, "screen").is_some() {
        return Err(bad(CMD, "`orientation` and `screen` go with `device`"));
    }
    let data = Value::Object(data);
    s.request_ui("devicePreview", data.clone());
    Ok(data)
}

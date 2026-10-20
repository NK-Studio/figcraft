//! Device profiles (`docs/device-profiles.md`): import Unity Device Simulator `.device` files or
//! `figcraft-device` files into the session, and query a device's screen size, safe area and
//! cutouts in pixels and points per orientation. A FigCraft extension (the incumbent has no safe
//! areas), so these ids are outside the menu catalog. None edits the document.

use std::path::{Path, PathBuf};

use figcraft_device::{DeviceProfile, Insets, Orientation, ParseHint, Platform, PxRect, Screen, native};
use figcraft_doc::{BoxRect, DeviceFrame, NodeId, SideInsets};
use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::params::{self, get};
use crate::{EngineError, Result, Session, bad, file_access, spec};

/// Files a folder import reads at most.
const MAX_FOLDER_FILES: usize = 512;
/// Imported paths the preferences remember.
pub const MAX_SOURCES: usize = 32;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "device.import",
            "Import Devices",
            "{path (a Unity `.device` file, a `*.figdevice.json` file, or a folder of them; a Unity package folder's `Editor/Devices` is found) | text (the file's JSON), scale? (pixels per point for every screen, instead of the inferred one), replace? (default true)} → {imported: [id], skipped: [{file, error}]}",
            false,
            device_import
        ),
        spec!(
            "device.list",
            "Devices",
            "{platform? (\"ios\" | \"android\" | \"other\")} → [{id, name, platform, source, screens: [{widthPx, heightPx, scale, widthPt, heightPt, orientations}]}]",
            false,
            device_list
        ),
        spec!(
            "device.get",
            "Device Safe Area",
            "{id, orientation? (\"portrait\" | \"portraitUpsideDown\" | \"landscapeLeft\" | \"landscapeRight\" or Unity's 1–4; default the first supported), screen? (index, default 0)} → size, safe area (rect and insets) and cutouts in px and pt, scale, corner radius, overlay",
            false,
            device_get
        ),
        spec!(
            "device.export",
            "Export Device",
            "{id, path? (a `.json` file to write)} → {id, json} (the profile as `figcraft-device` JSON)",
            false,
            device_export
        ),
        spec!(
            "device.remove",
            "Remove Device",
            "{id} | {all: true (also forgets the files and folders imported again at start-up)} → {removed: n}",
            false,
            device_remove
        ),
        spec!(
            "frame.device.set",
            "Set Frame Device",
            "{ids? (frames; default the selection), device (an id from device.list), orientation? (default the device's first), screen? (default 0), resize? (default true: the frames take the device's size in points)} → {ids, device, orientation, width, height}",
            true,
            frame_device_set
        ),
        spec!("frame.device.clear", "Clear Frame Device", "{ids? (default the selection)}", true, frame_device_clear),
    ]
}

fn device_err(cmd: &str, e: figcraft_device::DeviceError) -> EngineError {
    bad(cmd, e.to_string())
}

fn hint_for(path: Option<&Path>, scale: Option<f64>) -> ParseHint {
    ParseHint {
        file: path.map(|p| p.to_string_lossy().into_owned()),
        dir: path.and_then(Path::parent).map(|d| d.to_string_lossy().into_owned()),
        scale,
    }
}

fn is_device_file(p: &Path) -> bool {
    let name = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    name.ends_with(".device") || name.ends_with(native::EXTENSION)
}

/// The device files in folder `dir`, its `Editor/Devices` and its `Devices` folders, sorted.
fn folder_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for d in [dir.to_path_buf(), dir.join("Editor").join("Devices"), dir.join("Devices")] {
        if !file_access::is_dir(&d) {
            continue;
        }
        files.extend(file_access::read_dir(&d)?.into_iter().filter(|p| is_device_file(p)));
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn device_import(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "device.import";
    let scale = match get(p, "scale") {
        None => None,
        Some(v) => Some(params::f64_of(v).filter(|k| (0.1..=10.0).contains(k)).ok_or_else(|| bad(CMD, "`scale` must be a number from 0.1 to 10"))?),
    };
    let replace = params::boolean(p, "replace").unwrap_or(true);
    if let Some(text) = get(p, "text").and_then(Value::as_str) {
        let profile = figcraft_device::parse(text.as_bytes(), &hint_for(None, scale)).map_err(|e| device_err(CMD, e))?;
        return Ok(json!({"imported": [insert(s, profile, replace)?], "skipped": []}));
    }
    let Some(path) = get(p, "path").and_then(Value::as_str).filter(|p| !p.trim().is_empty()) else {
        return Err(bad(CMD, "give `path` (a device file or folder) or `text`"));
    };
    let path = PathBuf::from(path);
    let r = import_path(s, &path, scale, replace)?;
    if scale.is_none() && r["imported"].as_array().is_some_and(|a| !a.is_empty()) {
        remember_source(s, &path);
    }
    Ok(r)
}

/// Keep `path` (made absolute) in the preferences' device sources, newest last.
fn remember_source(s: &mut Session, path: &Path) {
    let p = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().into_owned();
    let sources = &mut s.prefs.device_sources;
    sources.retain(|q| *q != p);
    sources.push(p);
    let over = sources.len().saturating_sub(MAX_SOURCES);
    sources.drain(..over);
}

/// Import the devices of the files and folders the preferences remember (at start-up); ones that
/// are gone or unreadable are skipped quietly. Returns how many devices were imported.
pub fn restore_devices(s: &mut Session) -> usize {
    let sources = s.prefs.device_sources.clone();
    let mut n = 0;
    for src in sources.iter().take(MAX_SOURCES) {
        if let Ok(r) = import_path(s, Path::new(src), None, true) {
            n += r["imported"].as_array().map_or(0, Vec::len);
        }
    }
    n
}

fn import_path(s: &mut Session, path: &Path, scale: Option<f64>, replace: bool) -> Result<Value> {
    const CMD: &str = "device.import";
    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    if !file_access::is_dir(path) {
        let profile = read_one(path, scale).map_err(|e| bad(CMD, e))?;
        imported.push(insert(s, profile, replace)?);
        return Ok(json!({"imported": imported, "skipped": skipped}));
    }
    let files = folder_files(path)?;
    if files.is_empty() {
        return Err(bad(CMD, format!("no `.device` or `{}` files in {}", native::EXTENSION, path.display())));
    }
    for (i, f) in files.iter().enumerate() {
        let file = f.to_string_lossy().into_owned();
        if i >= MAX_FOLDER_FILES {
            skipped.push(json!({"file": file, "error": format!("more than {MAX_FOLDER_FILES} files")}));
            continue;
        }
        match read_one(f, scale).and_then(|profile| insert(s, profile, replace).map_err(|e| e.to_string())) {
            Ok(id) => imported.push(id),
            Err(error) => skipped.push(json!({"file": file, "error": error})),
        }
    }
    s.message = Some(format!("Imported {} devices", imported.len()));
    Ok(json!({"imported": imported, "skipped": skipped}))
}

fn read_one(path: &Path, scale: Option<f64>) -> std::result::Result<DeviceProfile, String> {
    let bytes = file_access::read_capped(path, figcraft_device::MAX_FILE_BYTES).map_err(|e| e.to_string())?;
    figcraft_device::parse(&bytes, &hint_for(Some(path), scale)).map_err(|e| e.to_string())
}

fn insert(s: &mut Session, profile: DeviceProfile, replace: bool) -> Result<String> {
    let id = profile.id.clone();
    s.devices.insert(profile, replace).map_err(|e| device_err("device.import", e))?;
    Ok(id)
}

/// `v` rounded to 4 decimals (points are often thirds).
fn r4(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

fn size_json((w, h): (f64, f64)) -> Value {
    json!({"width": r4(w), "height": r4(h)})
}

fn rect_json(r: PxRect) -> Value {
    json!({"x": r4(r.x), "y": r4(r.y), "width": r4(r.width), "height": r4(r.height)})
}

fn insets_json(i: Insets) -> Value {
    json!({"top": r4(i.top), "right": r4(i.right), "bottom": r4(i.bottom), "left": r4(i.left)})
}

fn screen_summary(sc: &Screen) -> Value {
    let (w, h) = sc.size_pt(Orientation::Portrait);
    json!({
        "widthPx": sc.width_px, "heightPx": sc.height_px, "scale": r4(sc.scale), "scaleSource": sc.scale_source.name(),
        "widthPt": r4(w), "heightPt": r4(h),
        "orientations": sc.supported().iter().map(|o| o.name()).collect::<Vec<_>>(),
    })
}

fn device_list(s: &mut Session, p: &Value) -> Result<Value> {
    let platform = match params::string(p, "platform") {
        None => None,
        Some(name) => Some(Platform::parse(&name).ok_or_else(|| bad("device.list", "`platform` must be ios, android or other"))?),
    };
    Ok(Value::Array(
        s.devices
            .iter()
            .filter(|d| platform.is_none_or(|pl| d.platform == pl))
            .map(|d| {
                json!({
                    "id": d.id, "name": d.name, "platform": d.platform.name(), "source": d.source.name(),
                    "screens": d.screens.iter().map(screen_summary).collect::<Vec<_>>(),
                })
            })
            .collect(),
    ))
}

fn profile<'a>(s: &'a Session, cmd: &str, p: &Value) -> Result<&'a DeviceProfile> {
    let id = params::req_string(cmd, p, "id")?;
    s.devices.get(&id).ok_or_else(|| bad(cmd, format!("no device `{id}` (see device.list; import with device.import)")))
}

fn device_get(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "device.get";
    let d = profile(s, CMD, p)?;
    let index = match get(p, "screen") {
        None => 0,
        Some(_) => params::req_index(CMD, p, "screen")?,
    };
    let sc = d.screen(index).ok_or_else(|| bad(CMD, format!("`screen` must be below {}", d.screens.len())))?;
    let supported = sc.supported();
    let names: Vec<&str> = supported.iter().map(|o| o.name()).collect();
    let o = match params::string(p, "orientation") {
        None => supported.first().copied().ok_or_else(|| bad(CMD, "the device lists no orientations"))?,
        Some(name) => {
            let o = Orientation::parse(&name)
                .ok_or_else(|| bad(CMD, "`orientation` must be portrait, portraitUpsideDown, landscapeLeft or landscapeRight"))?;
            if !supported.contains(&o) {
                return Err(bad(CMD, format!("{} doesn't support {} (it supports {})", d.name, o.name(), names.join(", "))));
            }
            o
        }
    };
    let k = 1.0 / sc.scale;
    let insets = sc.insets_px(o).unwrap_or_default();
    let safe = sc.safe_area_px(o).unwrap_or_default();
    let cutouts: Vec<PxRect> = sc.info(o).map(|i| i.cutouts.clone()).unwrap_or_default();
    Ok(json!({
        "id": d.id, "name": d.name, "platform": d.platform.name(), "model": d.model, "os": d.os, "source": d.source,
        "screen": index, "screens": d.screens.len(),
        "orientation": o.name(), "orientations": names,
        "scale": r4(sc.scale), "scaleSource": sc.scale_source.name(), "dpi": sc.dpi,
        "sizePx": size_json(sc.size_px(o)), "sizePt": size_json(sc.size_pt(o)),
        "safeAreaPx": rect_json(safe), "safeAreaPt": rect_json(safe.scaled(k)),
        "insetsPx": insets_json(insets), "insetsPt": insets_json(insets.scaled(k)),
        "cutoutsPx": cutouts.iter().map(|c| rect_json(*c)).collect::<Vec<_>>(),
        "cutoutsPt": cutouts.iter().map(|c| rect_json(c.scaled(k))).collect::<Vec<_>>(),
        "cornerRadiusPx": r4(sc.corner_radius_px), "cornerRadiusPt": r4(sc.corner_radius_px * k),
        "navigationBarPx": r4(sc.navigation_bar_px),
        "overlay": sc.overlay.as_ref().map(|ov| json!({"path": ov.path, "borderPx": insets_json(ov.border_px)})),
    }))
}

fn device_export(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "device.export";
    let d = profile(s, CMD, p)?;
    let text = native::to_string(d).map_err(|e| device_err(CMD, e))?;
    let id = d.id.clone();
    if let Some(path) = get(p, "path") {
        let Some(path) = path.as_str().filter(|p| p.to_ascii_lowercase().ends_with(".json")) else {
            return Err(bad(CMD, "`path` must be a `.json` file"));
        };
        file_access::write(path, text.as_bytes())?;
        return Ok(json!({"id": id, "path": path}));
    }
    Ok(json!({"id": id, "json": text}))
}

fn device_remove(s: &mut Session, p: &Value) -> Result<Value> {
    if params::boolean(p, "all") == Some(true) {
        // Forget the remembered sources too, so they don't come back at the next start.
        s.prefs.device_sources.clear();
        return Ok(json!({"removed": s.devices.clear()}));
    }
    let id = params::req_string("device.remove", p, "id")?;
    Ok(json!({"removed": usize::from(s.devices.remove(&id))}))
}

/// What a frame keeps of device `d`'s screen `index` in orientation `o`, in points.
fn frame_snapshot(d: &DeviceProfile, sc: &Screen, o: Orientation) -> DeviceFrame {
    let (width, height) = sc.size_pt(o);
    let i = sc.insets_pt(o).unwrap_or_default();
    DeviceFrame {
        id: d.id.clone(),
        name: d.name.clone(),
        orientation: o.name().into(),
        width: r4(width),
        height: r4(height),
        safe_area: SideInsets { top: r4(i.top), right: r4(i.right), bottom: r4(i.bottom), left: r4(i.left) },
        cutouts: sc.cutouts_pt(o).iter().map(|c| BoxRect { x: r4(c.x), y: r4(c.y), width: r4(c.width), height: r4(c.height) }).collect(),
        corner_radius: r4(sc.corner_radius_px / sc.scale),
    }
}

fn frame_targets(cmd: &str, s: &Session, p: &Value) -> Result<Vec<NodeId>> {
    let ids = params::req_targets(cmd, s, p)?;
    let doc = s.doc()?;
    if let Some(i) = ids.iter().find(|i| !doc.node(**i).is_some_and(|n| n.kind.is_frame_like())) {
        return Err(bad(cmd, format!("{i} is not a frame")));
    }
    Ok(ids)
}

fn frame_device_set(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "frame.device.set";
    let ids = frame_targets(CMD, s, p)?;
    let device = params::req_string(CMD, p, "device")?;
    let d = s.devices.get(&device).cloned().ok_or_else(|| bad(CMD, format!("no device `{device}` (see device.list; import with device.import)")))?;
    let index = match get(p, "screen") {
        None => 0,
        Some(_) => params::req_index(CMD, p, "screen")?,
    };
    let sc = d.screen(index).ok_or_else(|| bad(CMD, format!("`screen` must be below {}", d.screens.len())))?;
    let supported = sc.supported();
    let o = match params::string(p, "orientation") {
        None => *supported.first().ok_or_else(|| bad(CMD, "the device lists no orientations"))?,
        Some(name) => Orientation::parse(&name).filter(|o| supported.contains(o)).ok_or_else(|| {
            bad(CMD, format!("orientation must be one {} supports: {}", d.name, supported.iter().map(|o| o.name()).collect::<Vec<_>>().join(", ")))
        })?,
    };
    let snap = frame_snapshot(&d, sc, o);
    let resize = params::boolean(p, "resize").unwrap_or(true);
    // Layers inside the old device's safe area follow the safe area (as the SafeArea container
    // does in Unity): noted before the frame resizes, placed against the new one after.
    let followers: Vec<SafeFollowers> = if resize { ids.iter().filter_map(|i| safe_followers(s.doc().ok()?, *i)).collect() } else { Vec::new() };
    if resize {
        for i in &ids {
            super::arrange::resize_cmd(s, &json!({"ids": [i], "width": snap.width, "height": snap.height}))?;
        }
    }
    let doc = s.doc_mut()?;
    for i in &ids {
        doc.get_mut(*i)?.device = Some(snap.clone());
    }
    for f in &followers {
        place_followers(doc, f, &snap)?;
    }
    Ok(json!({"ids": ids, "device": snap.id, "orientation": snap.orientation, "width": snap.width, "height": snap.height}))
}

fn frame_device_clear(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = params::req_targets("frame.device.clear", s, p)?;
    let doc = s.doc_mut()?;
    let mut cleared = 0;
    for i in &ids {
        if doc.get_mut(*i)?.device.take().is_some() {
            cleared += 1;
        }
    }
    Ok(json!({"cleared": cleared}))
}

/// After frame `id` turned between portrait and landscape (`frame.swapOrientation`): its device
/// takes the turned orientation (portrait, else upside down; landscape left, else right), or is
/// dropped when the profile isn't imported or doesn't support one.
pub(crate) fn turn_frame_device(s: &mut Session, id: NodeId, followers: Option<SafeFollowers>) -> Result<()> {
    let Some(df) = s.doc()?.node(id).and_then(|n| n.device.clone()) else { return Ok(()) };
    let was_landscape = Orientation::parse(&df.orientation).is_some_and(Orientation::is_landscape);
    let wanted = if was_landscape {
        [Orientation::Portrait, Orientation::PortraitUpsideDown]
    } else {
        [Orientation::LandscapeLeft, Orientation::LandscapeRight]
    };
    let snap = s.devices.get(&df.id).and_then(|d| {
        let sc = d.screen(0)?;
        let o = wanted.into_iter().find(|o| sc.info(*o).is_some())?;
        Some(frame_snapshot(d, sc, o))
    });
    if snap.is_none() {
        s.message = Some(format!("{}: device dropped (import it again to keep its safe area when turning)", df.name));
    }
    let doc = s.doc_mut()?;
    doc.get_mut(id)?.device = snap.clone();
    if let (Some(snap), Some(f)) = (snap, followers) {
        place_followers(doc, &f, &snap)?;
    }
    Ok(())
}

/// A device frame's layers inside its safe area, with their boxes, before the device changes.
pub(crate) struct SafeFollowers {
    frame: NodeId,
    old_safe: BoxRect,
    /// (layer, x, y, width, height) in the frame.
    layers: Vec<(NodeId, f64, f64, f64, f64)>,
}

pub(crate) fn safe_followers(doc: &figcraft_doc::Document, frame: NodeId) -> Option<SafeFollowers> {
    let n = doc.node(frame)?;
    let old = n.device.as_ref()?;
    let mut layers = Vec::new();
    for c in n.children.iter().filter_map(|c| doc.node(*c)) {
        // Layers in an auto layout flow move with the layout; turned layers keep the resize's place.
        let in_flow = n.layout.is_some() && !c.child_layout.absolute;
        if in_flow || figcraft_geom::rotation_degrees(c.transform).abs() > 1e-6 {
            continue;
        }
        let at = figcraft_geom::translation(c.transform);
        if old.holds(n.width, n.height, at.x, at.y, c.width, c.height) {
            layers.push((c.id, at.x, at.y, c.width, c.height));
        }
    }
    Some(SafeFollowers { frame, old_safe: old.safe_rect_for(n.width, n.height), layers })
}

/// One axis of a constraint from a box `from` long to one `to` long: the new position and size.
fn follow(pin: Pin, pos: f64, len: f64, from: f64, to: f64) -> (f64, f64) {
    match pin {
        Pin::Start => (pos, len),
        Pin::End => (to - (from - pos - len) - len, len),
        Pin::Center => (pos + (to - from) / 2.0, len),
        Pin::Stretch => (pos, (len + to - from).max(0.0)),
        Pin::Scale if from > 0.0 => (pos * to / from, len * to / from),
        Pin::Scale => (pos, len),
    }
}

#[derive(Clone, Copy)]
enum Pin {
    Start,
    End,
    Center,
    Stretch,
    Scale,
}

fn place_followers(doc: &mut figcraft_doc::Document, f: &SafeFollowers, snap: &DeviceFrame) -> Result<()> {
    let Some(n) = doc.node(f.frame) else { return Ok(()) };
    let new_safe = snap.safe_rect_for(n.width, n.height);
    let (o, s) = (f.old_safe, new_safe);
    for &(id, x, y, w, h) in &f.layers {
        let Some(c) = doc.node(id) else { continue };
        let hpin = match c.constraints.horizontal {
            figcraft_doc::HConstraint::Left => Pin::Start,
            figcraft_doc::HConstraint::Right => Pin::End,
            figcraft_doc::HConstraint::Center => Pin::Center,
            figcraft_doc::HConstraint::LeftRight => Pin::Stretch,
            figcraft_doc::HConstraint::Scale => Pin::Scale,
        };
        let vpin = match c.constraints.vertical {
            figcraft_doc::VConstraint::Top => Pin::Start,
            figcraft_doc::VConstraint::Bottom => Pin::End,
            figcraft_doc::VConstraint::Center => Pin::Center,
            figcraft_doc::VConstraint::TopBottom => Pin::Stretch,
            figcraft_doc::VConstraint::Scale => Pin::Scale,
        };
        let (nx, nw) = follow(hpin, x - o.x, w, o.width, s.width);
        let (ny, nh) = follow(vpin, y - o.y, h, o.height, s.height);
        let mut props = serde_json::Map::new();
        props.insert("x".into(), json!(r4(nx + s.x)));
        props.insert("y".into(), json!(r4(ny + s.y)));
        props.insert("width".into(), json!(r4(nw)));
        props.insert("height".into(), json!(r4(nh)));
        crate::cmd::props::apply("frame.device.set", doc, id, &props)?;
    }
    Ok(())
}

//! Spine skeletons (`docs/spine.md`): place a Spine 4.3 export as a layer, pick its animation,
//! skin and time, and read what it holds. A FigCraft extension, outside the menu catalog.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use figcraft_doc::{Node, NodeId, NodeKind, SpineData};
use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::params::{boolean, get, num, string};
use crate::{Result, Session, bad, spec};

/// Largest skeleton, atlas or atlas page read from disk.
const MAX_FILE: u64 = 256 * 1024 * 1024;
/// Most atlas pages one skeleton may have.
const MAX_PAGES: usize = 256;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "spine.place",
            "Place Spine Skeleton",
            "{path (a skeleton `.json`/`.skel`, or its `.atlas`) | skeleton (base64) + binary? + atlas (text) + pages ({file: base64}), atlas? (path; default `<name>.atlas` beside the skeleton), name?, x?, y? (top-left; default centred in the view), parent?, animation?, skin?} → {id, asset, width, height, animations, skins}",
            true,
            place
        ),
        spec!(
            "spine.set",
            "Set Spine Pose",
            "{ids? (default the selection), animation? (a name, or null for the setup pose), skin? (a name, or null for the default skin), time? (seconds), looping?, playing? (plays in presentation), speed?} → {ids}",
            true,
            set
        ),
        spec!(
            "spine.info",
            "Spine Skeleton Info",
            "{id? (default the first selected Spine layer)} → {id, asset, name, version, animations: [{name, duration}], skins, pages, animation, skin, time, looping, playing, speed}",
            false,
            info
        ),
    ]
}

/// The pieces of a Spine export.
struct Export {
    name: String,
    skeleton: Vec<u8>,
    binary: bool,
    atlas: String,
    /// Where page files are looked for (the atlas's folder), when read from disk.
    dir: Option<PathBuf>,
    pages: Vec<(String, Vec<u8>)>,
}

fn ext(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn read(path: &Path) -> Result<Vec<u8>> {
    crate::file_access::read_capped(path, MAX_FILE)
}

/// The export at `path`: a skeleton with its atlas beside it, or an atlas with its skeleton.
fn from_path(cmd: &str, path: &Path, atlas: Option<&str>) -> Result<Export> {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Skeleton").to_string();
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let skeleton_path = if ext(path) == "atlas" {
        ["json", "skel"]
            .iter()
            .map(|e| dir.join(format!("{stem}.{e}")))
            .find(|p| crate::file_access::metadata(p).is_ok_and(|m| m.is_file()))
            .ok_or_else(|| bad(cmd, format!("no {stem}.json or {stem}.skel beside the atlas")))?
    } else {
        path.to_path_buf()
    };
    let atlas_path = match atlas {
        Some(a) => PathBuf::from(a),
        None if ext(path) == "atlas" => path.to_path_buf(),
        None => dir.join(format!("{stem}.atlas")),
    };
    let atlas =
        String::from_utf8(read(&atlas_path).map_err(|e| bad(cmd, format!("the atlas: {e}")))?).map_err(|_| bad(cmd, "the atlas isn't text"))?;
    Ok(Export {
        name: stem,
        binary: ext(&skeleton_path) == "skel",
        skeleton: read(&skeleton_path)?,
        atlas,
        dir: atlas_path.parent().map(Path::to_path_buf),
        pages: Vec::new(),
    })
}

fn decode(cmd: &str, what: &str, b64: &str) -> Result<Vec<u8>> {
    let b64 = b64.split_once(',').map_or(b64, |(_, d)| d);
    base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|e| bad(cmd, format!("{what}: {e}")))
}

fn from_data(cmd: &str, p: &Value) -> Result<Export> {
    let skeleton = decode(cmd, "`skeleton`", &string(p, "skeleton").ok_or_else(|| bad(cmd, "pass `path`, or `skeleton` and `atlas`"))?)?;
    let atlas = string(p, "atlas").ok_or_else(|| bad(cmd, "`atlas`: the .atlas text"))?;
    let mut pages = Vec::new();
    if let Some(m) = get(p, "pages").and_then(Value::as_object) {
        for (file, v) in m.iter().take(MAX_PAGES) {
            let b64 = v.as_str().ok_or_else(|| bad(cmd, format!("`pages.{file}`: base64 image data")))?;
            pages.push((file.clone(), decode(cmd, &format!("`pages.{file}`"), b64)?));
        }
    }
    Ok(Export {
        name: string(p, "name").unwrap_or_else(|| "Skeleton".into()),
        binary: boolean(p, "binary").unwrap_or(false),
        skeleton,
        atlas,
        dir: None,
        pages,
    })
}

/// An atlas page name as a file inside `dir` (no absolute paths or `..`).
fn page_path(dir: &Path, page: &str) -> Option<PathBuf> {
    let rel = Path::new(page);
    rel.components().all(|c| matches!(c, std::path::Component::Normal(_))).then(|| dir.join(rel))
}

pub fn place(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "spine.place";
    let mut export = match string(p, "path") {
        Some(path) => from_path(CMD, Path::new(&path), string(p, "atlas").as_deref())?,
        None => from_data(CMD, p)?,
    };
    let asset = figcraft_spine::Asset::load(&export.skeleton, export.binary, &export.atlas).map_err(|e| bad(CMD, e.to_string()))?;
    let info = asset.info().clone();
    if info.pages.len() > MAX_PAGES {
        return Err(bad(CMD, format!("the atlas has more than {MAX_PAGES} pages")));
    }
    if let Some(dir) = &export.dir {
        for page in &info.pages {
            let file = page_path(dir, page).ok_or_else(|| bad(CMD, format!("atlas page {page:?} isn't a file beside the atlas")))?;
            export.pages.push((page.clone(), read(&file).map_err(|e| bad(CMD, format!("atlas page {page}: {e}")))?));
        }
    }
    if let Some(missing) = info.pages.iter().find(|pg| !export.pages.iter().any(|(f, _)| f == *pg)) {
        return Err(bad(CMD, format!("atlas page {missing} is missing")));
    }
    export.pages.retain(|(f, _)| info.pages.contains(f));
    for (key, list) in [("animation", &info.animations.iter().map(|a| a.name.clone()).collect::<Vec<_>>()), ("skin", &info.skins)] {
        if let Some(n) = string(p, key)
            && !list.contains(&n)
        {
            return Err(bad(CMD, format!("no {key} named {n:?}")));
        }
    }
    // One skeleton unit is one point; the node is the setup pose's box.
    let frame = match info.setup_bounds {
        Some([x0, y0, x1, y1]) if x1 > x0 && y1 > y0 => [f64::from(x0), f64::from(y0), f64::from(x1 - x0), f64::from(y1 - y0)],
        _ => [-50.0, -100.0, 100.0, 100.0],
    };
    let (w, h) = (frame[2].round().max(1.0), frame[3].round().max(1.0));
    let center = s.ds().map(|d| (d.view.cx, d.view.cy)).unwrap_or((0.0, 0.0));
    let (x, y) = (num(p, "x").unwrap_or((center.0 - w / 2.0).round()), num(p, "y").unwrap_or((center.1 - h / 2.0).round()));
    let page = s.page()?;
    let parent = crate::params::opt_id(CMD, p, "parent")?.unwrap_or(page);
    let name = string(p, "name").unwrap_or_else(|| export.name.clone());
    let ds = s.ds_mut()?;
    let doc = ds.doc_mut();
    let key = figcraft_format::add_spine(doc, &export.name, export.skeleton, export.binary, export.atlas, export.pages)
        .map_err(|e| bad(CMD, e.to_string()))?;
    let data = SpineData {
        asset: key.clone(),
        animation: string(p, "animation"),
        skin: string(p, "skin"),
        time: 0.0,
        looping: true,
        playing: false,
        speed: 1.0,
        frame,
    };
    let nid = doc.alloc_id();
    let mut node = Node::new(nid, NodeKind::Spine(data), name, w, h);
    node.transform = figcraft_geom::Affine::translate((x, y));
    doc.insert(node, parent, None)?;
    ds.selection = vec![nid];
    Ok(json!({"id": nid, "asset": key, "width": w, "height": h, "animations": info.animations, "skins": info.skins}))
}

/// The loaded skeleton a Spine node shows.
fn asset_of(cmd: &str, s: &Session, id: NodeId) -> Result<(SpineData, std::sync::Arc<figcraft_spine::Asset>)> {
    let doc = s.doc()?;
    let n = doc.node(id).ok_or_else(|| bad(cmd, format!("{id} is not a layer")))?;
    let NodeKind::Spine(data) = &n.kind else { return Err(bad(cmd, format!("{id} isn't a Spine layer"))) };
    let asset = figcraft_render::spine::asset(doc, &data.asset).ok_or_else(|| bad(cmd, format!("{id}: its skeleton doesn't load")))?;
    Ok((data.clone(), asset))
}

/// A name parameter: `None` to leave it, `Some(None)` for null.
fn name_param(cmd: &str, p: &Value, key: &str) -> Result<Option<Option<String>>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(v)) => Ok(Some(Some(v.clone()))),
        Some(_) => Err(bad(cmd, format!("`{key}`: a name or null"))),
    }
}

fn finite(cmd: &str, p: &Value, key: &str) -> Result<Option<f64>> {
    match get(p, key) {
        None => Ok(None),
        Some(_) => num(p, key).filter(|v| v.is_finite()).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a number"))),
    }
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "spine.set";
    let ids = crate::params::req_targets(CMD, s, p)?;
    let animation = name_param(CMD, p, "animation")?;
    let skin = name_param(CMD, p, "skin")?;
    let time = finite(CMD, p, "time")?.map(|t| t.clamp(0.0, f64::from(figcraft_spine::MAX_TIME)));
    let speed = finite(CMD, p, "speed")?.map(|v| v.clamp(-100.0, 100.0));
    let (looping, playing) = (boolean(p, "looping"), boolean(p, "playing"));
    let mut updates = Vec::with_capacity(ids.len());
    for id in &ids {
        let (mut data, asset) = asset_of(CMD, s, *id)?;
        let info = asset.info();
        if let Some(a) = &animation {
            if let Some(name) = a
                && info.animation(name).is_none()
            {
                return Err(bad(CMD, format!("no animation named {name:?}")));
            }
            data.animation = a.clone();
        }
        if let Some(k) = &skin {
            if let Some(name) = k
                && !info.skins.contains(name)
            {
                return Err(bad(CMD, format!("no skin named {name:?}")));
            }
            data.skin = k.clone();
        }
        data.time = time.unwrap_or(data.time);
        data.speed = speed.unwrap_or(data.speed);
        data.looping = looping.unwrap_or(data.looping);
        data.playing = playing.unwrap_or(data.playing);
        updates.push((*id, data));
    }
    let doc = s.doc_mut()?;
    for (id, data) in updates {
        doc.get_mut(id)?.kind = NodeKind::Spine(data);
    }
    Ok(json!({"ids": ids}))
}

fn info(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "spine.info";
    let id = match crate::params::opt_id(CMD, p, "id")? {
        Some(id) => id,
        None => {
            let doc = s.doc()?;
            crate::params::targets(s, p)
                .into_iter()
                .find(|id| doc.node(*id).is_some_and(|n| matches!(n.kind, NodeKind::Spine(_))))
                .ok_or_else(|| bad(CMD, "select a Spine layer or pass `id`"))?
        }
    };
    let (data, asset) = asset_of(CMD, s, id)?;
    let name = s.doc()?.spine.get(&data.asset).map(|a| a.name.clone()).unwrap_or_default();
    let info = asset.info();
    Ok(json!({
        "id": id, "asset": data.asset, "name": name, "version": info.version,
        "animations": info.animations, "skins": info.skins, "pages": info.pages,
        "animation": data.animation, "skin": data.skin, "time": data.time,
        "looping": data.looping, "playing": data.playing, "speed": data.speed,
    }))
}

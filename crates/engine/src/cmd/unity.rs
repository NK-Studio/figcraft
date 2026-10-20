//! Unity UI Toolkit (`docs/unity-uitk.md`): what a design uses that the target Unity version's
//! UXML/USS can't represent exactly. A FigCraft extension, outside the menu catalog; nothing here
//! edits the document.

use figcraft_doc::{NodeId, Pivot, UnitySettings};
use figcraft_uitk::{Position, Severity, UnityVersion};
use serde_json::{Map, Value, json};

use crate::cmd::CommandSpec;
use crate::params::{self, get};
use crate::{Result, Session, bad, spec};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "unity.check",
            "Check for Unity UI Toolkit",
            "{ids? (default the selection, else the current page), version? (target Unity: \"6.0\"…, \"6000.3\"; default the file's, see unity.settings), grid? (the project enables UITK's experimental grid layout, 6.7+; default the file's)} → {version, grid, counts: {error, warning, info}, issues: [{id, name, severity, rule, message, since?}]}",
            false,
            unity_check
        ),
        spec!("unity.settings", "Unity Settings", "{} → {version, grid, versions (the ones the feature table knows)}", false, unity_settings),
        spec!(
            "unity.settings.set",
            "Set Unity Settings",
            "{version? (\"6.0\"…), grid? (the Unity project enables UITK's experimental grid layout)} → the file's settings",
            true,
            unity_settings_set
        ),
        spec!(
            "unity.layer",
            "Unity Layer",
            "{id} → {position (root | relative | absolute), inAutoLayout, pivot {x, y} (0–1), pivotSet, anchorH, anchorV}",
            false,
            unity_layer
        ),
        spec!(
            "unity.layer.set",
            "Set Unity Layer",
            "{ids? (default the selection), pivot? ([x, y] or {x, y} from 0 to 1, a name such as \"center\" or \"bottomRight\", or null for the one the constraints imply), position? (\"relative\" | \"absolute\": in or out of the auto layout parent's flow)} → {ids}",
            true,
            unity_layer_set
        ),
    ]
}

fn version_of(cmd: &str, v: &Value) -> Result<UnityVersion> {
    params::f64_of(v)
        .filter(|_| !v.is_string())
        .map(|f| format!("{f}"))
        .or_else(|| v.as_str().map(str::to_string))
        .and_then(|t| UnityVersion::parse(&t))
        .ok_or_else(|| bad(cmd, "`version` must be a Unity 6 version (\"6.0\" to \"6.99\", or \"6000.x\")"))
}

fn unity_check(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "unity.check";
    let mut opts = figcraft_uitk::file_options(s.doc()?);
    if let Some(v) = get(p, "version") {
        opts.version = version_of(CMD, v)?;
    }
    if let Some(g) = params::boolean(p, "grid") {
        opts.grid_enabled = g;
    }
    let ids = params::targets(s, p);
    let roots = if ids.is_empty() { vec![s.page()?] } else { ids };
    let doc = s.doc()?;
    let issues = figcraft_uitk::check(doc, &roots, opts);
    let count = |sev: Severity| issues.iter().filter(|i| i.severity == sev).count();
    Ok(json!({
        "version": opts.version,
        "grid": opts.grid_enabled,
        "counts": {"error": count(Severity::Error), "warning": count(Severity::Warning), "info": count(Severity::Info)},
        "issues": issues,
    }))
}

fn settings_json(s: &Session) -> Result<Value> {
    let o = figcraft_uitk::file_options(s.doc()?);
    Ok(json!({"version": o.version, "grid": o.grid_enabled, "versions": UnityVersion::known().collect::<Vec<_>>()}))
}

fn unity_settings(s: &mut Session, _: &Value) -> Result<Value> {
    settings_json(s)
}

fn unity_settings_set(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "unity.settings.set";
    let mut o = figcraft_uitk::file_options(s.doc()?);
    if let Some(v) = get(p, "version") {
        o.version = version_of(CMD, v)?;
    }
    if let Some(g) = get(p, "grid") {
        o.grid_enabled = params::bool_of(g).ok_or_else(|| bad(CMD, "`grid` must be a boolean"))?;
    }
    s.doc_mut()?.unity = Some(UnitySettings { version: o.version.to_string(), grid: o.grid_enabled });
    settings_json(s)
}

fn unity_layer(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "unity.layer";
    let id = params::id(p, "id").ok_or_else(|| bad(CMD, "`id` must be a layer id"))?;
    let info = figcraft_uitk::layer_info(s.doc()?, id).ok_or_else(|| bad(CMD, format!("{id} is not a layer")))?;
    Ok(serde_json::to_value(info).unwrap_or(Value::Null))
}

/// A pivot from `[x, y]`, `{x, y}` or a name ("center", "topLeft", "bottom right", …).
fn pivot_of(v: &Value) -> Option<Pivot> {
    let share = |v: Option<&Value>| v.and_then(params::f64_of).filter(|f| (0.0..=1.0).contains(f));
    match v {
        Value::Array(a) if a.len() == 2 => Some(Pivot::new(share(a.first())?, share(a.get(1))?)),
        Value::Object(o) => Some(Pivot::new(share(o.get("x"))?, share(o.get("y"))?)),
        Value::String(name) => {
            let key: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
            let (x, y) = match key.as_str() {
                "topleft" => (0.0, 0.0),
                "top" | "topcenter" => (0.5, 0.0),
                "topright" => (1.0, 0.0),
                "left" | "centerleft" | "middleleft" => (0.0, 0.5),
                "center" | "middle" | "centercenter" => (0.5, 0.5),
                "right" | "centerright" | "middleright" => (1.0, 0.5),
                "bottomleft" => (0.0, 1.0),
                "bottom" | "bottomcenter" => (0.5, 1.0),
                "bottomright" => (1.0, 1.0),
                _ => return None,
            };
            Some(Pivot::new(x, y))
        }
        _ => None,
    }
}

fn unity_layer_set(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "unity.layer.set";
    let ids = params::req_targets(CMD, s, p)?;
    // `None`: leave it; `Some(None)`: back to the constraints' pivot.
    let pivot = match p.get("pivot") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(pivot_of(v).ok_or_else(|| bad(CMD, "`pivot`: [x, y] or {x, y} from 0 to 1, a name such as \"center\", or null"))?)),
    };
    let position = match params::string(p, "position") {
        None => None,
        Some(m) => match m.to_ascii_lowercase().as_str() {
            "relative" => Some(Position::Relative),
            "absolute" => Some(Position::Absolute),
            _ => return Err(bad(CMD, "`position` must be relative or absolute")),
        },
    };
    if pivot.is_none() && position.is_none() {
        return Err(bad(CMD, "give `pivot` or `position`"));
    }
    let doc = s.doc()?;
    for id in &ids {
        let info = figcraft_uitk::layer_info(doc, *id).ok_or_else(|| bad(CMD, format!("{id} is not a layer")))?;
        match (position, info.position) {
            (Some(_), Position::Root) => return Err(bad(CMD, format!("{id} is a top-level frame: Unity places it as the panel's root"))),
            (Some(Position::Relative), _) if !info.in_auto_layout => {
                return Err(bad(CMD, format!("{id} isn't in an auto layout frame, so it's always absolute")));
            }
            _ => {}
        }
    }
    let doc = s.doc_mut()?;
    for id in &ids {
        if let Some(pv) = pivot {
            doc.get_mut(*id)?.pivot = pv;
        }
        let in_auto_layout = figcraft_uitk::layer_info(doc, *id).is_some_and(|i| i.in_auto_layout);
        if let Some(pos) = position.filter(|_| in_auto_layout) {
            let mut props = Map::new();
            props.insert("absolute".into(), json!(pos == Position::Absolute));
            crate::cmd::props::apply(CMD, doc, *id, &props)?;
        }
    }
    Ok(json!({"ids": ids.iter().map(|i: &NodeId| json!(i)).collect::<Vec<_>>()}))
}

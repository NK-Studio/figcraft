//! FigCraft's own device format (`*.figdevice.json`): the serde form of [`DeviceProfile`] with
//! `"format": "figcraft-device"` and `"version": 1`; pixels with a top-left origin, an explicit
//! scale. This is how users add devices Unity doesn't ship.

use std::path::Path;

use serde_json::{Value, json};

use crate::{DeviceError, DeviceProfile, ParseHint, Result, ScaleSource, Source};

pub const FORMAT: &str = "figcraft-device";
pub const VERSION: u64 = 1;
/// The file name ending of FigCraft device files.
pub const EXTENSION: &str = ".figdevice.json";

/// A profile from a parsed `figcraft-device` file (validated by the caller, [`crate::parse`]).
pub fn from_value(v: Value, hint: &ParseHint) -> Result<DeviceProfile> {
    let version = v.get("version").and_then(Value::as_u64).unwrap_or(VERSION);
    if version > VERSION {
        return Err(DeviceError::Format(format!("{FORMAT} version {version} is newer than this FigCraft reads ({VERSION})")));
    }
    let mut p: DeviceProfile = serde_json::from_value(v).map_err(|e| DeviceError::Format(format!("{FORMAT}: {e}")))?;
    p.source = Source::FigCraft { file: hint.file.clone() };
    for s in &mut p.screens {
        if let Some(k) = hint.scale {
            s.scale = k;
            s.scale_source = ScaleSource::Override;
        }
        if let (Some(o), Some(dir)) = (&mut s.overlay, &hint.dir)
            && Path::new(&o.path).is_relative()
        {
            o.path = Path::new(dir).join(&o.path).to_string_lossy().into_owned();
        }
    }
    Ok(p)
}

/// `p` as `figcraft-device` JSON.
pub fn to_value(p: &DeviceProfile) -> Result<Value> {
    let mut v = serde_json::to_value(p).map_err(|e| DeviceError::Format(e.to_string()))?;
    let Some(obj) = v.as_object_mut() else { return Err(DeviceError::Format("profile is not an object".into())) };
    let mut out = serde_json::Map::new();
    out.insert("format".into(), json!(FORMAT));
    out.insert("version".into(), json!(VERSION));
    out.append(obj);
    Ok(Value::Object(out))
}

/// `p` as pretty-printed `figcraft-device` JSON text.
pub fn to_string(p: &DeviceProfile) -> Result<String> {
    serde_json::to_string_pretty(&to_value(p)?).map_err(|e| DeviceError::Format(e.to_string()))
}

//! Unity UI Toolkit (`docs/unity-uitk.md`): what a design uses that the target Unity version's
//! UXML/USS can't represent exactly. A FigCraft extension, outside the menu catalog; nothing here
//! edits the document.

use figcraft_uitk::{CheckOptions, Severity, UnityVersion};
use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::params::{self, get};
use crate::{Result, Session, bad, spec};

pub fn specs() -> Vec<CommandSpec> {
    vec![spec!(
        "unity.check",
        "Check for Unity UI Toolkit",
        "{ids? (default the selection, else the current page), version? (target Unity: \"6.0\"…, \"6000.3\"; default 6.0), grid? (the project enables UITK's experimental grid layout, 6.7+)} → {version, counts: {error, warning, info}, issues: [{id, name, severity, rule, message, since?}]}",
        false,
        unity_check
    )]
}

fn unity_check(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "unity.check";
    let version = match get(p, "version") {
        None => UnityVersion::MIN,
        Some(v) => params::f64_of(v)
            .map(|f| format!("{f}"))
            .or_else(|| v.as_str().map(str::to_string))
            .and_then(|t| UnityVersion::parse(&t))
            .ok_or_else(|| bad(CMD, "`version` must be a Unity 6 version (\"6.0\" to \"6.99\", or \"6000.x\")"))?,
    };
    let grid_enabled = params::boolean(p, "grid").unwrap_or(false);
    let ids = params::targets(s, p);
    let roots = if ids.is_empty() { vec![s.page()?] } else { ids };
    let doc = s.doc()?;
    let issues = figcraft_uitk::check(doc, &roots, CheckOptions { version, grid_enabled });
    let count = |sev: Severity| issues.iter().filter(|i| i.severity == sev).count();
    Ok(json!({
        "version": version,
        "counts": {"error": count(Severity::Error), "warning": count(Severity::Warning), "info": count(Severity::Info)},
        "issues": issues,
    }))
}

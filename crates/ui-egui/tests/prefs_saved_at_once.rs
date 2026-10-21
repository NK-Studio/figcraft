//! A change to the preferences made by a command (the control channel, MCP) is written at once,
//! not only when the app quits, so a crash doesn't lose remembered font folders.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use egui::{Context, Pos2, RawInput, Rect, vec2};
use figcraft_engine::{Prefs, Session};
use figcraft_ui_egui::{FigApp, Services};
use serde_json::json;

struct Recorder(Arc<Mutex<Vec<Prefs>>>);

impl Services for Recorder {
    fn save_prefs(&mut self, prefs: &Prefs) {
        self.0.lock().unwrap().push(prefs.clone());
    }
}

fn frame(app: &mut FigApp, ctx: &Context, n: usize) {
    let mut out = ctx.run_ui(
        RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))), time: Some(n as f64 / 60.0), ..Default::default() },
        |ui| app.ui(ui),
    );
    out.textures_delta.clear();
}

#[test]
fn a_command_that_changes_the_preferences_saves_them() {
    let saved = Arc::new(Mutex::new(Vec::new()));
    let mut app = FigApp::new(Session::new(), Box::new(Recorder(saved.clone())));
    let ctx = Context::default();
    for n in 0..3 {
        frame(&mut app, &ctx, n);
    }
    assert!(saved.lock().unwrap().is_empty(), "nothing changed, nothing written");
    // Changed between frames, as a control request or an MCP call does.
    app.session.prefs.font_folders = vec!["/fonts/game".into()];
    frame(&mut app, &ctx, 3);
    assert_eq!(saved.lock().unwrap().last().map(|p| p.font_folders.clone()), Some(vec!["/fonts/game".to_string()]));
    app.session.execute("text.fontFolders", &json!({"clear": true})).unwrap();
    frame(&mut app, &ctx, 4);
    let writes = saved.lock().unwrap().len();
    assert!(saved.lock().unwrap().last().is_some_and(|p| p.font_folders.is_empty()));
    frame(&mut app, &ctx, 5);
    assert_eq!(saved.lock().unwrap().len(), writes, "written once per change");
}

//! A tiny synthetic skeleton for tests and demos (made for FigCraft, not a Spine example export):
//! a 32×32 body region on the root bone and a 16×32 arm mesh on a child bone that the `wave`
//! animation rotates 90° over one second. The atlas has one 64×32 page, `figure.png`: the body at
//! (0, 0), the arm at (32, 0).

/// The skeleton JSON.
pub const SKELETON_JSON: &str = r#"{
  "skeleton": { "spine": "4.3.00", "x": -16, "y": 0, "width": 48, "height": 48 },
  "bones": [
    { "name": "root" },
    { "name": "arm", "parent": "root", "x": 16, "y": 16, "length": 32 }
  ],
  "slots": [
    { "name": "body", "bone": "root", "attachment": "body" },
    { "name": "arm", "bone": "arm", "attachment": "arm" }
  ],
  "skins": [
    { "name": "default", "attachments": {
      "body": { "body": { "x": 0, "y": 16, "width": 32, "height": 32 } },
      "arm": { "arm": {
        "type": "mesh", "width": 16, "height": 32, "hull": 4,
        "uvs": [0, 1, 1, 1, 1, 0, 0, 0],
        "triangles": [0, 1, 2, 2, 3, 0],
        "vertices": [0, -8, 32, -8, 32, 8, 0, 8]
      } }
    } },
    { "name": "bare", "attachments": {} }
  ],
  "animations": {
    "wave": { "bones": { "arm": { "rotate": [ { "time": 0, "value": 0 }, { "time": 1, "value": 90 } ] } } }
  }
}"#;

/// The atlas text.
pub const ATLAS: &str = "figure.png
size: 64,32
filter: Linear,Linear
body
bounds: 0,0,32,32
arm
bounds: 32,0,16,32
";

/// The atlas page's file name and size.
pub const PAGE: (&str, u32, u32) = ("figure.png", 64, 32);

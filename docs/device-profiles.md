# Device profiles and safe areas

> **Status:** P1 landed (data model, Unity `.device` importer, `device.*` commands). P2–P5 planned.
> **Owner area:** `crates/device` (L2), `crates/engine/src/cmd/device.rs` (L5).

## Goal

After laying out a screen in FigCraft, check it against real device screens: the screen size in
points, the **safe area** (status bar, notch/Dynamic Island, home indicator, Android navigation bar)
and display cutouts, per orientation. The first target is to agree with what Unity's Device
Simulator reports (`Screen.safeArea`, `Screen.cutouts`) for the same device, so a UI designed in
FigCraft lines up with the Unity build's `SafeArea` panels. Later, devices Unity doesn't ship can be
added by the user in FigCraft's own format.

The incumbent design tool has no safe-area model (frame presets are plain sizes), so this is a
FigCraft extension, not a parity item: the commands live outside the menu catalog, like
`frame.presets`.

## Licensing and assets (read first)

- Unity's `com.unity.device-simulator.devices` package is under the **Unity Companion License**,
  which only covers Unity-dependent projects. FigCraft is not one. Its `.device` files and its
  `DeviceOverlays/*.png` (pictures of other companies' hardware) are therefore **never copied into
  this repo**, neither as files, nor as generated tables, nor as test fixtures.
- FigCraft *reads* `.device` files the user already has (their own Unity install / package cache) at
  run time, the same way it opens any user file. Nothing is bundled.
- Test fixtures are synthetic `.device`-shaped JSON written for FigCraft with made-up numbers.
- Overlay images are only ever referenced by path (the user's local file); FigCraft never ships,
  embeds or redistributes them. FigCraft's own device bezels stay drawn in code (`present.rs`).
- A future built-in catalogue (P5) is authored from public platform documentation, never derived
  from Unity's files.

## Coordinate conventions

| | Unity `.device` | FigCraft `DeviceProfile` |
|---|---|---|
| Units | physical pixels | pixels stored, **points** derived (`px / scale`) |
| Origin | **bottom-left** of the screen in that orientation | **top-left** (like the canvas) |
| Safe area | `Rect{x, y, width, height}` | `Insets{top, right, bottom, left}` in px |
| Cutouts | `Rect[]` | `PxRect[]` (top-left origin) |
| Orientation | `1` Portrait, `2` PortraitUpsideDown, `3` LandscapeLeft, `4` LandscapeRight | `Orientation` enum, same four |

For an orientation of size `W×H` (landscape swaps the portrait width and height):

```
left   = x
bottom = y
right  = W - (x + width)
top    = H - (y + height)
cutout.top_left_y = H - (cy + ch)
```

Example (synthetic numbers): a 1200×2600 px portrait screen with Unity safe area
`{x:0, y:90, w:1200, h:2360}` has insets top 150, bottom 90; at scale 3 that is 50 pt / 30 pt.

### Scale (pixels per point)

Unity has no scale field and its `dpi` is not reliable for iOS, so the importer infers one, and every
import accepts an explicit `scale` override:

- **iOS / iPadOS** (`systemInfo.operatingSystem` starts with `iOS`/`iPadOS`, or the model/name says
  iPhone/iPad/iPod): short side ≥ 1536 px (iPad) → 2; short side ≤ 828 px → 2; the 1080×1920
  "Plus" panel → 1080/414 (its 414×736 pt layout); otherwise → 3.
- **Android**: `dpi / 160` (dp), when `dpi` is finite and in 80..=1000; otherwise 1.
- **Anything else**: 1.

The inferred value and how it was chosen (`scaleSource`: `inferred-ios`, `inferred-android`,
`default`, `override`, `file`) are reported so a wrong guess is visible.

## Data model (`crates/device`)

```rust
DeviceProfile {
    id: String,              // slug, unique in the registry ("unity/apple-iphone-12")
    name: String,            // friendlyName
    platform: Platform,      // Ios | Android | Other
    source: Source,          // UnityDevice { file } | FigCraft { file } | Inline
    model: Option<String>,   // systemInfo.deviceModel
    os: Option<String>,      // systemInfo.operatingSystem
    screens: Vec<Screen>,    // ≥ 1 (foldables have 2)
}
Screen {
    width_px, height_px: u32,  // portrait, native
    dpi: Option<f64>,
    scale: f64, scale_source: ScaleSource,
    navigation_bar_px: f64,    // Android nav bar (Unity navigationBarHeight)
    corner_radius_px: f64,
    orientations: Vec<OrientationInfo>,  // only the ones the device supports
    overlay: Option<Overlay { path, border_px: Insets }>,  // reference only
}
OrientationInfo { orientation, safe_area: Insets, cutouts: Vec<PxRect> }
```

Queries: `Screen::size_px(o)`, `size_pt(o)`, `safe_area_px(o) -> PxRect`, `safe_area_pt(o)`,
`insets_pt(o)`, `cutouts_pt(o)`.

### Validation (never crash)

Parsing returns `Result<_, DeviceError>`; nothing panics. A file is refused when it is over 1 MiB,
isn't JSON, has no screens, has more than 8 screens or 8 orientations per screen or 64 cutouts,
a screen side outside 1..=32768 px, a non-finite or negative number, or a safe area/cutout that
doesn't fit in the screen (1 px tolerance; values are clamped into the screen after that). A
leading UTF-8 BOM is accepted. Unknown fields are ignored (`systemInfo.graphicsDependentData` etc.).

### FigCraft's own format (extensibility, P1 read/write, P5 catalogue)

`*.figdevice.json`: the serde form of `DeviceProfile` with `"format": "figcraft-device"` and
`"version": 1`, insets in px, top-left origin, explicit `scale`. This is how users add devices Unity
doesn't ship (new iPhones, tablets, custom hardware): write one by hand, or `device.export` an
imported profile and edit it. Overlay paths in it are resolved relative to the file.

## Engine (`Session::devices`)

A per-session `DeviceRegistry` (ordered map id → profile). P1 keeps it in memory for the session;
persisting the user's imported devices (prefs) is P2.

| Command | Params | Result |
|---|---|---|
| `device.import` | `{path}` (a `.device`, a `.figdevice.json`, or a folder: its `*.device`/`*.figdevice.json` files and those in its `Editor/Devices` and `Devices` sub-folders) **or** `{text, name?}`; `scale?`, `replace?` (default true) | `{imported: [id], skipped: [{file, error}]}` |
| `device.list` | `{platform?}` | `[{id, name, platform, screens: [{widthPx, heightPx, scale, widthPt, heightPt, orientations}]}]` |
| `device.get` | `{id, orientation? ("portrait"…/1–4), screen? (0)}` | size, safe area and insets in px and pt, cutouts, scale, overlay |
| `device.export` | `{id, path?}` | the profile as `figcraft-device` JSON (written to `path` when given) |
| `device.remove` | `{id}` / `{all: true}` | `{removed: n}` |

None of them edit the document, so none is an undo step. File access goes through
`file_access` (automation roots apply). Folder imports read at most 512 files.

## Phases

- **P1 — data model + Unity importer** (this change): `crates/device`, `device.*` commands, tests
  with synthetic fixtures, junk-parameter sweep coverage.
- **P2 — frames know their device**: `frame.device.set {id, device, orientation}` stores a device
  reference on a frame; the canvas draws the safe area, cutouts and corner radius over that frame
  (toggle in View); switching orientation rotates the frame; registry persisted in prefs.
- **P3 — multi-device check**: resize a frame (constraints + auto layout re-run) to each chosen
  device side by side; "outside safe area" warnings for layers whose bounds cross the insets; safe
  area as layout guides.
- **P4 — Unity parity harness** (local, `plan/`): drive Unity's Device Simulator through its MCP/CLI
  for each device and orientation, dump `Screen.safeArea`/`Screen.cutouts`/`Screen.width/height`,
  and diff against `device.get` (px exact, pt within 0.5). Also a CanvasScaler emulation note:
  Unity UI uses reference resolution + match width/height, which differs from point-based layout.
- **P5 — beyond Unity**: a built-in catalogue in `figcraft-device` format authored from public
  platform docs (current iPhones/iPads/Android classes), user folders of `*.figdevice.json`, and
  user-chosen overlay images (local paths, never shipped).

## Open questions

- Unity's `presentation.borderSize` is read as `x=left, y=top, z=right, w=bottom`; to confirm in P4.
- Unity's Android safe areas don't take out the navigation bar: in the files checked, they only
  take out the cutout at the top (`navigationBarPx` is kept separately). Whether the simulator
  takes the bar out of `Screen.safeArea` at run time is checked in P4.

# Device profiles and safe areas

> **Status:** P1 landed (data model, Unity `.device` importer, `device.*` commands); P2 landed
> (device frames: safe area and cutouts on the canvas); P3a landed (device preview: one device,
> without touching the file); P5a landed (built-in generic devices). P3b, P4 and P5b planned.
> **Owner area:** `crates/device` (L2; the built-in devices in `catalog.rs`), `DeviceFrame` in `crates/doc/src/node.rs` (L1),
> `crates/engine/src/cmd/device.rs` and `device_sim.rs` (L5), `crates/ui-egui/src/safe_area.rs`,
> `device_bezel.rs` and `device_preview.rs` (L6).

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
- The built-in devices ([Built-in devices](#built-in-devices-p5a)) are generic ones written for
  FigCraft from what the platforms publish about their screens, never derived from Unity's files:
  they don't claim to match a Unity device of a similar name. Import the user's own `.device`
  file for that.

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
  "Plus" panel → 1080/414 (its 414×736 pt layout); the 1080×2340 "mini" panel → 1080/375 (375×812
  pt); otherwise → 3.
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

A per-session `DeviceRegistry` (ordered map id → profile), which starts with the built-in devices
(`figcraft/…`, `source: "builtin"`); imported ones follow them. Paths imported with
`device.import {path}` (without `scale`) are remembered in the preferences (`deviceSources`, at
most 32) and imported again when the app starts; ones that are gone are skipped quietly, and
`device.remove {all: true}` forgets them.

| Command | Params | Result |
|---|---|---|
| `device.import` | `{path}` (a `.device`, a `.figdevice.json`, or a folder: its `*.device`/`*.figdevice.json` files and those in its `Editor/Devices` and `Devices` sub-folders) **or** `{text, name?}`; `scale?`, `replace?` (default true) | `{imported: [id], skipped: [{file, error}]}` |
| `device.list` | `{platform?}` | `[{id, name, platform, screens: [{widthPx, heightPx, scale, widthPt, heightPt, orientations}]}]` |
| `device.get` | `{id, orientation? ("portrait"…/1–4), screen? (0)}` | size, safe area and insets in px and pt, cutouts, scale, overlay |
| `device.export` | `{id, path?}` | the profile as `figcraft-device` JSON (written to `path` when given) |
| `device.remove` | `{id}` / `{all: true}` | `{removed: n}`: imported devices only. A built-in device is refused by id and stays after `all`; removing an import that took a built-in device's id brings that one back |
| `frame.device.set` | `{ids?, device, orientation?, screen?, resize? (true)}` | `{ids, device, orientation, width, height}`: the frames take the device's size in points and keep a `DeviceFrame` |
| `frame.device.clear` | `{ids?}` | `{cleared: n}` (`node.set {device: null}` does the same beside other properties, in their step) |
| `view.safeAreas` | `{on?}` | View ▸ Safe areas (on by default; kept in the preferences like the other View toggles) |
| `device.simulate` | `{frame?, device, orientation?, screen?, path? (a `.png`), scale? (0.1–4)}` | `{frame, device, name, orientation, screen, width, height, safeArea, cutouts, warnings: [{id, name, kind, edges, box}], path?}`: the frame as `frame.device.set` would leave it, worked out on a copy |
| `device.preview` | `{frame?, device?, orientation?, screen?}` | opens the device preview pane (`{frame, device?, orientation?, screen?}`) |
| `device.closePreview` | `{}` | closes it |

The `device.*` commands (the preview's too) don't edit the document, so none is an undo step; `frame.device.set` and
`frame.device.clear` are (one step each, resize included). File access goes through
`file_access` (automation roots apply). Folder imports read at most 512 files.

## Device frames (P2)

A frame stands for a device screen through `Node::device: Option<DeviceFrame>` (saved with the
file and synced in live sessions like any other layer property). `DeviceFrame` is a **snapshot in
points** taken from the profile when the device is chosen: id, name, orientation, the screen's
size, safe-area insets, cutouts and corner radius. So a file shows its safe areas on any machine,
without the profile or Unity installed.

- **Canvas** (`safe_area.rs`, while View ▸ Safe areas is on): the bands outside the safe area are
  tinted with the theme's danger colour, the safe area's edge is dashed, cutouts are filled and the
  screen's rounded corners are outlined. It's drawn in the frame's own coordinates, so it turns with
  a rotated frame. Insets stay in points from each edge when a frame is resized to another size;
  cutouts scale with the frame. Every stored number is checked (finite, clamped into the frame,
  at most 64 cutouts) before drawing: a file can say anything.
- **Device body** (`device_bezel.rs`, View ▸ Device bezels, `view.deviceBezels`, on by default): the
  phone or tablet drawn round the frame in code, in the presenter's style — a graphite slab with a
  rim and black glass, side buttons on phones, the screen's real corner radius masking the frame's
  corners, and the device's cutouts (notch, island, hole punch) in black. It's drawn only outside
  the screen (and in its corners and cutouts), and the frame's label moves above it. Turned frames
  have none. No device pictures are used.
- **Right panel**: the frame type dropdown lists the imported devices first under *Devices* (their
  portrait size), then *Clear device* when a selected frame has one, and *Import devices…* (a
  folder picker; the folder is remembered); the size presets follow under *Sizes only (not a
  device)*: they set the frame's size and drop its device (`node.set {width, height, device:
  null}`, one undo step), so a frame turned into a desktop screen or an A4 page loses the phone's
  body, notch and safe area.
- **Changing the device**: layers inside the safe area are placed against the new device's safe
  area by their constraints, as Unity's SafeArea container places them; other layers follow the
  frame (`DeviceFrame::holds`). A layer flush with a frame edge bleeds (backgrounds), unless it also
  lies on an inset edge of the safe area (a window a SafeArea component sizes, under the notch).
  A layer placed on the safe area this way is marked (`Node::follows_safe_area`, `followsSafeArea`
  in `node.set`/`node.get`; *Follows the safe area* in the Design tab's Unity section for children
  of device frames): on a device without a safe area (iPod touch) it fills the screen like a
  background, and the mark brings it back onto the safe area on the next device that has one
  (`DeviceFrame::follows`: holds, or marked and within the safe area). A marked layer moved out of
  the safe area loses the mark at the next change. Files from before the mark keep working; a
  window saved while filling such a screen needs the checkbox once. With a canvas scaler on the
  frame (`docs/unity-uitk.md` › UI scaling) everything scales first.
- **Orientation**: `frame.swapOrientation` turns a device frame's snapshot too (portrait, else
  upside down ⇄ landscape left, else right) when the profile is imported; otherwise it drops the
  device with a message, since the turned safe area isn't known.

## Built-in devices (P5a)

So a design can be checked before anything is imported, every session starts with eleven generic
devices (`crates/device/src/catalog.rs`). They are device *classes*, not models: common screen
sizes with round safe areas and the usual cutouts, in portrait and both landscapes (tablets and
the foldable's inner screen upside down too). A cutout turns with the device; a home indicator
stays at the bottom.

| Id (`figcraft/…`) | Name | Screen px (pt) | Safe area, portrait | Landscape |
|---|---|---|---|---|
| `notch-phone` | Notch Phone | 1170 × 2532 (390 × 844) | 47 pt top, 34 pt bottom | 47 pt each side, 21 pt bottom |
| `notch-phone-large` | Notch Phone Large | 1284 × 2778 (428 × 926) | 47 / 34 pt | 47 pt sides, 21 pt bottom |
| `notch-phone-small` | Notch Phone Small | 1080 × 2340 (375 × 812.5) | 50 / 34 pt | 50 pt sides, 21 pt bottom |
| `classic-phone` | Classic Phone | 750 × 1334 (375 × 667) | none | none |
| `punch-hole-phone-left` | Punch Hole Phone (Left) | 1080 × 2340 (411 × 891) | 120 px top | 120 px on the hole's side |
| `punch-hole-phone-center` | Punch Hole Phone (Center) | 1440 × 3088 (461 × 988) | 140 px top | 140 px on the hole's side |
| `punch-hole-phone-right` | Punch Hole Phone (Right) | 1080 × 2280 (411 × 869) | 120 px top | 120 px on the hole's side |
| `pill-cutout-phone` | Pill Cutout Phone | 1200 × 2640 (427 × 939) | 140 px top | 140 px on the pill's side |
| `large-tablet` | Large Tablet | 2048 × 2732 (1024 × 1366) | 20 pt bottom | 20 pt bottom |
| `small-tablet` | Small Tablet | 1536 × 2048 (768 × 1024) | none | none |
| `foldable-phone` | Foldable Phone | cover 1080 × 2520, inner 1840 × 2208 | 120 px top (each) | 120 px on the hole's side |

They behave like imported devices everywhere (`device.list`, `device.get`, `frame.device.set`,
the frame type dropdown, the device preview), and can be exported (`device.export`), edited and
imported under another id, or under the same id to stand in for the built-in one. Each is built
through the checks an imported file gets (`validate`), and a test holds every cutout outside its
safe area in every orientation.

## Device preview (P3a)

Checking a design on another device shouldn't mean changing the design. The device preview shows
a top-level frame as it would be on an imported device, on a copy of the document:

- **One code path.** `figcraft_engine::cmd::device_sim::simulate` copies the document and runs the
  steps of `frame.device.set` on the copy (`put_on_device`: the frame's UI scaling, the resize
  with its constraints and auto layout, the layers that follow the safe area), then normalises it.
  A test holds the two to the same boxes, with and without a canvas scaler, in portrait and
  landscape. The file, its selection and its undo history are untouched.
- **Warnings** (`device_sim::warnings`), top-most parents first, at most 200:
  - `offscreen`: a layer sticks out of the screen on a side where it was inside the frame as
    designed (not along an axis the frame scrolls, `overflow`);
  - `safeArea`: a layer crosses the safe area's edge without reaching the screen's edge on that
    side. A layer that reaches it is a background (it bleeds, as `DeviceFrame::holds` reads it),
    and the layers in it are checked instead; text never counts as a background.

  They are advice: a status bar drawn into the design is reported, and so is a list item cut by
  the home indicator.
- **`device.simulate`** answers the result as data for scripts, the CLI and MCP, and with `path`
  draws the frame into a PNG (the laid-out frame only: the safe area and the body are the UI's).
- **The pane** (`device.preview`; the Present button's menu ▸ Device preview; View ▸ Device
  preview): a floating window over the canvas with the device's body, safe area and cutouts drawn
  as on the canvas (View ▸ Safe areas and Device bezels apply). Its bar picks the device
  (*Import devices…* is there too), turns it through the orientations the screen supports and
  steps through the screens of a device with several (a foldable's cover and inner screens). The
  layers with a warning are outlined; hovering one says why, a click selects it, and the count in
  the status line selects them all. It follows edits live and shows the top-level frame selected
  last; the device chosen is kept while the app runs. `ui.inspect` reports it under
  `devicePreview`.

Not yet: several devices side by side (P3b), a fold's hinge (the profile has no field for it), and
playing a prototype on the device (the presenter keeps its own device list). With nothing
imported the pane starts on the first built-in device.

## Phases

- **P1 — data model + Unity importer** (landed): `crates/device`, `device.*` commands, tests
  with synthetic fixtures, junk-parameter sweep coverage.
- **P2 — frames know their device** (landed): see [Device frames](#device-frames-p2).
- **P3a — device preview** (landed): see [Device preview](#device-preview-p3a): one device at a
  time on a copy of the document, with warnings for layers that cross the safe area or leave the
  screen.
- **P3b — multi-device check**: the same preview for several chosen devices side by side (saved
  sets, one switch for all orientations); a fold's hinge in the `figcraft-device` format with a
  warning for layers across it; `DeviceFrame` remembers which screen it stands for; safe area as
  layout guides.
- **P4 — Unity parity harness** (local, `plan/`): drive Unity's Device Simulator through its MCP/CLI
  for each device and orientation, dump `Screen.safeArea`/`Screen.cutouts`/`Screen.width/height`,
  and diff against `device.get` (px exact, pt within 0.5). Also a CanvasScaler emulation note:
  Unity UI uses reference resolution + match width/height, which differs from point-based layout.
- **P5a — built-in devices** (landed): see [Built-in devices](#built-in-devices-p5a).
- **P5b — beyond Unity**: user folders of `*.figdevice.json`, user-chosen overlay images (local
  paths, never shipped), and more built-in classes as the platforms' screens change.

## Open questions

- Unity's `presentation.borderSize` is read as `x=left, y=top, z=right, w=bottom`; to confirm in P4.
- Unity's Android safe areas don't take out the navigation bar: in the files checked, they only
  take out the cutout at the top (`navigationBarPx` is kept separately). Whether the simulator
  takes the bar out of `Screen.safeArea` at run time is checked in P4.

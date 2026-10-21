# Spine skeletons

> **Last updated:** 2026-10-21 · **Change:** minor (the Spine hosts ship in the macOS and Windows packages; Spine 4.2 and 4.3 through the official spine-cpp in a host process; imports take the export's folder only; Spine 4.3 skeletons: import, posing, presentation playback, `.figcraft` storage and Unity export)

FigCraft places [Spine](https://esotericsoftware.com/) 4.2 and 4.3 skeleton exports as layers, poses them
at an animation, skin and time, plays them in presentation and carries them into Unity exports. A
FigCraft extension: Figma has no equivalent, and nothing here is in the menu catalog.

## Runtimes

Two runtimes pose skeletons, behind `crates/spine` (`Asset`, `Runtime`):

- **The Spine host** (`tools/spine-host`): the official
  [spine-cpp](https://github.com/EsotericSoftware/spine-runtimes) runtime of the file's version in
  a process of its own, `figcraft-spine-host-4.2` or `-4.3`, one process per loaded skeleton
  (`crates/spine/src/host.rs` drives it over pipes). It is used whenever the file's version has a
  host. Spine 4.2 files need it.
- **The built-in runtime**: [`spine2d`](https://crates.io/crates/spine2d) 0.5 (unofficial, pure
  Rust, 4.3 only) in the app's process, for 4.3 files when no 4.3 host is installed.

`spine.info` reports which one (`runtime`: `spine-cpp` or `spine2d`). The version comes from the
file's header (`skeleton.spine` in JSON, the string after the hash in `.skel`); other versions are
refused with a message to export for 4.2 or 4.3.

**Why a separate process.** spine-cpp trusts its input: a damaged `.skel` can crash or hang it
(a fuzz of the 4.2 runtime's transpiled C build crashed or hung on 30 of 30 damaged files). In its
own process that only ends the host: a crash, a garbled answer or a request past its deadline (10 s
to load, 3 s to pose) is an error for that layer, the host is started again for the next pose, and
after three starts the skeleton stays unavailable. Nothing of spine-cpp runs in FigCraft's process,
and no `unsafe` Rust is involved. The last pose of each skeleton is cached, so a canvas drawing a
layer tile by tile asks the host once.

**Building the hosts.** spine-cpp is under the Spine Runtimes License, so it never enters this
repository. `cargo xtask spine-host` builds both hosts with CMake and a C++ compiler from one
checkout per version in `SPINE_RUNTIMES_DIR` (default `../spine-runtimes`: `4.2/` and `4.3/`;
`--fetch` makes sparse clones of the official repository there) and copies them beside the debug
and release builds. The app looks for them in `FIGCRAFT_SPINE_HOST_DIR`, beside its executable, or
one folder up. Without them everything builds, tests and runs: 4.3 files use the built-in runtime,
4.2 files are refused with a message naming the host.

**In release packages.** `packaging/macos/package.sh` and `packaging/windows/package.ps1` build
the hosts for the package's architecture (`cargo xtask spine-host --out DIR --tag NAME`; CMake
reads `CMAKE_OSX_ARCHITECTURES` / `CMAKE_GENERATOR_PLATFORM`) and ship them beside the app with the
Spine Runtimes License (`LICENSE-spine-runtimes.txt`, which redistribution must include):

| Package | Hosts | Licence |
|---|---|---|
| macOS `FigCraft.app` | `Contents/MacOS/` (signed, hardened runtime, `ai.storyteller.figcraft.spine-host-4.x`) | `Contents/Resources/Licenses/` |
| macOS CLI zip | beside `figcraft-cli` (signed) | beside it |
| Windows MSI | the install folder, beside `figcraft.exe` (signed) | the install folder |
| Windows portable zip | beside `figcraft.exe` | beside it |

Without the spine-runtimes checkouts the scripts warn and package without hosts.

## Licensing

- Both runtimes come under the
  [Spine Runtimes License](https://esotericsoftware.com/spine-runtimes-license) (not an OSI
  licence): spine-cpp is Esoteric Software's, and spine2d's own sources say parts are ported from
  spine-cpp. **Everyone who uses Spine data with FigCraft needs their own Spine Editor licence**, as
  with any Spine runtime.
- `tools/spine-host/main.cpp` and `CMakeLists.txt` are FigCraft's own (MIT OR Apache-2.0); the
  hosts built from them include spine-cpp and carry its licence.
- The `runtime` feature of `figcraft-spine` (default on) is the only thing that pulls spine2d in.
  Without it 4.3 files need the host too.
- No Spine example exports are in the repository. Tests use a synthetic skeleton written for
  FigCraft (`crates/spine/src/fixture.rs`: a body region and an arm mesh, one `wave` animation);
  the host tests run when a host is built and are skipped otherwise.

## Files and import

A Spine export imports **as its folder**: the folder holding the skeleton (`<name>.json`, binary
`<name>.skel`, or Unity's `<name>.skel.bytes`), its atlas (`<name>.atlas` or `<name>.atlas.txt`)
and the atlas page images the atlas names. Choosing one file alone isn't accepted: it wouldn't
bring the atlas and pages with it.

- *Import Spine Folder…* (the Actions palette; `spine.place` with no folder) asks for the folder.
- `file.import {path}` with a folder places the skeleton in it. A single `.skel`, `.atlas` or Spine
  `.json` is refused with a message to choose the folder; other `.json` files import as FigCraft
  or REST API documents as before.
- A folder with several skeletons needs `skeleton` (the name). The atlas is the one with the
  skeleton's name, or the folder's only atlas.
- Dropping a folder on the canvas loads it as a font folder (font work), not as Spine.

| Command | Params → result |
|---|---|
| `spine.place` | `{dir, skeleton? (name, when the folder has several), name?, x?, y? (top-left; default centred in the view), parent?, animation?, skin?}`, or data instead of `dir`: `{skeleton (base64), binary?, atlas (text), pages ({file: base64})}` → `{id, asset, width, height, animations, skins}`; with neither, the UI asks for the folder → `{placed: false}` |
| `spine.set` | `{ids?, animation? (name, or null: setup pose), skin? (name, or null: default skin), time? (s), looping?, playing?, speed?}` → `{ids}` (undoable; unknown names are errors) |
| `spine.info` | `{id?}` → `{id, asset, name, version, animations: [{name, duration}], skins, pages, animation, skin, time, looping, playing, speed}` |

Atlas page names must be files inside the folder (no absolute paths or `..`). Files are read
through the automation file roots like every other import.

## Layers

A Spine layer (`NodeKind::Spine`, type name `SPINE`) holds the skeleton's key, the animation, skin,
time, looping, playing and speed, and `frame`: the skeleton-space rectangle mapped onto the layer's
box (the setup pose's bounds when placed, one skeleton unit per point). Resizing the layer scales
the skeleton. Fills, strokes and effects on it are ignored; opacity, blend mode, visibility and
masks work as on any layer.

The Design tab's **Spine** section picks the animation (or *Setup pose*) and the skin, and sets
time, speed, *Loop* and *Play in presentation*.

## Drawing

`crates/render/src/spine.rs` poses the skeleton (`figcraft_spine::Asset::pose`) and draws each
triangle with its atlas page as a pattern under the affine map from its UVs to its corners.
Triangles under the same map fill as one path, and every edge moves half a device pixel outward
(corners cut flat) so anti-aliased edges between neighbours leave no seams. Blend modes: normal, additive (plus), multiply and screen.

- On the canvas and in exports a layer shows its pose at `time`. In presentation a playing layer
  advances at `speed` from `time` (the presenter's clock, 60 frames a second, the same path as
  animated GIFs).
- Physics: the host simulates physics constraints from the start of the animation in steps of
  1/30 s (at most 60 s of it), so a pose is still a function of its time; the built-in runtime
  doesn't simulate physics.
- Vertex tint is applied as opacity only; RGB tint and two-colour (dark) tint are not drawn yet.
  Premultiplied-alpha pages are treated as straight alpha.
- Animations can reach past the setup-pose box: the layer's paint bounds are its box grown by its
  larger side so it isn't culled.

## Storage

In a `.figcraft` archive: `spine/index.json` (`{"skeletons": [{key, name, binary, pages: {file:
imageHash}}]}`), `spine/<key>/skeleton` and `spine/<key>/atlas`; atlas pages are ordinary `images/`
members. The key is the SHA-256 of the skeleton and atlas, so two layers from one export share it.
A damaged index or member is reported on loading and skipped. Version history and branch
snapshots don't keep skeletons of their own yet. `.fig` export writes a Spine layer as an empty
rectangle, with a warning.

## Unity export

`file.exportUitk` writes, for each frame with Spine layers:

```
<dir>/<Frame>.spine.json                 {"skeletons": [{id, name, class, skeleton, atlas, animation, skin, time, looping, playing, speed}]}
<dir>/spine/<name>/<name>.json           the skeleton (binary: <name>.skel.bytes, as spine-unity expects)
<dir>/spine/<name>/<name>.atlas.txt      the atlas
<dir>/spine/<name>/<page>.png            its pages, under the atlas's names
<dir>/images/baked-<id>.png              the layer's current pose, the element's background
```

The element is a `ui:VisualElement` named after the layer with the classes `<block>__<id>` and
`spine-skeleton`. Hook it up in Unity with the spine-unity runtime (its UI Toolkit package is
experimental and draws no blend modes); the manifest says which skeleton, animation and skin each
element shows. `unity.check` reports every Spine layer as an info issue (`spine`).

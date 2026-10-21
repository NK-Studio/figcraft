# Spine skeletons

> **Last updated:** 2026-10-21 · **Change:** new (Spine 4.3 skeletons: import, posing, presentation playback, `.figcraft` storage and Unity export)

FigCraft places [Spine](https://esotericsoftware.com/) 4.3 skeleton exports as layers, poses them
at an animation, skin and time, plays them in presentation and carries them into Unity exports. A
FigCraft extension: Figma has no equivalent, and nothing here is in the menu catalog.

## Licensing

- The runtime is [`spine2d`](https://crates.io/crates/spine2d) 0.5 (MIT OR Apache-2.0, unofficial,
  pure Rust, `#![forbid(unsafe_code)]`), wrapped by `crates/spine` so it can be swapped. Its own
  sources say parts are ported from the official spine-cpp runtime, which is under the
  [Spine Runtimes License](https://esotericsoftware.com/spine-runtimes-license) (not an OSI
  licence). Treat it accordingly: **everyone who uses Spine data with FigCraft needs their own Spine
  Editor licence**, as with any Spine runtime.
- The `runtime` feature of `figcraft-spine` (default on) is the only thing that pulls it in.
  Without it skeletons still load and save as data, and every pose call returns
  `SpineError::Unavailable` (nothing draws).
- No Spine example exports are in the repository. Tests use a synthetic skeleton written for
  FigCraft (`crates/spine/src/fixture.rs`: a body region and an arm mesh, one `wave` animation).

## Files and import

A Spine export is a skeleton (`<name>.json` or binary `<name>.skel`), its atlas (`<name>.atlas`)
and the atlas page images the atlas names. `file.import` (and dropping a file on the canvas) takes
any of the skeleton or atlas and finds the others beside it; a `.json` goes to Spine only when it
has a top-level `skeleton` object and `bones` (otherwise it is a FigCraft or REST API document as
before).

| Command | Params → result |
|---|---|
| `spine.place` | `{path (.json/.skel/.atlas) \| skeleton (base64) + binary? + atlas (text) + pages ({file: base64}), atlas? (path), name?, x?, y? (top-left; default centred in the view), parent?, animation?, skin?}` → `{id, asset, width, height, animations, skins}` |
| `spine.set` | `{ids?, animation? (name, or null: setup pose), skin? (name, or null: default skin), time? (s), looping?, playing?, speed?}` → `{ids}` (undoable; unknown names are errors) |
| `spine.info` | `{id?}` → `{id, asset, name, version, animations: [{name, duration}], skins, pages, animation, skin, time, looping, playing, speed}` |

Atlas page names must be files inside the atlas's folder (no absolute paths or `..`). Files are
read through the automation file roots like every other import.

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
triangle with its atlas page as a pattern under the affine map from its UVs to its corners, grown
half a device pixel against seams. Blend modes: normal, additive (plus), multiply and screen.

- On the canvas and in exports a layer shows its pose at `time`. In presentation a playing layer
  advances at `speed` from `time` (the presenter's clock, 60 frames a second, the same path as
  animated GIFs).
- Physics constraints are not simulated (a pose is a pure function of its time).
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

# Unity UI Toolkit: structure mapping, compatibility and sync

> **Status:** U1 landed (compatibility check: `figcraft-uitk`, `unity.check`); U2 landed (file
> settings, layer position and pivot, the right panel's Unity section); U3 landed (UXML/USS
> export, version-aware conversions, SafeArea container, Unity scripts); U4 run on Unity 6.6
> (layout matches on 5 devices, text aside). U5 planned.
> **Owner area:** `crates/uitk` (L2), `UnitySettings` / `Pivot` in `crates/doc` (L1),
> `crates/engine/src/cmd/unity.rs` (L5), `crates/ui-egui/src/unity_panel.rs` (L6).
> **Related:** [`device-profiles.md`](device-profiles.md) (devices, safe areas, device frames).

## Goal

A screen laid out in FigCraft for each device (device frames, safe areas: `device-profiles.md`)
must behave **the same** once taken to Unity UI Toolkit (UITK): every element at the same place and
size, in points, on every device and orientation. The end goal is a "sync to Unity" feature; this
doc fixes the model it rests on:

1. which FigCraft structures map to which UXML/USS, per Unity version;
2. what can't be represented (checked and reported per layer before anything is exported);
3. the coordinate, scale and safe-area rules both sides follow;
4. how "the same" is measured (a harness that runs the export in Unity and compares rectangles).

**Supported Unity versions: 6.0 and later.** Every rule below is keyed by the target version.

## UITK features by version

From Unity's manuals and release posts (see Sources). 6.7 was in beta when this was written; the
table is rechecked when it ships.

| Feature | Unity | Notes |
|---|---|---|
| Flexbox (Yoga), `position: absolute`, `left/top/right/bottom`, `%` units, border-box sizing | 6.0 | default `flex-direction: column`, `flex-shrink: 1`, `align-items: stretch` |
| `translate` in `%` of the element's own size, `transform-origin` (default centre) | 6.0 | transforms don't change other elements' layout |
| `overflow: hidden` (rounded clipping), `border-*-radius` (clamped to half the size) | 6.0 | no elliptical radii |
| World-space UI | 6.2 | not used here |
| `filter` (blur, grayscale, tint, …) | 6.3 | |
| `aspect-ratio` | 6.3 | |
| SVG as a core module (Vector Images), `Image` in UXML | 6.3 | before: the Vector Graphics package |
| UI Shader Graph (URP) | 6.3 | escape hatch for effects |
| `-unity-text-auto-size` | 6.3 (roadmap post; unconfirmed) | |
| `PanelRenderer` replaces `UIDocument` | 6.5 | the Unity-side setup differs |
| `filter: drop-shadow()` | 6.6 | follows the element's silhouette; **no spread** (not CSS `box-shadow`) |
| `backdrop-filter` | 6.6 | background blur |
| `gap` between flex children | 6.7 | the 6.7 manual lists `row-gap`/`column-gap` only under grid: **to confirm in U4** |
| `linear-gradient()`, `radial-gradient()` on `background-image` | 6.7 | no angular/diamond |
| `z-index` | 6.7 | |
| CSS Grid (`display: grid`, `grid-template-*`, `row-gap`, `column-gap`) | 6.7, **experimental** (Project Settings ▸ UI Toolkit ▸ Enable Grid Layout) | no named lines, areas, subgrid |
| `box-shadow`, soft masks, media queries, `vh`/`vw` | not up to 6.7 | |

## Coordinates, units and scale

- Both sides measure from the **top-left**, y down. UITK uses border-box sizing, like FigCraft
  frames with strokes inside.
- **1 UITK px = 1 FigCraft point = 1 device point.** A FigCraft device frame is the device screen in
  points; in Unity the panel's scale is set so the panel is the same size in its own px:
  `panelSettings.scale = Screen.width / frameWidthPt` (equivalently, the device scale factor). The
  Scale With Screen Size / reference resolution modes don't give per-device equality and aren't the
  sync target (they can still be used for other UI).
- **Safe area:** UITK doesn't apply it. The export wraps the content of a device frame in a
  *SafeArea container* whose padding is `Screen.safeArea` converted to panel coordinates
  (`RuntimePanelUtils.ScreenToPanel`), which is exactly what FigCraft draws on device frames.
  Layers that stand outside it in FigCraft (backgrounds that bleed under the status bar) stay
  outside it in the export.

## Structure mapping

| FigCraft (Figma model) | UITK | Version |
|---|---|---|
| Frame / group / component / instance | `VisualElement` (instances later as UXML templates) | 6.0 |
| Auto layout horizontal / vertical | `flex-direction: row / column` | 6.0 |
| Wrap | `flex-wrap: wrap` (+ `align-content`) | 6.0 |
| Main align min / centre / max / space-between / around / evenly | `justify-content` | 6.0 (`space-evenly`: to confirm in U4) |
| Cross align min / centre / max | `align-items` | 6.0 |
| Cross align **baseline** | — | ✗ |
| Padding | `padding-*` | 6.0 |
| Gap (and cross gap when wrapping) | 6.7+: `gap`; earlier: margins on the children | see table |
| Hug / Fill / Fixed | `auto` / `flex-grow: 1; flex-basis: 0` / `width`/`height` + `flex-shrink: 0` | 6.0 |
| Min / max sizes | `min-*` / `max-*` | 6.0 |
| Absolute child in auto layout | `position: absolute` | 6.0 |
| Grid auto layout | 6.7 with the experimental flag: `display: grid`; earlier ✗ | |
| Constraints left / right / left & right | `left` / `right` / both (stretch) | 6.0 |
| Constraints centre | `left: 50%` + `translate: -50%` + offset (the pivot rule below) | 6.0 |
| Constraints scale | `left` and `width` in `%` | 6.0 |
| Clip content | `overflow: hidden` | 6.0 |
| Solid fill | `background-color` | 6.0 |
| Image fill fill / fit | `background-image` + `-unity-background-scale-mode` scale-and-crop / scale-to-fit | 6.0 |
| Linear / radial gradient | `background-image: linear-/radial-gradient()`; earlier: baked image | 6.7 |
| Inside stroke (per side too) | `border-*-width` + `border-*-color` | 6.0 |
| Corner radii | `border-*-radius` | 6.0 |
| Opacity | `opacity` | 6.0 |
| Rotation | `rotate` about `transform-origin` | 6.0 |
| Drop shadow | `filter: drop-shadow()`; earlier: baked image | 6.6 |
| Layer blur | `filter: blur()` | 6.3 |
| Background blur | `backdrop-filter: blur()` | 6.6 |
| Text | `Label` + `font-size`, `-unity-font-definition`, `color`, `letter-spacing`, `-unity-text-align`, `white-space`, `text-overflow` | 6.0 |
| Vector shapes, booleans | Vector Image (SVG); earlier: baked image | 6.3 |

### Pivot (absolute layers)

UITK has no pivot property; a pivot is a combination that works from 6.0:

- the **anchor** is where in the parent the layer is attached: `left`/`top` in `%` (0%, 50%, 100%)
  or `right`/`bottom`;
- the **pivot** is which point of the layer sits on the anchor: `translate: -px% -py%`;
- the offset is added in px (`margin-left`/`margin-top`, or folded into `left`/`top` with `calc`-free
  arithmetic since USS has no `calc`).

FigCraft's constraints give the anchor; U2 adds a per-layer **pivot (0–1, 0–1)** used for absolute
layers (default: the corner the constraint implies), edited with a 9-point picker like Unity's
anchor presets. `transform-origin` stays the rotation/scale origin and is separate.

## Compatibility check (U1)

`unity.check {ids?, version? ("6.0"…), grid?}` walks the given layers (default: the selection, else
the current page) and reports, per layer, what the target version can't represent exactly:

- **error**: can't be represented (the export will drop or flatten it);
- **warning**: approximated or baked into an image (looks right, but isn't live UITK);
- **info**: converted to an equivalent construct (behaves the same).

| Rule | Error | Warning | Info |
|---|---|---|---|
| `grid-layout` | grid auto layout before 6.7, or 6.7+ without `grid: true` | — | 6.7+ with `grid: true` (experimental flag) |
| `gap` | — | gap with wrap before 6.7 (margins plus a negative container margin) | gap before 6.7 (margins) |
| `baseline-align` | cross align baseline | — | — |
| `strokes-in-layout` | — | auto layout with strokes not included and a stroke (UITK borders always take space) | — |
| `stroke-align` | — | centre/outside strokes (UITK borders are inside) | — |
| `stroke-style` | dashed strokes, variable width, brushes | — | — |
| `stroke-paint` | — | gradient/image strokes (borders are one colour; baked) | — |
| `fill-count` | — | more than one visible fill (one colour + one image) | — |
| `gradient` | — | linear/radial before 6.7, angular/diamond always (baked) | — |
| `image-mode` | — | crop/tile image fills | — |
| `blend` | — | non-normal blend modes on layers, fills or effects | — |
| `corner-smoothing` | — | smoothing > 0 | — |
| `drop-shadow` | — | before 6.6 (baked); 6.6+ with spread ≠ 0 or "show behind" | — |
| `inner-shadow` | inner shadows | — | — |
| `layer-blur` | — | before 6.3 | — |
| `background-blur` | — | before 6.6 | — |
| `effect-other` | noise, texture, glass | — | — |
| `mask` | masks other than clip content | — | — |
| `vector` | — | vector shapes/booleans before 6.3 (baked) | 6.3+ (Vector Image); their strokes and fills are inside the image |
| `text-line-height` | — | a line height other than auto (USS has none; `<line-height>` rich text) | — |
| `text-features` | — | lists, paragraph indents, decoration styles (thickness, offset, colour) | — |
| `text-runs` | — | — | mixed styles (rich text tags) |
| `text-fill` | — | gradient/image text fills (text colour is solid; baked) | — |
| `pivot` | — | a pivot set on a layer that isn't absolute (ignored) | — |
| `z-order` | — | — | "first on top" stacking: before 6.7 children reversed with a `*-reverse` direction; 6.7+ `z-index` |

Rules marked "to confirm" in the tables above are rechecked by the U4 harness and the table is
updated with what Unity actually does.

## Unity data in the file (U2)

- **File:** `Document::unity: Option<UnitySettings {version, grid}>` — the target Unity version
  and whether the project turns on UITK's experimental grid layout. Saved with the file and synced
  in live sessions (`DocProp::Unity`). `unity.check` uses it unless told otherwise.
- **Position** isn't stored separately: it follows from the structure, as UITK's does. A
  top-level frame is the panel's **root**; a layer in an auto layout frame is **relative** unless it
  ignores auto layout (Figma's absolute flag, `child_layout.absolute`); every other layer is
  **absolute**. `unity.layer.set {position}` switches the absolute flag (only in auto layout).
- **Pivot:** `Node::pivot: Option<Pivot {x, y}>` (0–1 each). `None` means the pivot the
  constraints imply: left/top → 0, centre → 0.5, right/bottom → 1, stretch and scale → 0. A pivot
  only places absolute layers; one set on another layer is reported by the `pivot` rule.

| Command | Params | Result |
|---|---|---|
| `unity.settings` | `{}` | `{version, grid, versions}` |
| `unity.settings.set` | `{version?, grid?}` (undo step) | the settings |
| `unity.layer` | `{id}` | `{position, inAutoLayout, pivot, pivotSet, anchorH, anchorV}` |
| `unity.layer.set` | `{ids?, pivot? ([x, y], {x, y}, a name like "center", or null), position? ("relative" \| "absolute")}` (undo step) | `{ids}` |

**Right panel:** with nothing selected, *Unity UI Toolkit* shows the target version (a dropdown,
6.0 to the newest the table knows), the experimental grid checkbox from 6.7, and the page's check
counts with the first issues. With layers selected, it shows the first layer's position (a
Relative/Absolute switch inside auto layout), for absolute layers the 9-point pivot picker (the
pivot as numbers, whether it comes from the constraints or the layer, Reset), and the selection's
counts and issues; clicking an issue selects its layer. Results are cached per document revision.

## Export (U3)

`file.exportUitk {ids?, dir?, version?, grid?, scale?, scripts?}` exports top-level frames (the
given ones' top-level frames, else the selection's, else every one on the page; frames in
sections count). Without `dir` it returns the UXML and USS as text (agents, tests); with `dir` it
writes, and the Design tab's Unity section has *Export to Unity…* (a folder picker):

```
<dir>/<Frame>.uxml          one per frame (the name made file-safe and unique)
<dir>/<Frame>.uss           one rule per layer: .fc-<layer id> { … }
<dir>/images/<hash>.<ext>   image fills, as stored
<dir>/images/baked-<id>.png layers the target version can't draw (rendered at `scale`, 2 by default)
<dir>/FigcraftPanelScale.cs
<dir>/FigcraftSafeArea.cs
```

**Elements.** Frames, groups, rectangles → `ui:VisualElement`; text → `ui:Label` (its text in
`text`, the line height as a `<line-height>` rich text tag); vector shapes and booleans → a
`VisualElement` with their baked PNG as `background-image`. `name` is the layer name; hidden layers
are `display: none`; locked ones `picking-mode="Ignore"`. The root fills the panel
(`position: absolute; left/top/right/bottom: 0`).

**Placement.**
- *Absolute* layers (outside auto layout, or ignoring it): `position: absolute`, per axis by
  constraint — left/top → `left`/`top` px; right/bottom → `right`/`bottom` px; left & right →
  both (no width); scale → `left` and `width` in %; centre, or any pin with a pivot that isn't its
  edge → the anchor as `left: 0|50|100%`, the pivot as `translate: -p%`, the rest a `margin-left`
  (same for the vertical axis). Offsets are measured inside the parent's border (UITK places
  absolute children in the padding box). Rotation → `rotate` about the centre, the box placed by
  its unrotated position.
- *Relative* layers (in auto layout): fixed → size + `flex-shrink: 0`; hug → no size; fill on the
  main axis → `flex-grow: 1; flex-basis: 0`; fill across → `align-self: stretch`; min/max sizes.
- *Auto layout frames*: `flex-direction`, `justify-content`, `align-items`, `flex-wrap` +
  `align-content`, padding (minus the border when strokes aren't included in the layout).

**By version.**

| Design | Before | From |
|---|---|---|
| Gap | `margin-left`/`margin-top` on every flow child after the first | 6.7: `gap` (`row-gap`/`column-gap` when wrapping) |
| "First on top" | children reversed + `row-reverse`/`column-reverse` (start/end swapped) | 6.7: `z-index` |
| Linear/radial gradient | leaf: baked; container: its first stop as the colour | 6.7: `linear-gradient()` / `radial-gradient()` |
| Drop shadow | leaf: baked, drawn by an overlay child where the PNG reaches past the box | 6.6: `filter: drop-shadow()` |
| Layer blur | leaf: baked | 6.3: `filter: blur(r/2)` |
| Background blur | — | 6.6: `backdrop-filter: blur(r/2)` |
| Vectors, inner shadows, noise/texture/glass, dashed or non-solid strokes, crop/tile/pattern fills, several fills | baked (leaf layers) | — |

**SafeArea.** A device frame's export has a `SafeArea` element (`figcraft-safe-area`) placed on
the frame's safe area. Layers that fit inside it go in it, with coordinates from its top-left;
layers reaching into the unsafe bands (backgrounds) stay on the root. With an auto layout frame,
the container is the flex container, with the frame's padding less the insets. At run time
`FigcraftSafeArea` moves every container onto `Screen.safeArea`.

**In Unity.** Copy the folder into `Assets/`, show `<Frame>.uxml` in a `UIDocument`, and add
`FigcraftPanelScale` (set `designWidth` to the frame's portrait width in points; it switches the
PanelSettings to Constant Pixel Size with `scale = short screen side / designWidth`) and
`FigcraftSafeArea` next to it. Fonts are the project's: each label carries `--fc-font-family` and
`--fc-font-style` for a project style sheet to map to font assets.

**Known limits (checked in U4):** whether Unity accepts `gap` on flex containers in 6.7 without the
grid flag; absolute children in the padding box; the blur radius halving; gradient angles on
non-square boxes; grid auto layout is exported as a wrapping row; rotated baked layers; mixed text
styles aren't carried (one style per label).

## Unity comparison (U4)

**Harness** (local, `plan/uitk-harness/`, needs a running Editor with the Unity CLI pipeline):
`figcraft_side.py` builds a test design, exports it into a Unity project's `Assets/FigCraftTest/`
and records every layer's rectangle (relative to the frame) after `frame.device.set` to each
device; `unity_side.py` shows the export in a `UIDocument` in play mode, sizes the root to the
device in points, puts the SafeArea on that device's insets (what `FigcraftSafeArea` computes on
the device), sets the panel scale to the device's (so layout rounds to its physical pixels), waits
for the layout and reads each element's `worldBound`; `compare.py` diffs them (0.5 pt tolerance).

**Run of 2026-10-20** — Unity 6000.6.0f1, target 6.6, devices from Unity's own device files:
iPhone 12 Pro Max (the design device), iPhone SE (2nd gen), Pixel 5, iPad Air (4th gen), iPhone X
landscape. The design covers stretch, right/top, centre/bottom, scale, a set centre pivot, an auto
layout row (hug text, fill spacer, fixed avatar, gap), a vertical auto layout list, and an absolute
child in a frame with a 3 pt border.

| | Layers within 0.5 pt |
|---|---|
| Every non-text layer, every device | **70 / 70** |
| The text label | 0 / 5: Unity's line is 22.5–22.7 pt tall where FigCraft's is 24 (font metrics), ≤ 0.3 pt narrower; the row centres it 0.7 pt lower |

**What it settled:**
- Absolute children are placed inside the parent's border (the padding box), as exported.
- The constraint, pivot (`left %` + `translate` + margin), scale (%), stretch and flexbox
  mappings give Unity's layout exactly, with gaps as margins (6.6).
- Unity's USS importer takes the generated properties without warnings (`translate`,
  `filter: drop-shadow()`, `--fc-*` custom properties).
- **Fixed in FigCraft:** layers inside a device frame's safe area now follow the safe area when the
  device changes (`frame.device.set`, `frame.swapOrientation`), by their constraints — what the
  SafeArea container does in Unity; before, FigCraft placed them against the whole frame (the
  bottom-pinned button was 34 pt off on Pixel 5). Layers flush with a frame edge bleed and follow
  the frame (`DeviceFrame::holds`, shared by both).

**Still open:** text metrics (U5 or a text-box option), `gap` and gradients on 6.7, the blur radius
halving, and the scripts on a real device (the run set the SafeArea the way `FigcraftSafeArea`
does rather than through the Device Simulator).

## Phases

- **U1 — mapping spec + compatibility check** (landed): this doc, `crates/uitk` (versions, rules),
  `unity.check`.
- **U2 — Unity data on layers** (landed): see [Unity data in the file](#unity-data-in-the-file-u2).
- **U3 — UXML/USS export** (landed): see [Export](#export-u3). The Unity-side scripts ship as
  generated text with each export (`crates/uitk/src/scripts.rs`).
- **U4 — Unity comparison harness** (run on 6.6): see [Unity comparison](#unity-comparison-u4). Next
  runs: 6.0 and 6.7 Editors, and the Device Simulator with the scripts.
- **U5 — flexbox-compatible layout, if U4 shows gaps**: frames in "UITK mode" laid out with Yoga's
  rules (e.g. the MIT `taffy` crate configured with UITK's defaults) so FigCraft's preview matches
  Unity where Figma's and Yoga's rules differ (shrinking, overflow). Only when measured differences
  can't be closed in the export.
- **Then: sync** — the export repeated on change into a Unity project.

## Open questions

- Does `gap` work on flex containers in 6.7 without the grid flag (the manual and the release post
  disagree)?
- `justify-content: space-evenly` support in each version.
- Text: TextCore/ATG metrics against FigCraft's (hug sizes of text are the most likely source of
  differences); line height.
- Where Figma and Yoga resolve the same structure differently (fill children that overflow,
  `flex-shrink: 1` by default, min sizes) — measured in U4.

## Sources

- [USS common properties (6000.7)](https://docs.unity.com/en-us/engine/6000.7/manual/uitoolkits/uielements/uie-uss/properties/supported-properties)
- [State of UI Toolkit in Unity 6.7](https://discussions.unity.com/t/state-of-ui-toolkit-in-unity-6-7/1736756)
- [UI Toolkit development status and next milestones (Nov 2025)](https://discussions.unity.com/t/ui-toolkit-development-status-and-next-milestones-november-2025/1698009)
- [New in Unity 6.3](https://docs.unity3d.com/6000.3/Documentation/Manual/WhatsNewUnity63.html),
  [6.4](https://docs.unity3d.com/6000.6/Documentation/Manual/WhatsNewUnity64.html),
  [6.6](https://docs.unity3d.com/6000.6/Documentation/Manual/WhatsNewUnity66.html)
- [USS transform (6000.0)](https://docs.unity3d.com/6000.0/Documentation/Manual/UIE-Transform.html)

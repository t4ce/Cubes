# CubeInterface previews

Press **3** in Cubes to cycle **confirm → info → slider → confirm**. Holding the
key does not advance repeatedly. Other number keys select their usual modes.

The panel has a small fixed tilt and fits the window. Hover highlights and raises
the apparent bevel; pressing provides visual feedback. Drag the slider or click
the checkbox, toggle, and counter to change their preview values. Each example
remembers its state for the current app session. Action buttons (including RESET)
and the X only provide feedback: they run no commands and do not close the panel.

## Editable examples

These files use the custom editor's `subcubes-interface` v2 configuration:

| File | Example |
| --- | --- |
| `confirm.json` | Fern confirmation with two text/icon buttons |
| `info.json` | Glacier information panel with one button |
| `slider.json` | Ember panel with all four controls and three button styles |

Keep `format`, `version`, and `preset` as supplied. The editable options are:

- `tierId`: `c1`, `c2`, `c3`, `c4`, `c6`, `c8`, or `c12` (panel cell scale).
- `themeId`: `fern`, `glacier`, or `ember`.
- `menu.title`: up to 30 characters.
- `menu.text`: up to 2048 characters, wrapped to 40 columns and 12 visible rows.
- `menu.buttonCount`: 1–3; enables that many entries from `menu.buttons`.
- Each button has `mode` (`text`, `icon`, or `both`), `text` (up to 14 characters),
  and `icon` (`close`, `check`, `plus`, `minus`, `left`, `right`, `heart`, `menu`,
  `play`, or `gear`).
- `menu.controls`: booleans for `slider`, `checkbox`, `toggle`, and `counter`.
- `state`: initial `progress` (0–100), `enabled`, `checked`, and `count` (−999–999).

The font supports printable ASCII; other characters appear as `?`. The examples
are validated and embedded by `build.rs`, so rebuild Cubes after editing JSON.
Exported geometry records from the HTML editor are not needed.

## Native rendering and checks

The font, icons, palettes, and layout come from `../AssetShowcase.html`'s
`CUBE_UI_CORE`. The native preview rasterizes logical cells and their layer
heights into a color map and a bevel normal map. One retained PBR quad displays
the panel. Hit testing intersects that same tilted quad. A worker creates and
uploads the maps through TRUEOS's standard BMP retained-image API; the previous
complete texture pair stays visible during interaction updates.

From the Cubes repository root:

```sh
node tools/export_interface_style.cjs --check
python3 tools/test_cube_interface.py
cargo check --offline
```

The host tests compare each default menu's layout and pixels with the actual HTML
editor, exercise preview actions, key cycling and picking, and round-trip the maps
through TRUEOS's BMP decoder. Set `CUBE_INTERFACE_PREVIEW_DIR` to a directory when
running the tests to save the three color maps as BMPs for inspection. Native GPU
lighting and UI4 pointer delivery still require a run on TRUEOS.

After changing the editor's font, icons, or palettes, run
`node tools/export_interface_style.cjs` to regenerate `src/interface_style.rs`.

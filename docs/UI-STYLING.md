# UI styling: CSS engines evaluated (2026-11-02)

Decision: widget styles stay in Rust (`ss13_ui_kit`). Neither `bevy_flair` nor `bevy_superui` is
adopted. The spike code was removed; this file keeps what was measured and why.

## bevy_flair 0.9 (CSS for bevy_ui)

Spike: `Button` and `NoticeBox` styled from a `tgui.css` with tgui-core's class names and color
variables, plus BandaStation's `ss220` theme as a class on the window. The output matched the
Rust-styled widgets pixel for pixel (±1/255), and the theme recolored default buttons as tgui does.

Measured with a headless Vending window (60 products, ~2000 UI nodes), CPU only:

| | without flair | with flair |
|---|---|---|
| idle frame, min | 0.70 ms | 0.92 ms (+0.2 ms) |
| update frame, p95, full re-render (973 nodes respawned) | ~19 ms | ~30 ms (+58%) |
| update frame, p95, memoized rows (148 nodes respawned) | 4.5–8.8 ms | 6.6–11.3 ms (~+2 ms) |
| web client wasm, gzip | 14.7 MB | 15.8 MB (+1.07 MB, +7%) |
| incremental release rebuild | ~65 s | ~66 s |

Only 2 of ~25 widgets were moved to CSS; the costs grow with every styled node.

Why not:
- **Crash risk.** flair 0.9 (and its `main` after 0.9) panics in `sync_em_size` (index out of bounds)
  when a node joins a styled tree after style computation but before layout. Our inline-run
  wrapping (`layout.rs`, `wrap_inline_runs`) did exactly that and had to be ordered before
  `StyleSystems::Prepare`; any future system that spawns UI nodes in `PostUpdate` would crash the
  client.
- **Silent misses.** A window spawned before the stylesheet asset loads is left unstyled with no error.
- **+1 MB gzip** of wasm for styling only.
- **No layout gains.** flair only decides values of `Node`, colors, fonts; layout is still bevy_ui.
  Its own limits: `calc()` cannot mix `%` and `px`, `var()` has no fallback, one stylesheet per
  entity, no `@container`.
- What it would give (selectors, `:hover`, CSS variables for tgui themes, transitions, hot reload)
  does not need a CSS engine: themes can be a Rust resource of the same variables
  (`--color-primary`, `--button-background-default`, ...) that widgets read.

Revisit if: flair fixes the `sync_em_size` panic and stops walking every styled node each frame,
and a wasm size budget allows +1 MB.

## bevy_superui (HTML + CSS + JS/TSX on bevy_ui)

Read the source (not built). HTML and a Solid-style `.tsx` layer run on V8 natively (`deno_core`)
or the browser's JS on wasm; styling is a fork of flair 0.9 (no fix for the panic in its patch list).
Not adopted:
- each HTML text node becomes its own `Text` entity in flex layout
  (`superui_bridge/src/reconcile.rs`), so there is no inline formatting either;
- no `innerHTML`, tables, SVG or canvas yet;
- tgui is React + tgui-core, not Solid: windows would be rewritten anyway, on the same layout;
- a second language and runtime in the client, with window data crossing a JS bridge instead of
  typed `UiInterface::Data`.

## What actually limits porting tgui windows

Independent of the styling engine (bevy_ui has no inline formatting context):
- **HTML/Markdown content**: paper, books, manuals, PDA messages (`dangerouslySetInnerHTML` in
  20 BandaStation interfaces, `marked` in 12). Needs a rich-text paragraph (`Text` + `TextSpan`).
- **Text and buttons on one wrapping line**: see the shortcut in `ss13_ui_kit/src/layout.rs`.
- **ByondUi** map views (11 interfaces): render-to-texture camera in a UI node.
- **SVG/canvas/Chart** (~10 interfaces): drawn case by case.

The re-render cost that made flair look expensive was ours: `ss13_tgui` respawned the whole
window on every update. Vending rows are now memoized (`memo_of`), and other large lists should
follow the same pattern.

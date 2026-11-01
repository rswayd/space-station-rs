# Plan: first playable vertical slice

Goal of the slice: a headless server loads a real station map from the source `.dmm`,
two native clients connect over WebTransport, see the map drawn from source `.dmi`, walk and see
each other, one tgui window works 1:1, and a bot harness reports tick time and bytes per client.

Coordinator: the main session. It owns contracts (`ss13_shared`, `ss13_assets::manifest`, root
`Cargo.toml`), merges agent branches, resolves `Cargo.lock`, and starts the next wave only after
the previous one is merged and green.

Budget: 19 agents in 4 waves. Every agent runs in its own git worktree; worktrees share
`CARGO_TARGET_DIR` by group (A: `target-a`, B: `target-b`, C: `target-c`) to keep disk at ~36 GB
instead of ~12 GB per agent.

## Wave 0 — coordinator (done)

Workspace, crate boundaries, `TilePos`, `Dir`, `Appearance`, `IconStateId`, `ClientIntent`,
`NetId`, `AssetManifest`, server/client composition roots, `AGENTS.md`.

## Wave 1 — foundations (9 agents, parallel, no shared files)

| # | Agent | Owns | Acceptance |
| --- | --- | --- | --- |
| 1 | dmi-import | `ss13_assets::dmi`, atlas packer, `ss13-import` bin | every `.dmi` in the source repo parses; states/dirs/frames/delays match `Description`; atlases + manifest written to `assets/generated` |
| 2 | dmm-import | `ss13_assets::dmm` | every `.dmm` in `_maps/` parses (TGM and classic); prefab count and grid size tests on 3 real maps |
| 3 | dm-extract | new crate `ss13_dm_extract` (tool, not linked into the game) | reads the source `.dme` with SpacemanDMM `dreammaker`; dumps static vars (name, desc, icon, icon_state, dir, density, opacity, layer, plane, anchored, parent) of every type used by maps; zero unresolved map types on `tramstation`/`MetaStation` |
| 4 | prototypes | new crate `ss13_proto` | prototype format with inheritance, loader, component registry, `spawn(proto_id, TilePos)`; tests for override/inheritance |
| 5 | world-core | `ss13_world` | chunked grid, `TilePos -> entities` spatial index kept in sync by observers, z-levels, movement with density blocking, move events; bench: 200k entities, 1k moves/tick |
| 6 | net-transport | `ss13_net` | lightyear over WebTransport (self-signed), handshake checks `PROTOCOL_VERSION` + token stub, channels, `ClientIntent` delivery; in-process test with 2 clients |
| 7 | ui-kit-layout | `ss13_ui_kit` modules `theme`, `window`, `layout`, `button`, `section` | tgui theme; `Window` (drag, close, z-order, focus), `Section`, `Stack`, `Flex`, `Box`, `Divider`, `Button`, `NoticeBox`, `LabeledList`; `examples/gallery.rs` |
| 8 | ui-kit-input | `ss13_ui_kit` module `input` | `Input`, `TextArea`, `NumberInput`: Cyrillic, selection, clipboard, IME; first checks what Bevy 0.20 (`bevy_ui_widgets`, text editing) already provides |
| 9 | render-core | `ss13_render` | loads manifest + atlases, draws `Appearance` at `TilePos`, dirs, animation by delays, layer/plane ordering, camera; `examples/map_view.rs` draws a static test map |

## Wave 2 — integration (5 agents)

| # | Agent | Depends on | Acceptance |
| --- | --- | --- | --- |
| 10 | map-load | 2, 3, 4, 5 | server spawns a full station from `.dmm` via prototypes; entity count and load time logged |
| 11 | replication | 5, 6 | `TilePos`/`Appearance` replicated with per-client interest by view radius and z; `NetId` mapping; bytes/client/s metric |
| 12 | client-play | 9, 11 | client renders replicated world, sends `Move` intents, own-mob prediction, others interpolated, camera follows |
| 13 | ui-protocol | 6, 7 | server window model (open/close/state/`act` with validation), client binding; one tgui window (vending) ported 1:1 |
| 14 | ui-kit-more | 7 | `Tabs`, `Table`, `Dropdown`, `Tooltip`, `Modal`, scroll area, `VirtualList`, `ProgressBar`, `Icon`/sprite in UI |

## Wave 3 — scale and chat (3 agents)

| # | Agent | Acceptance |
| --- | --- | --- |
| 15 | load-bots | headless bot client crate; N bots walk/click; report: tick p50/p99, bytes/client; runs with N = 100, 250, 500 |
| 16 | perf | profile under 500 bots (`trace_tracy`), fix the top hot spots in interest/replication; before/after numbers |
| 17 | chat | chat window: markup → `TextSpan`, virtualization, `Say` handled and range-limited on the server |

## Wave 4 — independent check (2 agents)

| # | Agent | Acceptance |
| --- | --- | --- |
| 18 | review | read-only review of waves 1–3 against `AGENTS.md`; findings with file:line and a failing scenario |
| 19 | e2e | scripted scenario: server + 2 clients on a real station, walk, open window, chat; documented commands |

## Out of scope for the slice

Atmos, power, lighting, combat, health, inventory, jobs, round flow, DB, admin tools, web client.
They are the next plans, built on the same contracts.

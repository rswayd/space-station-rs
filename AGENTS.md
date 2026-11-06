# AGENTS.md — ss13-rs

Use Russian when talking to the user. Code, identifiers, comments and commit messages are in English.

## What this is

A from-scratch Rust implementation of a Space Station 13 server and native client.
Core: `bevy_ecs` (Bevy, latest release). Everything else is a plugin.
Reference behavior and assets come from a BYOND codebase checkout (default: `../BandaStation`,
override with `SS13_SOURCE`). That checkout is read-only input: never edit it.

Priorities: native desktop client first, web client later (keep the door open: no native-only
APIs in `ss13_shared`, transport is WebTransport/QUIC). UI is a 1:1 port of tgui on `bevy_ui`.
Goal: many concurrent players per round — every hot-path decision states frequency × scale × cost.

## Crates and ownership

| Crate | Role | Must not depend on |
| --- | --- | --- |
| `ss13_shared` | contracts: `TilePos`, `Dir`, `Appearance`, protocol | anything else in the workspace |
| `ss13_assets` | offline import: `.dmi`, `.dmm`, manifest | Bevy app types |
| `ss13_world` | authoritative simulation core | rendering, networking |
| `ss13_net` | the only crate that imports the network library (`wtransport`) | game logic |
| `ss13_render` | client drawing | networking |
| `ss13_ui_kit` | tgui-core primitives on `bevy_ui` | protocol, game types |
| `ss13_server` / `ss13_client` | composition roots: add plugins, no logic | — |

- A task owns the crate or module named in it. Edits outside it are limited to the wiring needed
  (`mod` lines, adding a plugin in a composition root) and are reported.
- `ss13_shared`, `ss13_assets::manifest` and root `Cargo.toml` are contracts owned by the coordinator.
  A task that needs a contract change stops and reports the exact proposed change instead of making it.
- Crate-local dependencies may be added (latest stable versions); report every new one.

## Rules

- Server is authoritative. Clients send `ClientIntent` only; every intent is validated at the moment of
  effect (distance, permissions, current state). Never trust client data.
- Game systems never touch the network library: they read/write components and events from
  `ss13_shared`; `ss13_net` maps them.
- Simulation is frame-independent: rates are per second, multiplied by the delta.
- No per-tick global scans on hot paths; use the spatial index and change detection.
- Prefer existing Bevy mechanisms (plugins, observers, events, change detection, states, `SystemSet`)
  over hand-rolled ones. Read the Bevy version actually in `Cargo.lock` before using an API:
  Bevy breaks APIs between releases and memory of older versions is often wrong.
- No speculative abstraction. Shortest code that fully solves the task.
  Deliberate limits are marked `// shortcut: <limit>, <when to upgrade>`.
- Comments explain only why.
- `unsafe` is denied workspace-wide.
- Licensing. Code is AGPL-3.0-only (`LICENSE`); it ports tg/BandaStation logic (AGPL v3), so no
  permissive relicensing of any part. Assets come from BandaStation under CC BY-SA 3.0 unless an
  `attributions.txt`/`license.txt` next to them says otherwise: every asset the converter copies keeps
  its source path and those files. A new asset not from BandaStation (font, sound, image) lands only
  with its license file next to it and a source link. A new crate dependency must have an
  AGPL-compatible license. The project is non-commercial: an asset with non-commercial or
  personal-use terms is listed by `ss13-mapload audit` and named in `ASSETS-LICENSE.md`.

## Done means

While working: `cargo check` and the tests of your area (`cargo nt -E 'test(/^<module>::/)'`;
docs/RUNNING.md "Tests"). Once, at the end, after `git merge master`: `scripts/gate.sh` (fmt,
`cargo lint`, `cargo nt --no-fail-fast`, the ignored real-asset tests when `SS13_SOURCE` is set,
`cargo bins`, the bot seed suite). The coordinator
re-runs it after merging. New integration tests are modules of `crates/<crate>/tests/it/`, not new
`tests/*.rs` files.
Plus the task's own acceptance criterion is demonstrated by a test or an exact command with its
output. Report what was not checked.

## Builds

Each agent worktree uses its own `./target` (a shared target dir mixes artifacts of same-named
workspace crates across worktrees and gives false test results). The coordinator seeds it with a
copy of master's `target`, so the first build is incremental; do not `cargo clean` it. Avoid release
builds unless a number needs them. Large generated data goes to the scratch dir named in the task,
never into the repo.

Generated assets: use the shared directory named in the task (built by the coordinator from current
master). Regenerate your own copy only when your change affects their output (`ss13_dm_extract`,
`ss13-import`, `ss13-mapload convert`, prototype components) — then into your own scratch dir.

## Visual checks

Never drive the desktop: no `xdotool`, `ydotool`, `import`, `grim` or anything that injects OS input
or captures the screen; on this machine they trigger a remote-control permission prompt the user
may not be there to answer. Screenshots and input go through the app itself (Bevy `Screenshot`,
injected input messages) — use the scripted client: `scripts/visual-check.sh` (`docs/RUNNING.md`,
"Scripted client / visual checks").

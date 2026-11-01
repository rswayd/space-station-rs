# Browser demo (plan)

Goal: a static page; the game loads, a server runs in the same browser (Web Worker), the client
(main thread, WebGL2) connects to it in memory; a reload resets everything. No backend.

Status: phase 1 done (in-memory transport, `wasm32-unknown-unknown` type-checks, tick cost
measured); phase 2 steps 1-3 done (data source `ss13_data`, worker entry `ss13_web_server`, page
entry `ss13_web_client`, shared bridge `ss13_web_common`), page and build script in `web/` and
`scripts/web-build.sh`.

## Architecture

```
 main thread (page)                                   Web Worker
 ┌──────────────────────────────┐                     ┌──────────────────────────────┐
 │ client App (bevy_winit,      │                     │ server App (no window,       │
 │ WebGL2 canvas, bevy_audio)   │                     │ ticked every 50 ms)          │
 │ NetClientPlugin              │                     │ NetServerPlugin              │
 │   + LoopbackConnector ───┐   │                     │   + LoopbackListener ◄──┐    │
 │                          ▼   │     postMessage     │                         │    │
 │ bridge: LoopbackListener ────┼── open / frame / ───┼─► bridge: LoopbackConnector  │
 │   (one end per connect)      │   datagram / close  │   (connect() per "open")     │
 └──────────────────────────────┘   (ArrayBuffer,     └──────────────────────────────┘
                                     transferred)
```

- `ss13_net::loopback`: `listener()` gives a `LoopbackListener` (server) and a `LoopbackConnector`
  (clients). A connection is a pair of `LoopbackEnd`s: frames exactly as on the WebTransport stream
  (`u32` length prefix + postcard, the same `MAX_*_FRAME` limits and decoding, so a malformed or
  oversized frame closes the session as over QUIC), motion datagrams packed to 1200 bytes, and a
  close with the QUIC application codes (`protocol::close`). No async runtime: the server pumps
  connections in `PreUpdate` (`server_loopback`), the client in its `receive` system. A server
  with a listener opens no socket; on wasm32 loopback is the only transport.
- The worker and the page are separate wasm instances (no shared memory without
  cross-origin isolation), so they do not share a `LoopbackConnector`. The page runs its own
  `listener()`: the client app gets the connector, a small bridge system holds the listener. For
  each new end (a connect, or a reconnect after a loss) it posts `open` to the worker, whose bridge
  calls the worker's `LoopbackConnector::connect()`; afterwards both bridges forward every
  `recv_frame` / `recv_datagram` as a transferred `ArrayBuffer` and call `send_frame` /
  `send_datagram` on the other side, and forward a close as `{code, reason}`. The bridges never
  decode: validation stays in `ss13_net`.
- The worker drives its app with its own timer calling `App::update` every 50 ms (Bevy 0.20's
  `ScheduleRunnerPlugin` on wasm calls `web_sys::window().set_timeout`, and a worker has no
  `window`); the page drives the client from `requestAnimationFrame` (`bevy_winit`).
- Reload = new worker and new page state: nothing persists, as wanted.

## Assets: start vs lazy (MetaStation, measured on the converted assets)

| Who | What | Raw | gzip | When |
| --- | --- | --- | --- | --- |
| server | `prototypes/` (7.9 MB), `*.ron` (jobs, cargo, mobs, reagents, research, ...; ~5.5 MB), `manifest.postcard` (4.9 MB) | 18.8 MB | 1.9 MB | start |
| server | map files: `MetaStation.dmm`, `metastation.json`, emergency/cargo/infiltrator shuttles (the list in `scripts/demo-package.sh`) | 2.3 MB | ~0.35 MB | start |
| client | `manifest.postcard` (shared with the server: fetch once, hand the bytes to the worker) | 4.9 MB | 0.73 MB | start |
| client | `atlas/*.png`, 70 pages of 2048x2048 | 118 MB | (PNG) | lazily, per page on first use (the native client already loads pages on demand and evicts by `SS13_ATLAS_BUDGET_MB`) |
| client | `sound/` (ogg) | 12 MB | (ogg) | lazily, on first play |
| — | `dm_types.json` (46 MB), `licenses/`, `ICON-SOURCES.txt` | | | not shipped (converter input, attribution files linked from the page) |

So a first frame needs ~21 MB raw / ~2.6 MB gzip of data plus the two wasm binaries (size not yet
measured; Bevy 2D + UI release wasm is typically tens of MB before `wasm-opt`) plus the atlas
pages around the spawn.

## Remaining wasm32 blockers

`cargo check --target wasm32-unknown-unknown` passes for `ss13_server`, `ss13_client` and
`ss13_game` (`server`, `client`). These compile but fail **at runtime** on wasm32:

| Area | Files | Why | Phase 2 fix |
| --- | --- | --- | --- |
| Debug panel time stamp | `ss13_debug/src/client/panels.rs` | `std::time::SystemTime::now` panics | `web_time::SystemTime`, as done for the server crates |
| Config by environment | every `std::env::var` | always unset on wasm32: defaults apply (fine for the demo) | URL query parameters if a knob is needed |

Fixed in phase 2: data files (every runtime read goes through `ss13_data`: `std::fs` natively,
the downloaded bundle mounted by the wasm entry; atlas pages and sounds are copied from the JS
`Map` into wasm memory on first read), admin config (the worker mounts `config/admin_ranks.txt`
and `dev = Host`, `insecure_admins`), composition roots (`ss13_web_server`/`ss13_web_client`
repeat the plugin lists of `ss13_server`/`ss13_client`; the worker rebuilds the app on
`REBOOT_EXIT_CODE` and moves the listener over), WebGL2 (`bevy/2d` already brings `webgl2`; the
canvas comes from `start`).

Fixed in phase 1: `wtransport`/tokio runtime (native only; tokio's `sync` channels work
everywhere), `std::time::Instant` (`bevy::platform::time::Instant`, `web-time` on wasm via
`bevy/web`), `SystemTime` seeds in `ss13_game`, `ss13_chat`, `ss13_admin`, `ss13_mapload`
(`web-time`), `getrandom` (enabled for the web by `bevy/web`), the dev certificate digest file
(skipped when there is no certificate). Not blockers: Bevy `multi_threaded` (bevy_tasks is
single-threaded on wasm32 whatever the feature; atmos' `ComputeTaskPool::scope` runs on the
calling thread), `rayon` in `ss13_render`/`ss13_assets` (rayon-core falls back to the current thread
when it cannot spawn).

## Tick cost

`loopback::metastation_tick_single_threaded` (ignored test, `crates/ss13_game/tests/it/loopback.rs`):
MetaStation, round started, one player joined over loopback and idle, gameplay plugins as in
`ss13_server` (atmos, tgui, chat), every schedule on Bevy's `SingleThreadedExecutor`, task pools
without worker threads; 5 s warm-up, then 1200 ticks (60 s of game time), server `update` only.

```sh
SS13_ASSETS=$ASSETS cargo test --release -p ss13_game --all-features --test it \
    loopback::metastation -- --ignored --nocapture
```

| Build | avg | p50 | p95 | p99 | max | map load |
| --- | --- | --- | --- | --- | --- | --- |
| native release | 0.65 ms | 0.62 ms | 0.80 ms | 0.93 ms | 10.8 ms | 16.0 s |
| native dev profile (deps `opt-level=3`, our crates 1) | 1.17 ms | 1.12 ms | 1.40 ms | 1.60 ms | 12.9 ms | 15.4 s |
| wasm32 release in node 22 (`ss13_web_server`, no player joined, 200 ticks) | 5.22 ms | | 6.63 ms | | 16.0 ms | 25.7 s |

The budget is 50 ms per tick, so even a several-fold wasm slowdown leaves the idle server far
inside it; the open questions are busy rounds (atmos fires, many mobs) and the server start
(`Station::start`: map load and round start, 16 s native even in release: on wasm it blocks the
worker, not the page, but the player waits; profile it before phase 3).

The wasm row is the release build before `wasm-opt`, idle with no client (the native rows have
one idle player), timed by the worker's own tick log (`web server: N ticks, avg ...`); its map
load is the whole start (`start` to `ready`). Prototypes and the map load in plugin
`finish`/`build`, so the worker posts only two coarse `progress` stages.

## Phase 2 steps

1. Data source: a resource of bytes by relative path, read by `ss13_proto`, `ss13_mapload`,
   `ss13_reagents`, the manifest and atlas loaders; native fills it from disk lazily, the web from
   a fetched bundle (server set above) before the app is built. Keep native behaviour identical.
2. Server wasm entry (`ss13_web` crate or a `cfg(wasm32)` bin of `ss13_server`): the server plugin
   list, `LoopbackListener`, a JS-facing `open`/`frame`/`datagram`/`close` API for the bridge,
   ticked by a timer. Measure the wasm tick in node with the same scenario as the native probe.
3. Client wasm entry: `bevy/webgl2`, canvas, the page-side `listener()` + bridge, lazy atlas pages
   and sounds over `fetch`, local player as host admin.
4. Page: `index.html` + loader (progress over the start bundle), worker script, `trunk` or a
   plain `wasm-bindgen --target web` build + `wasm-opt -Oz`; licence and source links (AGPL §13,
   CC BY-SA attribution) on the page.
5. CI: a `wasm32` `cargo check` of both roots in the gate so native changes do not reintroduce
   `std::fs`/`Instant` regressions, and a release job publishing the static site.

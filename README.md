# ss13-rs

A from-scratch Rust implementation of a Space Station 13 server and client. It ports the
behaviour of [BandaStation](https://github.com/ss220club/BandaStation), a
[tgstation](https://github.com/tgstation/tgstation) fork, and converts its icons, sounds and maps
from a BandaStation checkout. No BYOND is involved at runtime.

About 69% of the BandaStation mechanics in the catalog are ported (weighted over 2,483 entries,
`docs/parity/PROGRESS.html`).

## Stack

- **Rust** (edition 2024) and **Bevy 0.20** (`bevy_ecs`): server and client are Bevy apps,
  every feature is a plugin.
- **Networking**: WebTransport over QUIC (`wtransport`), postcard-encoded messages. The server is
  authoritative: clients send only intents, and the server checks each one when it takes effect.
- **UI**: a port of tgui on `bevy_ui`. Debug and admin windows use `bevy_egui`.
- **Assets**: an offline converter turns `.dmi` icons, `.dmm` maps and DM type definitions into a
  texture atlas, a manifest and prototype files.

## Performance

Server under load: release build, MetaStation (~110k entities), headless bots that take a step
about every 0.5 s, 60 s per stage. Measured 2026-11-06. The tick budget is 50 ms (20 ticks/s).

| bots | server RSS | tick p50 | tick p99 | traffic per client |
|---|---|---|---|---|
| 0 | 656 MiB | 11.0 ms | 17.2 ms | — |
| 10 | 673 MiB | 11.4 ms | 17.8 ms | — |
| 50 | 687 MiB | 12.5 ms | 19.5 ms | — |
| 100 | 705 MiB | 14.0 ms | 21.5 ms | 2.3–3.3 KB/s (max 10.5 KB/s) |

Each client adds about 0.5 MiB of server memory. The rest is the station itself. Walking bots are
lighter than real players (no clicks, fights, windows or speech), so these numbers are a lower
bound. With one idle player, a single-threaded tick takes 0.65 ms on average in release. How to
reproduce: `docs/RUNNING.md`, "Per-system profile".

## Client

The client is a native desktop app (Linux, Windows). It draws the station with Bevy's 2D
renderer, BYOND-style lighting and the tgui windows. It loads atlas pages when it first needs them
and evicts them past a memory budget (`SS13_ATLAS_BUDGET_MB`, 256 MiB by default).

To play alone on Windows, download the zip of a
[release](https://github.com/rswayd/space-station-rs/releases), unpack it and run
`ss13_client.exe`.

## Web version

The same code also builds for the browser (`wasm32`). In the browser demo, the server runs in a
Web Worker and the client draws on a WebGL2 canvas. The two connect in memory, so it is
single-player and needs no backend. It downloads about 35 MB of data before the start and fetches
the sounds in the background. On wasm in Node, an idle tick takes about 5 ms against the 50 ms
budget. It currently runs in Chrome only. For a multiplayer browser client, WebTransport is the
same protocol the native client already uses. Details are in `docs/WEB.md`.

## Running

To generate the assets and run a server, clients and the tests, see
[`docs/RUNNING.md`](docs/RUNNING.md). The architecture is described in `docs/ARCHITECTURE.html`.

## License

Code: AGPL-3.0-only (`LICENSE`). Assets: see [`ASSETS-LICENSE.md`](ASSETS-LICENSE.md). Ported
MIT code (tgui, tgui-core), fonts and the Rust dependencies:
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).
A public server must offer its source to players: set `SS13_SOURCE_URL` (`docs/RUNNING.md`).
The project is non-commercial, and a few assets allow only non-commercial use
(`ASSETS-LICENSE.md`).

# ss13-rs

A from-scratch Rust implementation of a Space Station 13 server and native client (Bevy), ported
from the behaviour of [BandaStation](https://github.com/ss220club/BandaStation), a
[tgstation](https://github.com/tgstation/tgstation) fork; its icons and sounds are converted
from a BandaStation checkout. To generate the assets and run a server and clients, see
[`docs/RUNNING.md`](docs/RUNNING.md).

To just play alone on Windows: download the zip of a [release](https://github.com/rswayd/space-station-rs/releases),
unpack it and run `ss13_client.exe` (`docs/RUNNING.md`, "Windows demo / releases").

License: code AGPL-3.0-only (`LICENSE`); assets: see [`ASSETS-LICENSE.md`](ASSETS-LICENSE.md).
A public server must offer its source to players: set `SS13_SOURCE_URL` (`docs/RUNNING.md`).
The project is non-commercial; a few assets allow only non-commercial use (`ASSETS-LICENSE.md`).

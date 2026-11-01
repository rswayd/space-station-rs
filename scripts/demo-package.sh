#!/usr/bin/env bash
# The platform-independent part of the single-player demo package (docs/RUNNING.md, "Windows
# demo / releases"): converted assets, the BYOND map files the server reads at runtime, the admin
# config, licenses and a README. The release workflow (.github/workflows/demo.yml) adds the
# executables and zips it.
# Usage: scripts/demo-package.sh <BandaStation checkout> <converted assets> <out dir>
set -euo pipefail
source_dir=$(realpath "$1")
assets=$(realpath "$2")
mkdir -p "$3"
out=$(realpath "$3")
cd "$(dirname "$0")/.."
mkdir -p "$out/assets" "$out/data/BandaStation" "$out/config"

# dm_types.json is converter input only (ss13-import, ss13-mapload convert); nothing reads it at
# runtime.
cp -a "$assets/." "$out/assets/"
rm -f "$out/assets/dm_types.json"

# What the server opens under SS13_SOURCE for MetaStation:
# - the station map (ss13_mapload MapLoadPlugin::map_paths), read again for landmarks;
# - the map json naming its shuttles (ss13_mapload::dock::map_shuttle) and the templates it names:
#   emergency (ss13_game::shuttle) and cargo (ss13_game::cargo::supply), at
#   dock::shuttle_template_path;
# - the nuke ops infiltrator (ss13_game::antag::nukeop::server).
maps=(
    _maps/map_files/MetaStation/MetaStation.dmm
    _maps/map_jsons/metastation.json
    _maps/shuttles/emergency/emergency_meta.dmm
    _maps/shuttles/cargo/cargo_box.dmm
    _maps/shuttles/infiltrator/infiltrator_basic.dmm
)
for map in "${maps[@]}"; do
    mkdir -p "$out/data/BandaStation/$(dirname "$map")"
    cp "$source_dir/$map" "$out/data/BandaStation/$map"
done

cp config/admin_ranks.txt "$out/config/"
# The local player (default token `dev`) is the host. The bundled server trusts this only because
# the client binds it to loopback (crates/ss13_client/src/local_server.rs).
printf '%s\n' "dev = Host" >"$out/config/admins.txt"

cp LICENSE ASSETS-LICENSE.md "$out/"
commit=$(git rev-parse HEAD)
source_commit=$(git -C "$source_dir" rev-parse HEAD)
cat >"$out/README.txt" <<EOF
space-station-rs: single-player demo
====================================

Run ss13_client.exe. It starts the bundled ss13_server.exe on this computer (loopback only, a free
port) with MetaStation, waits for it, connects and stops it when the window closes. The round is
already running: pick a job in the lobby ("Присоединиться"). You are the host admin (F7 admin
panel, F6 aghost).

Controls and features: docs/RUNNING.md in the source repository.

To play on a remote server instead, set SS13_SERVER_ADDR=<host:port> and SS13_CERT_DIGEST=<hex>
(the digest the server logs); the client then starts no local server.

Source code (AGPL-3.0-only, LICENSE): https://github.com/rswayd/space-station-rs
Built from commit $commit:
https://github.com/rswayd/space-station-rs/tree/$commit

Assets (assets/) and map files (data/BandaStation/) come from BandaStation
(https://github.com/ss220club/BandaStation) commit $source_commit:
CC BY-SA 3.0 unless an attribution file says otherwise; the map files are AGPL-3.0, like
BandaStation's code. Details: ASSETS-LICENSE.md. This project is non-commercial.
EOF

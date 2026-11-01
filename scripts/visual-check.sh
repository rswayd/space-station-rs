#!/usr/bin/env bash
# Visual check without OS-level input or screen capture: a private server on a free port and one
# scripted client per script (docs/RUNNING.md, "Scripted client / visual checks").
#
#   scripts/visual-check.sh [-o OUT_DIR] SCRIPT [SCRIPT...]
#
# Env: SS13_ASSETS (default assets/generated), SS13_MAP (default MetaStation, "" = empty world),
#      SS13_SKIP_LOBBY (default 1: clients get a body at once; 0 for the lobby, with
#      SS13_LOBBY_SECONDS for its countdown),
#      VISUAL_CHECK_WINDOWED=1 (real windows instead of offscreen rendering),
#      VISUAL_CHECK_TIMEOUT (seconds per client, default 300), VISUAL_CHECK_NO_BUILD=1.
# A script line `# spawn: x,y[,z]` spawns that script's client there (SS13_DEV_SPAWNS, debug
# server builds), whatever order the clients join in; scripts without it take the next spawn point.
# Exit code: 0 when every client's script succeeded.
set -euo pipefail

cd "$(dirname "$0")/.."
out=""
while getopts "o:" opt; do
    case $opt in
        o) out=$OPTARG ;;
        *) exit 2 ;;
    esac
done
shift $((OPTIND - 1))
if [ $# -eq 0 ]; then
    sed -n '2,14p' "$0" >&2
    exit 2
fi
for script in "$@"; do
    [ -f "$script" ] || { echo "no such script: $script" >&2; exit 2; }
done

assets=${SS13_ASSETS:-assets/generated}
map=${SS13_MAP-MetaStation}
out=${out:-${TMPDIR:-/tmp}/ss13-visual-$(date +%Y%m%d-%H%M%S)}
mkdir -p "$out"
out=$(cd "$out" && pwd)

if [ "${VISUAL_CHECK_NO_BUILD:-}" != 1 ]; then
    cargo bins -q
fi
target=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')

# QUIC is UDP: ask the kernel for a free UDP port so a server the user runs on 7777 is untouched.
port=$(python3 -c 'import socket; s=socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')

pids=()
cleanup() {
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Client N's token is clientN; its script's `# spawn:` line maps that token to a tile.
dev_spawns=""
i=0
for script in "$@"; do
    i=$((i + 1))
    tile=$(sed -n 's/^# spawn: *\([0-9]\+,[0-9]\+\(,[0-9]\+\)\?\) *$/\1/p' "$script" | head -n 1)
    if [ -n "$tile" ]; then
        dev_spawns="${dev_spawns:+$dev_spawns;}client$i=$tile"
    fi
done

SS13_BIND_ADDR=127.0.0.1:$port SS13_MAP=$map SS13_ASSETS=$assets SS13_DEV_SPAWNS=$dev_spawns \
    SS13_SKIP_LOBBY=${SS13_SKIP_LOBBY-1} \
    "$target/debug/ss13_server" >"$out/server.log" 2>&1 &
server=$!
pids+=("$server")

# The digest line is logged once the map is loaded and the endpoint is up.
digest=""
for _ in $(seq 1 600); do
    digest=$(grep -o 'certificate digest [0-9a-f]\{64\}' "$out/server.log" | awk '{print $3}' || true)
    [ -n "$digest" ] && break
    kill -0 "$server" 2>/dev/null || { echo "server died, see $out/server.log" >&2; exit 1; }
    sleep 0.1
done
[ -n "$digest" ] || { echo "server did not start in 60 s, see $out/server.log" >&2; exit 1; }

offscreen=1
[ "${VISUAL_CHECK_WINDOWED:-}" = 1 ] && offscreen=0
clients=()
i=0
for script in "$@"; do
    i=$((i + 1))
    name=client$i
    mkdir -p "$out/$name"
    cp "$script" "$out/$name/script.txt"
    SS13_SERVER_ADDR=127.0.0.1:$port SS13_CERT_DIGEST=$digest SS13_TOKEN=$name \
        SS13_ASSETS=$assets SS13_SCRIPT=$script SS13_SCRIPT_OUT=$out/$name SS13_OFFSCREEN=$offscreen \
        timeout --signal=KILL "${VISUAL_CHECK_TIMEOUT:-300}" "$target/debug/ss13_client" \
        >"$out/$name.log" 2>&1 &
    clients+=($!)
    pids+=($!)
done

status=0
i=0
for pid in "${clients[@]}"; do
    i=$((i + 1))
    if wait "$pid"; then
        result=ok
    else
        result="FAILED (exit $?)"
        status=1
    fi
    echo "client$i: $result, log $out/client$i.log"
    sed 's/\x1b\[[0-9;]*m//g' "$out/client$i.log" | grep -h 'script: \(state\|FAILED\)' | sed 's/^/    /' || true
    for shot in "$out/client$i"/*.png; do
        [ -e "$shot" ] && echo "    screenshot $shot"
    done
done
echo "server log $out/server.log"
echo "output $out"
exit $status

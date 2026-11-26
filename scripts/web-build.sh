#!/usr/bin/env bash
# The browser demo as a static site (docs/RUNNING.md, "Browser demo"): both wasm crates (release,
# wasm-bindgen --target web, wasm-opt), the page (web/) and every runtime data file with
# data/files.json. The data comes from a demo package (scripts/demo-package.sh), so the map list
# lives in one place.
# Usage: scripts/web-build.sh <demo package dir> [out dir, default dist]
set -euo pipefail
pkg=$(realpath "$1")
cd "$(dirname "$0")/.."
out=${2:-dist}
target=target/wasm32-unknown-unknown/release

# wasm-bindgen's CLI must match the crate in Cargo.lock exactly.
locked=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p')
cli=$(wasm-bindgen --version | cut -d' ' -f2)
if [[ $cli != "$locked" ]]; then
    echo "wasm-bindgen $cli, Cargo.lock has $locked: cargo install wasm-bindgen-cli --version $locked --locked" >&2
    exit 1
fi

# Two invocations: one build would unify the features of both crates into both binaries.
cargo build --release --target wasm32-unknown-unknown -p ss13_web_server
cargo build --release --target wasm32-unknown-unknown -p ss13_web_client

rm -rf "$out"
mkdir -p "$out/data"
for side in client server; do
    crate=ss13_web_$side
    wasm-bindgen --target web --no-typescript --out-dir "$out/$side" "$target/$crate.wasm"
    # The features rustc enables by default for wasm32-unknown-unknown.
    wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
        --enable-mutable-globals --enable-reference-types --enable-multivalue \
        -o "$out/$side/${crate}_bg.wasm" "$out/$side/${crate}_bg.wasm"
    # Cloudflare Pages rejects files over 25 MiB and the client wasm is larger: ship it gzipped,
    # the page inflates it (DecompressionStream). Also cuts the download to about a third.
    gzip -9 "$out/$side/${crate}_bg.wasm"
done
cp web/index.html web/style.css web/main.js web/server-worker.js "$out/"
# The parity readiness page (scripts/parity_progress.py), self-contained, linked from the landing.
cp docs/parity/PROGRESS.html "$out/progress.html"

# Data root = the assets root; the map files keep the BandaStation/ prefix of the package.
cp -a "$pkg/assets/." "$out/data/"
cp -a "$pkg/data/BandaStation" "$out/data/"
cp "$pkg/LICENSE" "$pkg/ASSETS-LICENSE.md" "$pkg/THIRD-PARTY-NOTICES.md" \
    "$pkg/THIRD-PARTY-LICENSES.html" "$out/"
# Packs instead of thousands of small files, files.json lists them (scripts/web-pack.py); the
# download counts the packs fetched before the start.
python3 scripts/web-pack.py "$out/data"
data_bytes=$(python3 -c 'import json, sys
print(sum(p["size"] for p in json.load(open(sys.argv[1]))["packs"] if not p["later"]))' "$out/data/files.json")
# Packs are named by their content: a browser keeps them across visits and builds.
printf '%s\n' '/data/pack/*' '  Cache-Control: public, max-age=31536000, immutable' >"$out/_headers"

client_bytes=$(stat -c %s "$out/client/ss13_web_client_bg.wasm.gz")
server_bytes=$(stat -c %s "$out/server/ss13_web_server_bg.wasm.gz")
download_mb=$(((data_bytes + client_bytes + server_bytes + 524288) / 1048576))
commit=$(git rev-parse HEAD)
# AGPL-3.0 §6(d): the page links the Corresponding Source of this build, the public repository at
# this commit (web/index.html footer).
cp LICENSE ASSETS-LICENSE.md THIRD-PARTY-NOTICES.md "$out/"
sed -i -e "s/__COMMIT__/$commit/g" -e "s/__DOWNLOAD_MB__/$download_mb/g" \
    -e "s/__WASM_BYTES__/{\"client\":$client_bytes,\"server\":$server_bytes}/" "$out/index.html"
echo "$out: data $((data_bytes / 1048576)) MB, client wasm $((client_bytes / 1048576)) MB," \
    "server wasm $((server_bytes / 1048576)) MB, download ~$download_mb MB"

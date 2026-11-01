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

# Data root = the assets root; the map files keep the BandaStation/ prefix of the package.
cp -a "$pkg/assets/." "$out/data/"
cp -a "$pkg/data/BandaStation" "$out/data/"
cp "$pkg/LICENSE" "$pkg/ASSETS-LICENSE.md" "$out/"

# Runtime files only: attribution texts and licenses/ are shipped (linked from the page) but not
# downloaded up front.
list=$(cd "$out/data" && find . -type f ! -name '*.txt' ! -path './licenses/*' -printf '%P\t%s\n' | LC_ALL=C sort)
data_bytes=$(awk -F'\t' '{ s += $2 } END { print s + 0 }' <<<"$list")
awk -F'\t' '
    BEGIN { printf "[" }
    {
        side = "server"
        if ($1 == "manifest.postcard") side = "both"
        else if ($1 ~ /^(atlas|sound)\//) side = "client"
        gsub(/\\/, "\\\\", $1); gsub(/"/, "\\\"", $1)
        printf "%s\n{\"path\":\"%s\",\"size\":%d,\"side\":\"%s\"}", (NR > 1 ? "," : ""), $1, $2, side
    }
    END { print "\n]" }
' <<<"$list" >"$out/data/files.json"

client_bytes=$(stat -c %s "$out/client/ss13_web_client_bg.wasm.gz")
server_bytes=$(stat -c %s "$out/server/ss13_web_server_bg.wasm.gz")
download_mb=$(((data_bytes + client_bytes + server_bytes + 524288) / 1048576))
commit=$(git rev-parse HEAD)
# shortcut: no source archive on the site yet (owner's decision while the code is not ready);
# AGPL-3.0 §6 requires offering the Corresponding Source to everyone the site conveys the wasm to,
# so add `git archive` here (and the page link) before showing the demo beyond trusted testers.
cp LICENSE ASSETS-LICENSE.md "$out/"
sed -i -e "s/__COMMIT__/$commit/g" -e "s/__DOWNLOAD_MB__/$download_mb/g" \
    -e "s/__WASM_BYTES__/{\"client\":$client_bytes,\"server\":$server_bytes}/" "$out/index.html"
echo "$out: data $((data_bytes / 1048576)) MB, client wasm $((client_bytes / 1048576)) MB," \
    "server wasm $((server_bytes / 1048576)) MB, download ~$download_mb MB"

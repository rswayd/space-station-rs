#!/usr/bin/env python3
"""Packs the browser demo's runtime data (scripts/web-build.sh, docs/RUNNING.md "Browser demo").

Thousands of small files (mostly sounds) cost a request each; the page downloads a few packs
instead. A pack is its files concatenated, named by its content hash (the host may cache it
forever, `_headers`), gzipped unless it is sound (Ogg does not compress, the host compresses no
unknown type). `files.json`:

    {"packs": [{"url", "size", "side", "later", "files": [[path, offset, length], ...]}]}

`side`: server, client or both (manifest.postcard). `later`: downloaded after the game starts
(the sounds but the lobby music: a sound not there yet is skipped).

Usage: web-pack.py <data dir> (packs every file into <data dir>/pack/, deletes the originals but
the attribution texts and licenses/, writes <data dir>/files.json)
"""

import gzip
import hashlib
import json
import os
import sys

# Cloudflare Pages rejects files over 25 MiB.
PACK_LIMIT = 24 * 1024 * 1024
# The lobby music plays as soon as the lobby shows (crates/ss13_game/src/lobby_music.rs).
UP_FRONT_SOUNDS = "sound/sound/music/lobby_music/"


def group(path):
    """(side, later, gzip) of a data file."""
    if path == "manifest.postcard":
        return ("both", False, True)
    if path.startswith("sound/"):
        return ("client", not path.startswith(UP_FRONT_SOUNDS), False)
    if path.startswith(("atlas/", "nanomap/")):
        return ("client", False, True)
    return ("server", False, True)


def shipped_only(path):
    """Attribution texts and licenses: linked from the page, never read by the game."""
    return path.endswith(".txt") or path.startswith("licenses/")


def main():
    root = sys.argv[1]
    files = []
    for dirpath, _, names in os.walk(root):
        for name in names:
            full = os.path.join(dirpath, name)
            path = os.path.relpath(full, root).replace(os.sep, "/")
            if not shipped_only(path):
                files.append(path)
    files.sort()

    groups = {}
    for path in files:
        groups.setdefault(group(path), []).append(path)

    os.makedirs(os.path.join(root, "pack"))
    packs = []
    for (side, later, gz), paths in sorted(groups.items()):
        chunk, body = [], bytearray()

        def flush():
            if not chunk:
                return
            data = gzip.compress(bytes(body), 9, mtime=0) if gz else bytes(body)
            name = hashlib.sha256(data).hexdigest()[:16] + (".bin.gz" if gz else ".bin")
            with open(os.path.join(root, "pack", name), "wb") as out:
                out.write(data)
            packs.append(
                {"url": "pack/" + name, "size": len(data), "side": side, "later": later,
                 "files": list(chunk)}
            )
            chunk.clear()
            body.clear()

        for path in paths:
            with open(os.path.join(root, path), "rb") as f:
                data = f.read()
            if len(data) > PACK_LIMIT:
                sys.exit(f"{path}: {len(data)} bytes, over the pack limit")
            if len(body) + len(data) > PACK_LIMIT:
                flush()
            chunk.append([path, len(body), len(data)])
            body += data
        flush()

    for path in files:
        os.remove(os.path.join(root, path))
    for dirpath, _, _ in sorted(os.walk(root), reverse=True):
        if dirpath != root and not os.listdir(dirpath):
            os.rmdir(dirpath)
    with open(os.path.join(root, "files.json"), "w") as out:
        json.dump({"packs": packs}, out, separators=(",", ":"))
    for kind, later in (("up front", False), ("later", True)):
        chosen = [p for p in packs if p["later"] == later]
        print(f"{kind}: {len(chosen)} packs, {sum(p['size'] for p in chosen) // 1048576} MB")


if __name__ == "__main__":
    main()

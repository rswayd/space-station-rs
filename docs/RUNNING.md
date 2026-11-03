# Running a local server and clients

All commands run from the repository root. `ASSETS` is any directory for generated data
(~170 MB); the default both binaries use is `assets/generated`.

## 1. Generate assets (once, and after the BYOND source changes)

```sh
ASSETS=assets/generated
cargo run -p ss13_dm_extract -- --out $ASSETS/dm_types.json               # DM type vars
cargo run -p ss13_assets --bin ss13-import -- dmi --out $ASSETS --types $ASSETS/dm_types.json
                                                                          # icons -> atlases + manifest
cargo run -p ss13_mapload --bin ss13-mapload -- convert --assets $ASSETS  # types -> prototypes
```

Licensing (`ASSETS-LICENSE.md`): `ss13-import` writes `$ASSETS/ICON-SOURCES.txt` (each packed
`.dmi` or GAGS icon: source `.dmi`, atlas pages, source commit) and copies the attribution/license
files of the source `.dmi` directories to `$ASSETS/licenses/icons/<source path>`; `convert` does the
same for sounds (`$ASSETS/sound/SOURCES.txt`). After either, refresh the audit of non-default terms:
`cargo run -p ss13_mapload --bin ss13-mapload -- audit --assets $ASSETS` (writes
`docs/ASSET-AUDIT.md`; the terms are in `crates/ss13_mapload/src/asset_terms.tsv`).

`--types` packs the `.dmi` files the maps place first (most placed first), so a client, which loads
2048x2048 atlas pages only when a sprite needs them, loads few pages. Without it the import still
works, in `.dmi` path order, and clients load more pages.

`--types` also renders tg's GAGS icons (`code/datums/greyscale/`, `ss13_assets::gags`): every
`greyscale_config` (and worn / in-hand config) some type uses, in its `greyscale_colors`, packed
like a `.dmi` named `gags:<config><colors>`; `convert` points those types' `icon` (with
`post_init_icon_state`), `worn_icon`, `lefthand_file` and `righthand_file` at them. The renders
match tg's own previews (`icons/map_icons/`) pixel for pixel:
`SS13_ASSETS=$ASSETS cargo test -p ss13_mapload --test it gags_real:: -- --ignored --nocapture`.
Screenshots of GAGS clothes, PDAs, hats and canisters, worn and held:
`SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/gags.dmm SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/gags_1.txt scripts/visual/gags_2.txt scripts/visual/gags_3.txt`.

The BYOND source defaults to `../BandaStation` next to the workspace (`.cargo/config.toml` `[env]`);
set `SS13_SOURCE` (or `--source`)
for another checkout.

## 2. Server

```sh
SS13_MAP=MetaStation SS13_ASSETS=$ASSETS cargo run -p ss13_server
```

- Binds `127.0.0.1:7777`: only clients on this machine can connect. To accept other machines set
  `SS13_BIND_ADDR=0.0.0.0:7777` (or a LAN address). **Unsafe on an untrusted network:** there is no
  real auth yet: the handshake accepts any non-empty token, so anyone who can reach the port can
  join under any name — including an admin's ckey (see "Admins"). Sessions are capped (1024 in total, 8 per IPv4 address or IPv6 /64), so one
  host cannot take every slot, but many hosts can. Bind publicly only on a trusted LAN or behind a
  firewall until an auth service issues tokens.
- `SS13_TLS_CERT=chain.pem SS13_TLS_KEY=key.pem`: present this certificate instead of a fresh
  self-signed one (same digest across restarts; clients still pin the digest).
- Logs `certificate digest <hex>` and writes it to `ss13-rs-dev-cert-<port>.txt` in a directory
  only this user can access: `$XDG_RUNTIME_DIR` (`/run/user/<uid>/ss13-rs-dev-cert-7777.txt`),
  else `$TMPDIR/ss13-rs-$USER/` created with mode 0700. It also prints
  `ss13-certificate-digest <hex>` on stdout (for a client that started it, see "Windows demo / releases").
- `SS13_SOURCE_URL=https://...`: where players get this server's source code. The code is
  AGPL-3.0-only, and section 13 requires offering the source of the running version (with your
  changes) to everyone who plays on it: the lobby shows the URL as "Исходный код". Unset, the
  server logs a warning at startup and the lobby shows no source line. Set it on every public
  server.
- `SS13_MAP_SEED=<u64>` fixes the random spawner rolls of the map load (default: new each start).
- `SS13_SPAWN_AT=x,y[,z]` (dev): every player spawns on that tile, for screenshots of a spot.
- Clients start in the lobby; `SS13_LOBBY_SECONDS`, `SS13_SKIP_LOBBY=1` and the round end: see
  "Lobby and rounds" below.
- `SS13_DEV_SPAWNS=alice=108,66;bob=98,72` (dev, debug builds only; a release build ignores it):
  the client whose handshake token is `alice` spawns on (108,66), whatever order clients join in.
- Every 10 s logs `replication: N clients, bytes/s per client ...`.
- `SS13_METRICS_ADDR=127.0.0.1:9100`: Prometheus metrics at `http://127.0.0.1:9100/metrics`
  (`crates/ss13_debug/src/metrics.rs`, also in the bot runner's servers; off when unset). Every tick: `ss13_tick_seconds`,
  `ss13_clients`, `ss13_entities`, `ss13_replication_seconds`,
  `ss13_network_sent_bytes_per_second`, `ss13_replication_{spawned,despawned,appearances_sent}_total`,
  `ss13_round_state` (0 startup, 1 lobby, 2 setting up, 3 playing, 4 finished),
  `ss13_round_duration_seconds`; every atmos fire: `ss13_atmos_fire_seconds`,
  `ss13_atmos_machinery_seconds` and by `z`: `ss13_atmos_level_fire_seconds`,
  `ss13_atmos_active_tiles`, `ss13_atmos_excited_groups`, `ss13_atmos_superconducting_tiles`.
  Durations are summaries (quantiles). New ones: `metrics::counter!`/`gauge!`/`histogram!` (the
  `metrics` crate; recorded once the server installed the exporter). The endpoint has no auth: keep it on loopback or behind a firewall.
  A Prometheus scrape config: `- job_name: ss13` / `static_configs: [{targets: ["127.0.0.1:9100"]}]`.

## 3. Clients (one per player, each in its own terminal)

```sh
SS13_ASSETS=$ASSETS SS13_TOKEN=alice cargo run -p ss13_client
SS13_ASSETS=$ASSETS SS13_TOKEN=bob   cargo run -p ss13_client
```

- `SS13_SERVER_ADDR` (default `127.0.0.1:7777`), `SS13_TOKEN` (any non-empty string for now).
- `SS13_CERT_DIGEST`: the server's digest. **Dev only:** when it is unset and the server address is
  loopback, the client reads the digest file the local server wrote. A remote server always needs
  `SS13_CERT_DIGEST`.
- `SS13_LOG_FPS=1` logs fps and frame time every second, and the longest frame of each second
  (`frame max N ms`: a once-a-second hitch the averages hide).
- `SS13_LIGHTING=0` turns tg lighting off (fullbright). Every 10 s the client logs
  `lighting: N sources ..., frame max T` (CPU time of the lighting systems). Lighting needs
  prototypes converted with this version (`ss13-mapload convert`, `dm_light` component);
  `scripts/visual/lighting.txt` shows the lit departure lounge and the dark maintenance next to it.
  Light sources and opacity reach the client per 16x16 light block from 11 tiles beyond the
  entity view (sources up to range 8 light the screen edge, shadows included);
  `scripts/visual/light_walk.txt` with `SS13_SPAWN_AT=100,68,1` walks along such an edge.
  A living mob sees what BYOND `view()` shows from its tile (line of sight through opacity,
  `ss13_shared` `line_of_sight`), plus a dimmed ring one tile past it next to seen floor (not
  tg: a glance around corners and door frames; nothing shows through a straight wall). The
  client computes both from its predicted tile and fades tiles in and out over 0.2 s; everything
  else is black. The server sends entities only on those tiles, seen from its own copy of the
  mob and, for 0.4 s after a step, from the next tile ahead too (so a predicted step finds its
  tiles there); the extra tiles stay black on the client. Light sources and opacity go unmasked,
  so walls cast true shadows (a modified client could read door states behind walls). Ghosts
  and admin ghosts see through walls (tg observer `sight`).
  Screens, bulbs and indicator lights glow in the dark through emissive overlays (round-start
  looks from `ss13-mapload convert`, `dm_look`). `scripts/visual/spot.txt` with `SS13_SPAWN_AT`
  screenshots any spot.
- `SS13_ATLAS_BUDGET_MB` (default 256): atlas pages no sprite has used for 10 s are freed, least
  recently used first, while the loaded pages take more than this. Pages in use always stay. The
  log has a line per page loaded or evicted with the resident total.
- `SS13_SIM_LATENCY_MS=50` (dev): delays every packet the process receives by that much. Set it
  for the server and the client to simulate a slow link (50 + 50 = 100 ms round trip); the own
  mob still walks at once (client-side prediction), other things show the delay.
- For screenshots and scripted input use the scripted client (next section), never OS tools.

Controls: arrows or WASD, two keys for diagonals; left click on the world clicks the topmost thing drawn under the cursor: only where a pixel of its current icon frame or of an overlay is not fully transparent (BYOND `mouse_opacity = 1`; a click on the empty corner of a tile with a small item goes to the floor), with pixel offsets and the turn of a lying mob, `mouse_opacity = 2` things anywhere in their icon, skipping tg's click-through atoms (`mouse_opacity = 0`: gas overlays, fires, many effects); every click is handled in order, one per tick per client (tg's 15 clicks per second limit), and a plain click does nothing during the mob's click cooldown (tg `next_move`: after a melee hit, a throw); the left button acts on release, as in BYOND: released over what it was pressed on it clicks (with an empty active hand an item next to the mob is picked up; with an item in hand a free tile next to the mob gets it), released over something else it drags (tg `MouseDrop`; a drag shorter than 16 icon pixels and 0.1 s is still a click, tg `check_drag_proximity`): another human dragged onto the own mob opens tg's strip menu (their slots; a slot with an item takes it off after its strip delay, 4 s, pockets 4 s; an empty slot with an item in hand puts it on after 2 s, pockets 1 s, hands too; cuffs come off through its handcuffs entry), a storage dragged from the HUD onto an empty hand slot goes into that hand, onto the own mob opens, onto another storage empties into it at once, onto a floor or an object empties onto that tile after 2 s, the active hand's item dragged onto a storage goes in, an item or a mob dragged from the floor onto an open closet goes onto its tile or (a mob, after 4 s) is stuffed in and the closet closes (stepping inside opens an unlocked closet again), and an item dragged onto another cell of the open storage grid moves there; a middle click swaps hands (tg `cycle_hand`), shift+middle click points (an arrow over the target for 2.5 s, "X указывает на Y.", once a second); alt+click is tg `AltClickOn`: a storage next to the mob opens (also on the floor), a PDA gives back its ID, anything else opens tg's loot panel (the tile's turf and everything on it; a click on an entry clicks it), as does alt+shift+click. X swaps hands, Q drops the active hand's item; the HUD has tg's human slots at their tg places: the bottom row (suit storage, ID, belt, back, hands, pockets) and, behind the toggle button at the bottom left, the clothes (uniform, suit, gloves, shoes, neck, mask, ears, glasses, head); a click on a hand selects it, a click on another slot equips the active hand's item there (tg `can_equip`: `slot_flags`, a jumpsuit for ID/belt/pockets, a suit for suit storage) or takes the worn item into the empty hand (taking off the jumpsuit drops ID, belt and pockets). Alt+click on a gas tank's slot toggles internals when a breath mask (or an internals helmet) is worn. Players spawn with a backpack holding their outfit's `box` (the survival box) and `backpack_contents` (a security officer's baton, ...): clicking the back slot with an empty active hand opens its storage grid above the HUD row (click an item to take it, the arrow closes it), clicking it with an item in hand puts the item in (tg weight class and slot limits). Closets, lockers and crates open and close on click next to them; their contents come from tg `PopulateContents()` on the first opening, secure lockers unlock with an ID that has their access. F toggles combat mode (also the combat button at the bottom right): a click on a mob next to yours with an empty hand punches it in combat mode (kicks it when it lies) and hugs it otherwise; with an item in hand it is hit with the item's force. The health indicator and the health doll on the right edge show the own health. A mob at 0 health or below lies in crit, at -100 it dies; a dead player's first step turns them into a ghost that only other ghosts see and that moves through walls. A client that disconnects leaves its body. Ctrl+click on something next to the mob (an item on the floor, a closet, a mob) pulls it: it follows the mob's steps (tg `Move`), a lying body or a closet slows the puller, the pull breaks at distance 2; ctrl+click on the pulled mob again is an aggressive grab (its steps and the resist key B are tg's 60% escape rolls, a failed step blocks moving for 4 s). C or Delete (or the HUD pull icon) stops pulling. R (or the HUD throw button) toggles throw mode: the next click throws the active hand's item at what was clicked (tg `throw_at`: `throw_range`, `throw_speed`); it hits the first dense thing or the clicked mob for its `throwforce`, a mob in throw mode with an empty hand catches it. Shift+click examines (title, description, size; for mobs clothes, held items, damage, crit, death), also from crit and as a ghost. In the say line `*scream`, `*nod Боб`, `*me текст` are emotes (`*help` lists them), `#` whispers, `;` (Common), `:h` (the headset's department), `:e`/`:у` and the other tg department keys (both keyboard layouts), `:i` (intercoms within 1 tile) and `:r`/`:l` (the held radio) talk into radios (see Radio below) and are whispered next to the speaker; a mob in soft crit can only whisper (heard in full at 1 tile, starred at 2), in hard crit it says nothing, a dead mob or ghost talks in deadchat. The window shows 19x15 tiles at zoom 2 left of the 480 px chat panel (window 1696x960).

## Windows demo / releases

A single-player package: unzip, run `ss13_client.exe`, play alone. `.github/workflows/demo.yml`
builds it: a `v*` tag publishes `space-station-rs-demo-windows-<tag>.zip` as a GitHub Release, a
manual run (`workflow_dispatch`) uploads the same tree as a workflow artifact. Jobs: `data`
(linux) checks out BandaStation at the commit of `ASSETS-LICENSE.md`, runs the three commands of
"Generate assets" and `scripts/demo-package.sh`; `windows` builds `ss13_client` and `ss13_server`
(release, `x86_64-pc-windows-msvc`); `package` puts the executables into that tree and zips it.

The package (`scripts/demo-package.sh <BandaStation> <assets> <out>` makes all but the
executables):

- `ss13_client.exe`, `ss13_server.exe`, `LICENSE`, `ASSETS-LICENSE.md`, `README.txt` (how to run,
  the source link and both commits);
- `assets/`: the converted assets without `dm_types.json` (converter input only);
- `data/BandaStation/`: only the BYOND files the server reads at runtime for MetaStation: the
  `.dmm`, `_maps/map_jsons/metastation.json` and the shuttle templates it loads (emergency
  `emergency_meta`, cargo `cargo_box`, the nuke ops infiltrator). Another map needs its files
  added to the script's list;
- `config/`: BandaStation's admin ranks and `admins.txt` with `dev = Host`.

Single-player launch (`crates/ss13_client/src/local_server.rs`): when `SS13_SERVER_ADDR` is unset
and a `data/` directory lies next to the client executable, the client starts the
`ss13_server` next to it before opening its window: bound to `127.0.0.1` on a free port, without
a console window of its own on Windows (its log goes to the client's stdout as `[server] ...`),
with `SS13_MAP=MetaStation`, `SS13_LOBBY_SECONDS=0` (the round runs, the player joins from the
lobby), `SS13_SOURCE_URL` set to this repository and the package's `assets/`, `data/BandaStation`
and `config/` (each of these only when not set already). The server prints
`ss13-certificate-digest <hex>` on stdout once it listens; the client reads it from the pipe (no
digest file) and connects. The local player (token `dev`) is the host admin through
`SS13_INSECURE_ADMINS=1`, safe only because nothing but this machine can reach a loopback port.
The client kills the server when it exits; if the client is killed, the server sees its stdin
close (`SS13_EXIT_WITH_PARENT=1`) and exits too. With `SS13_SERVER_ADDR` set (or no `data/`, as in
`target/`) the client behaves as in section 3.

Local check of the package on linux (debug binaries, the scripted client offscreen):

```sh
scripts/demo-package.sh $SS13_SOURCE $ASSETS /tmp/pkg && cp target/debug/ss13_{client,server} /tmp/pkg/
SS13_ADMIN_UI=players SS13_SCRIPT=$PWD/scripts/visual/demo.txt SS13_SCRIPT_OUT=/tmp/demo \
    SS13_OFFSCREEN=1 /tmp/pkg/ss13_client      # latejoins as an Assistant, the admin panel open
```

## Browser demo

A single-player static site (architecture: `docs/WEB.md`): the server runs in a Web Worker, the
client on a WebGL2 canvas, connected in memory; Chrome only. The landing page explains it and
links the source (AGPL §13, the exact commit) and the asset licenses; "Запустить игру" downloads
every file of `data/files.json` and both wasm binaries with a progress bar (~150 MB, no lazy
loading), starts the worker, waits for its `ready` and then starts the client. A reload resets
everything.

```sh
scripts/demo-package.sh $SS13_SOURCE $ASSETS /tmp/pkg   # the data, as for the Windows demo
scripts/web-build.sh /tmp/pkg dist                      # release wasm of ss13_web_server/_client
python3 -m http.server -d dist 8000                     # then open http://127.0.0.1:8000
```

`web-build.sh` needs the `wasm32-unknown-unknown` target (`rust-toolchain.toml` installs it),
`wasm-bindgen` at the exact version of the `wasm-bindgen` crate in `Cargo.lock` (it checks) and
`wasm-opt` (binaryen). `dist/`: `index.html`, `main.js`, `style.css`, `server-worker.js` (from
`web/`), `client/` and `server/` (wasm-bindgen `--target web` output after `wasm-opt -Oz`),
`data/` (the package's assets and `BandaStation/_maps/...` files, keyed by these relative paths)
and `data/files.json` (`[{path, size, side}]`, `side` = `server`, `client` (atlas pages, sounds)
or `both` (`manifest.postcard`)). Attribution texts are shipped but not downloaded up front.
Page/worker messages: `{t: "init", files, wasm}` page→worker (`wasm`: the server binary the page
already downloaded), `progress`/`ready`/`error` worker→page, then `open`/`frame`/`dgram`/`close`
both ways (`ArrayBuffer`s transferred).

`.github/workflows/web.yml` (manual run only) builds it and deploys it to Cloudflare Pages:
`data` (as in `demo.yml`), `site` (wasm build, `web-build.sh`, the 25 MiB per-file limit check,
`wrangler pages deploy`). It needs the repository secrets `CLOUDFLARE_API_TOKEN` (permission
"Cloudflare Pages: Edit") and `CLOUDFLARE_ACCOUNT_ID` and a Pages project `space-station-rs`.
The site ships `LICENSE` and `ASSETS-LICENSE.md`, but no source archive yet (see the shortcut in
`web-build.sh`): the site conveys the program, so AGPL-3.0 needs the source offered to its users
before the demo is shown beyond trusted testers.
`scripts/gate.sh` runs `cargo check --target wasm32-unknown-unknown` of `ss13_server` and
`ss13_client`, so native changes cannot break the wasm build unnoticed.

## Scripted client / visual checks

The client runs a script when `SS13_SCRIPT` is set: input is injected as Bevy messages
(`KeyboardInput`, `PointerInput`) into the real input systems and screenshots are taken with
Bevy's `Screenshot`, so nothing touches OS input or screen capture. Without `SS13_SCRIPT` nothing
is added. Code: `crates/ss13_devtools`.

One command: a private server on a free UDP port (a server on 7777 is untouched), one offscreen
client per script, everything killed at the end:

```sh
SS13_ASSETS=$ASSETS scripts/visual-check.sh -o /tmp/vc scripts/visual/walk_and_say.txt scripts/visual/walk_and_say.txt
```

Client N joins with the token `clientN`; a script line `# spawn: x,y[,z]` spawns that client on
the tile (`SS13_DEV_SPAWNS`), so several scripts run together do not depend on the join order.
It prints each client's result, its `log_state` lines and screenshot paths; the output dir holds
`server.log`, `clientN.log` and `clientN/*.png`. Exit code 0 when every script succeeded.
Env: `SS13_MAP` (default `MetaStation`, empty = empty world), `VISUAL_CHECK_WINDOWED=1` (real
windows), `VISUAL_CHECK_TIMEOUT` (seconds per client, default 300), `VISUAL_CHECK_NO_BUILD=1`.

A single client by hand:

```sh
SS13_SCRIPT=scripts/visual/walk_and_say.txt SS13_SCRIPT_OUT=/tmp/shots SS13_OFFSCREEN=1 \
  SS13_ASSETS=$ASSETS SS13_TOKEN=alice cargo run -p ss13_client
```

- `SS13_SCRIPT_OUT`: screenshot directory (default `.`).
- `SS13_OFFSCREEN=1`: no OS window at all (winit is not started); cameras render into an image of
  the window's size (1696x960) and screenshots read it. Needs a GPU adapter (Vulkan/GL; a software
  one such as lavapipe works for CI). Without it the client opens a normal window that keeps
  running at full rate when unfocused; on Wayland a minimized window may stop presenting frames,
  so prefer offscreen.
- Exit code: 0 on success (`quit` or end of script), 1 when a step fails (the log has
  `script: FAILED at line N ...` with a state dump and `failure.png` is written), 2 when the script
  does not parse.

Script format: one step per line, `#` comments, `"quoted strings"` (`\"` escapes). Waits take an
optional trailing `timeout=<secs>` (default 30 for `wait_connected`, 10 for `wait_until`).

| Step | Effect |
| --- | --- |
| `wait_connected` | until the handshake is accepted |
| `wait_secs 1.5` | real time |
| `wait_until controlled` / `entities N` / `chat "text"` | own mob exists / at least N replicated entities / a chat entry contains text |
| `key_down W`, `key_up W`, `tap Enter` | keys: `A`-`Z`, `0`-`9`, `Numpad0`-`Numpad9`, `Enter`, `Escape`, `Space`, `Tab`, `Backspace`, `Delete`, `Up`/`Down`/`Left`/`Right`, `Shift`, `Ctrl`, `Alt`, `F1`-`F12` |
| `type "Привет"` | one key press per character with that text, as a keyboard layout produces it |
| `click_tile dx dy [px py]` / `click_world x y [px py]` | left click on a tile relative to the own mob / an absolute tile on its z, at its center or at icon pixel `px py` (0-31 from the bottom-left corner) |
| `rclick_tile dx dy` | right click on the center of a tile relative to the own mob |
| `click_ui "text"` | left click on the first visible UI node whose text contains it (or whose `Name` equals it) |
| `drag_tile dx dy dx2 dy2` | left button pressed at the center of a tile relative to the own mob, moved, released at the center of another (a click-drag) |
| `scroll_ui "text" N` | mouse wheel over that node, `N` lines down (negative: up) |
| `screenshot "name.png"` | waits until the atlas pages requested so far are loaded, then until the file is written |
| `log_state` | one log line: connection, own tile, replicated entities, visible sprites, atlas pages loaded/total and their MiB, open windows, focus in a text field, fps, last 5 chat lines |
| `quit` | exit 0 after pending screenshots |

Example (`scripts/visual/walk_and_say.txt`):

```text
wait_connected timeout=60
wait_until controlled timeout=30
screenshot "before.png"
key_down D
wait_secs 0.9
key_up D
click_ui "Сообщение"        # the say line's placeholder: focuses the field
type "Привет, станция!"
tap Enter
wait_until chat "Привет, станция!" timeout=5
screenshot "said.png"
log_state
quit
```

To look at a screenshot, open the PNG (an agent reads it as an image file).

## Per-system profile

Which systems take the tick (server) or the frame (client), summed in process, nothing on disk:

```sh
cargo build -p ss13_server -p ss13_client --features ss13_server/profile,ss13_client/profile
SS13_PROFILE=1 SS13_MAP=MetaStation SS13_ASSETS=$ASSETS target/debug/ss13_server
SS13_PROFILE=1 SS13_ASSETS=$ASSETS SS13_TOKEN=alice target/debug/ss13_client
```

Every 10 s each prints `profile:` lines: the frame count, the average `Main` schedule time, the
sum of all system runs, and the top systems (`SS13_PROFILE_TOP`, default 25) with time per frame,
share of `Main`, average and slowest run, and run count. The `profile` feature turns on Bevy's
system spans, which cost too: the absolute numbers run high, the ranking holds. Render-world
systems run in parallel with `Main` on the client, so their shares can pass 100%. Bevy's
`trace_chrome` writes every span to a file instead: gigabytes a minute for the client.

Under load: the bot runner takes the same switch, its server and bots share the report (read the
average run, `Main` counts every app's frames). A case with N bots walking on a station map:

```sh
SS13_PROFILE=1 SS13_ASSETS=$ASSETS cargo run -p ss13_bots --features ss13_bots/profile \
    --bin ss13-bots -- --jobs 1 load30.case
```

## UI gallery

Every tgui window the client knows, without a server or assets:

```sh
cargo run -p ss13_client --bin ui_gallery                      # the list; click a name to open it
cargo run -p ss13_client --bin ui_gallery -- Canister          # and open one at once
cargo run -p ss13_client --bin ui_gallery -- Canister --screenshot out.png   # PNG, then exit
```

Windows open with sample data (`crates/ss13_tgui/src/samples.rs`); their buttons change nothing,
the list shows the last action and logs it. A new interface needs a sample there:
`cargo test -p ss13_tgui --features client --test samples` fails for a view without one.

## Debug UI

An egui overlay in the client (`crates/ss13_debug`): built into debug builds and the browser
demo; a release client has it only with `cargo build -p ss13_client --release --features
debug-ui`. **F9** (or the backtick key when no text field has focus) opens and closes it; not
F12, which browsers keep for their developer tools. Closed, it draws nothing, takes
no input and the network tap is off. Open, the "ss13 debug" bar toggles the panels (they stay as
left for the session); egui takes the mouse over its windows (the map under them gets no
clicks) and the keyboard only while one of its text fields has focus.

"⏺ Записать видео" in the bar (native client only, needs `ffmpeg` in `PATH`) records the window
until "⏹ Стоп": `ss13-recording-<unix time>.mp4` (H.264, 30 fps, real time: a slower game repeats
frames) in `SS13_DEBUG_DIR`, default the temp dir; the client log names the file when it is done.
Recording goes on with the overlay closed (F9), so the video can show the game alone. A resized
window's frames are skipped; frames are dropped if ffmpeg falls behind.

| Panel | Shows |
| --- | --- |
| Perf | fps, frame time and a graph of the last 300 frames, client entity count, replicated entities, cached appearances, light blocks, atlas pages loaded/MiB (budget), the lighting numbers of the 10 s log line, live |
| Network | connection state, RTT, bytes/s in/out, QUIC UDP totals and lost packets; per message kind (`Intent::Move`, `Ui::Open`, `Replication`, `Motion`, `Debug::Tile`, ...) counts and bytes in/out; a log of the last 5000 messages (time, direction, kind, size, a `Debug` summary; replication batches as op counts) with pause, clear, filters (direction, kind, text) and Export (`ss13-packets-<unix time>.tsv` in `SS13_DEBUG_DIR`, default the temp dir). The tap is on only while this panel is shown; nothing is formatted otherwise |
| Entities | replicated and in-view counts; with "click selects a tile" on, a click on the map selects a tile instead of reaching the game: its entities (NetId, server name, DM icon/state, server components, dir, icon state, layer/plane, pixel offset, rotation, color, overlays), and "inspect components" lists every client component of one, with its reflected value |
| Prediction | sent and acked move seq, pending moves, corrections (acks that disagreed with the prediction), predicted and server tile |
| Tile | the tile under the cursor (else the selected one): turf, area, power channels and loads, air (pressure, temperature, moles, gases), from the server |
| UI | open tgui windows (id, interface, title, status) with their static data and data as JSON (the interfaces this client knows), and the last 50 acts sent |
| Log | the client's log lines (what passes `RUST_LOG`) with level, target and text filters |
| Server | server tick time p50/p99/max over 10 s, entities, round state, time scale, per client bytes/s, packets/s and visible entities; dev tools: teleport the own mob to the selected tile, god mode, time scale (0.1-10), end the round (tg `force_ending`), spawn an `/obj` prototype searched by path on the selected tile |

Server side (`ss13_debug::server`, always in the server binary): debug queries (tile details,
stats, prototype search) are answered in debug builds, or in any build with
`SS13_DEBUG_QUERIES=1`; `SS13_DEBUG_QUERIES=0` turns them off in a debug build. Dev tools need
queries on **and** a debug server build: a release server refuses them whatever the
environment says. Tile queries are not limited to what the asking client sees, so
`SS13_DEBUG_QUERIES=1` leaks the whole station to every client: trusted test servers only.
Requests are rate-limited per client (10/s, burst 20; the excess is dropped). Dev tool use is
logged (`dev: ...`).

- `SS13_DEBUG_UI=1` starts the client with the overlay open; `SS13_DEBUG_UI=net,entities`
  also opens those panels (`perf`, `net`, `entities`, `prediction`, `tile`, `ui`, `log`,
  `server`). Scripted runs use it: egui gets no injected input, but map clicks (the entities
  panel's tile pick) work.

```sh
cargo test -p ss13_debug --all-features    # packet log ring/filter/counters, rate limit; over real
                                           # transport: tile query (air, power, entities), dev
                                           # tools applied with the gate open, refused when closed
SS13_DEBUG_UI=net,entities SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/debug_ui.txt
SS13_DEBUG_UI=perf,tile,prediction,server,log SS13_ASSETS=$ASSETS scripts/visual-check.sh \
    scripts/visual/debug_tools.txt
```

## Admins

Real admin tools in every build (`crates/ss13_admin`), after tg `/datum/admins` and BandaStation's
ticket manager. Separate from the debug overlay's dev tools, which stay debug-only.

- **Who is an admin.** At start the server reads `admin_ranks.txt` and `admins.txt` from
  `SS13_CONFIG_DIR` (default `config`; the repo's `config/` has BandaStation's ranks and an empty
  admin list), in tg's text format: ranks with `Include`/`Exclude`/`Edit` keywords (`ADMIN`,
  `BAN`, `FUN`, `SERVER`, `DEBUG`, `SPAWN`, `VAREDIT`, ..., `@`, `EVERYTHING`; tg `R_*` bits) and
  `key = Rank` (or `Rank1+Rank2`) lines. A client is matched by the ckey of its handshake token
  (tg `ckey()`: `The Admin` → `theadmin`). **There is no real auth yet: token = ckey, and the
  handshake accepts any token**, so anyone can claim an admin's ckey. Listed admins therefore get
  no session (a loud warning at start, `ACCESS` log lines) unless the server trusts every client:
  `SS13_INSECURE_ADMINS=1` (or a `TokenValidator` that authenticates, once an auth service exists).
  Set it only on trusted servers.
- Ranks without `AUTOADMIN` connect deadminned (tg); `Readmin`/`Deadmin` in the panel.
- **Checks.** Every request is validated on the server against the client's active rights when it
  is applied (`ss13_admin::server::required_rights`); a player's admin request is refused and
  logged. Requests are rate-limited (10/s, burst 20). Kicking, banning, muting or smiting an admin
  needs strictly more rights (tg `check_if_greater_rights_than`).
- **Log.** Every admin action goes to `SS13_ADMIN_LOG` (default `logs/admin.log`, empty: off) as
  `[UTC time] ADMIN: ...` (`ASAY`, `ADMINPRIVATE` for tickets, `ACCESS` for logins), and as tg
  `message_admins` ("ADMIN LOG: ...") to the online admins with `+ADMIN`.
- **Bans and mutes** are in memory, by ckey: they last until the round reboot (a new server
  `App`) or a restart (no database). A banned ckey is kicked on connect; a muted one is muted
  again when it reconnects. Banning an offline admin also needs strictly more rights than its
  rank. A kick or ban closes the session
  for good: the client shows the reason and does not reconnect.

Client: **F1** opens the adminhelp window (any player): a new ticket, or the open ticket's
conversation and a reply line. **F7** opens the admin panel, only after the server confirmed
admin rights; **F6** is aghost. Panel tabs: players (ckey, name, job, antag, alive/dead/ghost,
tile; JMP, GET, FLW, PM, Mute, Kick, Ban, Heal, Smite, Traitor, Brother, with the shared
message/reason line), tickets (list, conversation, reply, close/resolve/reopen), game (spawn by
prototype search under yourself, force event, Central Command announcement, end round, reboot,
time scale, view-only dynamic summary), and an asay line on top. Buttons follow the rights; the
server checks them anyway. `SS13_ADMIN_UI=help,players,tickets,game` opens windows at start.

Tickets (BandaStation): an adminhelp opens ticket #N for every online admin (chat "Тикет #N");
an admin's reply links the admin; until then the player cannot write more; then one message per
5 s; an idle ticket closes after 20 minutes; an admin PM to a player without a ticket opens a PM
ticket. Messages arrive in the chat (`ChatChannel::AdminPm`), the windows show the conversation.

```sh
cargo test -p ss13_admin --all-features    # rank/admin parsing, rights; over real transport: a
                                           # player's requests refused, deadmin/readmin, jump,
                                           # spawn, make traitor, kick (no reconnect), the admin
                                           # log, an adminhelp answered and resolved
mkdir -p /tmp/adm && cp config/admin_ranks.txt /tmp/adm/ && echo "client1 = Game Master" > /tmp/adm/admins.txt
SS13_CONFIG_DIR=/tmp/adm SS13_INSECURE_ADMINS=1 SS13_ADMIN_UI=tickets SS13_ASSETS=$ASSETS scripts/visual-check.sh \
    scripts/visual/admin_panel.txt scripts/visual/adminhelp.txt
```

## Map checks without a window

```sh
cargo run -p ss13_mapload --bin ss13-mapload -- load --assets $ASSETS --seed 1 MetaStation  # report
cargo run -p ss13_mapload --example map_shot -- --assets $ASSETS --seed 1 \
    --rect 51,125,66,140 --scale 2 --out hallway.png   # PNG of tiles x 51..66, y 125..140
```

`--raw` on either skips the load passes (spawners, undertile, smoothing) for before/after.

## Tests

The full gate is `scripts/gate.sh` (`--no-bots` skips the seed suite): `cargo fmt --check`,
`cargo deny check licenses sources bans` (dependency licenses must be AGPL-compatible, `deny.toml`),
`cargo lint`, `cargo nextest run --workspace --all-features --no-fail-fast`, `cargo bins` and the
bot seed suite, each step timed. It needs `cargo-nextest` and `cargo-deny` (`cargo install
--locked cargo-nextest cargo-deny`) and assets converted from the same tree (`SS13_ASSETS`, default
`assets/generated`).

- **Aliases** (`.cargo/config.toml`): `cargo lint` (clippy, `-D warnings`), `cargo nt` (nextest),
  `cargo bins` (server, client and tools). They always build the whole workspace with all
  features. A narrower selection can resolve another feature set and compile Bevy and the game a
  second time: `cargo build -p ss13_server -p ss13_client` after a test build took 157 s and
  several GB, `cargo bins` 1 s. `cargo test -p ss13_game --all-features --test it` reuses the
  workspace build.
- **Integration tests are one binary per crate**: `crates/ss13_game/tests/it/` (one module per
  area, listed in `main.rs`) and `crates/ss13_mapload/tests/it/`. A new area is a new module
  there, not a new `tests/*.rs` file: every test binary links Bevy and the game (~350 MB, several
  seconds).
- **While working**, run only your area: `cargo nt -E 'test(/^cult::/)'`, or
  `cargo test -p ss13_game --all-features --test it cult::` for `--nocapture`/`--ignored` runs.
  nextest runs every test in its own process, all binaries in parallel (the whole suite takes
  ~15 s); it has no doctests and the workspace has none.
- **Seed suite parallelism**: `SS13_BOTS_JOBS` servers (default 6, so several agents can run it at
  once; 12 on an idle machine).

### Integration-test servers (`ss13_game::testing`)

Every integration test that starts a server uses `ss13_game::testing` (feature `testing`: the
`it` binary requires it; another crate adds the feature to its `ss13_game` dev-dependency). It makes the server deterministic:

- **Game time.** `TimeUpdateStrategy::ManualDuration(TICK)`: one update is one 50 ms tick on
  the server and on its clients, whatever the machine load. Timers, cooldowns and per-second
  rates count ticks, so a test asserts game time (`Station::now()`), never wall time.
- **One seed.** Every game RNG derives from `MapSeed` (`ss13_game::server::seed_rngs`, run in
  `GameServerPlugin::finish`: `CombatRng`, `EngineRng`, the records `Manifest`, chat's
  `SpeechRng`; spawners, closet contents, wires, jobs, cargo, shuttle, dynamic and events read
  `MapSeed` themselves). Test servers use `SS13_SEED` (default 1): rerun a failure with another
  seed to explore, with the same one to reproduce. A test that needs a particular roll inserts
  the RNG after `Station::start` (`finish` would overwrite an earlier insert). Production keeps
  `SS13_MAP_SEED` or the clock.
- **No random events** unless a test forces one (`EventsConfig::forced_only`).
- **The network stays real** (QUIC on its own threads). A test never sleeps: it steps until a
  condition holds. A wait fails only after its budget of game time *and* as many wall-clock
  seconds are spent, so neither a fast machine (network not done yet) nor a loaded one (game
  not far enough yet) fails it early. Intents still arrive after a variable number of ticks:
  assert "eventually" with `run_until`, and "never" with `hold` after the effect it guards
  against could have happened.
- **In-memory transport** (`ss13_net::loopback`, the browser build's transport, `docs/WEB.md`): a
  test that inserts `loopback::listener()`'s listener *and* connector into the server app in the
  `setup` closure gets clients joined over channels instead of QUIC (the server opens no socket);
  same handshake, frames and limits, pumped by `update` (`it` `loopback::`).

```rust
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};

struct Station(testing::Station);          // add fields for the test's own state
ss13_game::drive_station!(Station, 0);     // Deref to testing::Station + Drive

impl Station {
    fn start(test: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mytest").join(test);
        let assets = write_fixture(&dir, TYPES, &USED, &STATES, &[("map.dmm", MAP)]);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            // Resources and plugins before the game plugins (they override the defaults).
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(2, |client| {                // tokens p0, p1; waits until each has a mob
            client.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }
    fn said(&self, p: usize, text: &str) -> bool { /* reads players[p].app */ }
}

let mut s = Station::start("hug");
let b = s.players[1].mob;
s.click(0, b);                                 // send / act / try_step / click_with
s.run_until("B is hugged", |s| s.said(1, "обнимает"));   // closures get your Station
s.hold(0.5, "no damage", |s| s.get::<Health>(b).health() == 100.0);  // game seconds
s.run_for(2.0);
```

Joining (`connect`, `join`, `join_all`) stops game time until the handshake is done: mobs
already in the world do not age by however long it took on this machine. Through `DerefMut` a
call cannot borrow the station twice (`s.server.world_mut()...(s.players[0].link)`): copy the
value into a `let` first, or name the field (`s.0.server`).

`testing::server_app(map, setup)` alone gives a started server for server-only tests (step it
with `app.update()`, one tick each); `Station::connect` joins without waiting for a mob (lobby
tests); `Drive::context` adds diagnostics to a failed wait. The functional bots' pooled servers
(`ss13_bots::server`) use the same `testing::deterministic` (seed, game time, events).

```sh
cargo test -p ss13_game --all-features                     # fixture station over real WebTransport
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it play:: -- --ignored --nocapture
                                                            # join and walk on MetaStation, bytes/s
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it doors:: -- --ignored --nocapture
                                                            # leave the MetaStation spawn room through
                                                            # a door; door counts by kind;
                                                            # `metastation_access`: doors with
                                                            # access by department, assistant ID
                                                            # denied at command/security, opens maint
```

```sh
SS13_ASSETS=$ASSETS LOS_CLIENTS=50 cargo test -p ss13_game --all-features --test it los:: -- --ignored --nocapture
                                                          # 50 clients walking on MetaStation:
                                                          # replicate pass time, bytes per client,
                                                          # line-of-sight cost
```

```sh
cargo test -p ss13_game --all-features --test it border::      # directional windows, a windoor desk,
                                                          # a multi-tile airlock (fixture)
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it border:: -- --ignored --nocapture
                                                          # walk through a MetaStation windoor
```

Border objects (tg `ON_BORDER_1`: directional windows, windoors, railings, directional firelocks)
block only the edge they face, entering and leaving; windoors open when bumped across that edge.

Doors open by walking into them or by clicking one next to the mob, if the mob's ID (active hand, other hand, ID slot) has the door's access; players spawn with an assistant ID (maintenance access) and other jobs get their trim's access (see Jobs below). Door access comes from the map's `req_access`/`req_one_access` edits, door type defaults and the tg airlock access helpers (`/obj/effect/mapping_helpers/airlock/access/{any,all}/...`, whose `get_access()` additions `ss13-dm-extract` dumps per type as `get_access`); `unres` helpers open their side without access.

```sh
cargo test -p ss13_game --all-features --test it combat::      # hug, toolbox, punches to crit, death, ghost
```

```sh
cargo test -p ss13_game --all-features --test it interact::    # drag a dead body 4 tiles, throw a toolbox
                                                          # at it; grab + resist; examine; crit
                                                          # whisper falloff, emotes, deadchat
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/arena.dmm SS13_ASSETS=$ASSETS scripts/visual-check.sh \
    scripts/visual/pull_puller.txt scripts/visual/pull_victim.txt   # screenshots of a dragged body
```

Health and combat on screen (two clients on a small arena map, attacker and victim, fullbright):

```sh
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/arena.dmm SS13_ASSETS=$ASSETS scripts/visual-check.sh \
    scripts/visual/combat_attacker.txt scripts/visual/combat_victim.txt
```

Pixel-accurate clicks: shift+clicks on a pen and on the transparent pixels around it examine the pen
and the floor (the last `log_state` chat line is "Это floor."):

```sh
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/pixel_click.dmm SS13_ASSETS=$ASSETS scripts/visual-check.sh \
    scripts/visual/pixel_click.txt
```

```sh
cargo test -p ss13_game --all-features --test it inventory::   # pickup/drop/swap/equip, door access
cargo test -p ss13_game --all-features --test it drag::        # strip menu (uncuff, put an item on),
                                                          # backpack dump, mob into a closet,
                                                          # loot panel, pointing, middle click
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/arena.dmm SS13_ASSETS=$ASSETS scripts/visual-check.sh \
    scripts/visual/strip_stripper.txt scripts/visual/strip_victim.txt  # strip menu, loot panel
cargo test -p ss13_game --all-features --test it vending::     # vending window, vend checks, spam, close
cargo test -p ss13_game --all-features --test it storage::     # open a locker, item into the backpack and out
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it storage:: -- --ignored --nocapture
                                                          # MetaStation: closet/storage contents coverage,
                                                          # the HoP's and captain's lockers opened
SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/storage.txt   # open locker, storage grid
cargo test -p ss13_game --all-features --test it power::       # fixture grid: SMES -> APC, outage
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it power:: -- --ignored --nocapture
                                                            # MetaStation nets, APC coverage,
                                                            # supply/load, outage of a real area
cargo test -p ss13_game --all-features --test it engine::      # fixture: emitter -> supermatter -> coil
                                                          # -> SMES; SolarControl window; delam
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it engine:: -- --ignored --nocapture
                                                            # MetaStation: solars wired and tracking,
                                                            # engine emitters on, kW and SMES charge
                                                            # per minute
SS13_SKIP_LOBBY=1 SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/engine.txt
                                                            # the engine room, an emitter turned on
```

Vending machines open the tgui `Vending` window on a click with an empty active hand next to
them; products cost nothing yet (vending does not charge accounts), an ID is still required. A vendor without power
goes dark (`-off` sprite, no glow), opens nothing and refuses to vend from an open window. `scripts/visual/vending.txt`
spawns under the Getmore machine in the MetaStation departure lounge, switches the layout, searches,
vends and walks away.

Power follows tg: every 2 s the SMES feed their powernets, APCs pay their area's machine loads
from the grid, then from their cell, and switch channels off by cell charge (equipment below 30%,
lighting below 15%, everything at 0). An area without power has its lights on emergency cells
(dim red, ~10 minutes, then dark) and its airlocks and windoors stay shut. At round start nothing
generates power (the engine needs setting up, as in tg): the station runs on the SMES charge.

Generation (`ss13_game::power::{solar, emitter, supermatter, accumulator}`), as tg:

- **Solars.** The sun turns 6° × a random 0.5-2 (either way) per minute. A panel makes
  2500 W × cos(panel − sun) when the station does not shade it (checked when it turns: 20 steps
  toward the sun hit an opaque tile). Map panels face north and belong to no console; the
  `SolarControl` window (click the console with an empty hand) scans its powernet for panels and
  a tracker ("Scan for new hardware"), sets the azimuth, a timed rate or auto tracking. MetaStation's
  arrays come unwired, as in tg: the runs stop short of the consoles and the rows are split.
- **Emitters.** Welded and wired, a click turns one on: 300 W from its net, bursts of four shots
  2 s apart, then a 2-10 s pause. The beam passes glass, grilles, tables and glass doors, turns at
  reflectors and stops at the first other dense thing (30 burn to a mob). Wrench anchors, welder
  welds (2 s, 1 fuel).
- **Supermatter.** Each SSair fire (0.5 s) the crystal takes 15% of its tile's air: the gases set
  its heat, power and transmission factors; an emitter hit adds 60 MeV; energy decays as
  (E/500)³; waste heat, plasma and oxygen go back into the tile. Its energy × 1040 W/MeV (plus gas
  bonus) accumulates and discharges in arcs (under 240 kJ they fizzle): a tesla coil keeps 25% of
  an arc for its powernet (released over minutes, `dE/dt = −0.04E − 2000 W`), a grounding rod
  swallows the rest. Damage comes from heat over its limit, energy over 5 GeV, moles over 1800
  and space; over 5 it warns on the engineering channel each minute (common in an emergency),
  over 100 it counts down 15 s and delaminates — a stub: everyone on the level is told and the
  crystal is deleted. Without its cooling loop running, a crystal overheats.
- **Cooling the supermatter (MetaStation).** The loop is mapped empty and mostly off, as in tg:
  an engineer wrenches nitrogen canisters to the loop's connector ports (or feeds it from
  atmos), turns on the loop pumps (max pressure) and the two loop filters by the chamber (mapped
  off), keeps the radiator bypass pump (172,149) off, and sets the chamber scrubbers in the
  engine's air alarm. The radiator pumps (171,148)/(173,148) are mapped on: the gas then runs
  through the space radiator, 67 heat exchanging pipes east of the engine, which radiate to
  space (tg `temperature_interact`: 2.7 K, `HEAT_CAPACITY_VACUUM`); the ignored atmos test
  shows the radiator at ~50 K and the chamber cooling from 293 K within a minute.

Heat exchanging pipes (`/obj/machinery/atmospherics/pipe/heat_exchanging/*`) connect only to
each other and, through a junction, to plain pipes on the junction's back; bends follow their
diagonal `dir`. Every SSair fire each one whose tile differs from its gas by more than 20 K
(junctions 300 K, conductivity 0) exchanges heat with the tile: space takes it and stays at
2.7 K, an open tile's air warms or cools and wakes.

Chargers (`ss13_game::power::chargers`, `ss13_game::mecha::bay`): a cell charger takes a power
cell clicked onto it (anchored, empty, its area powered) and charges it at 2.5 kW from the
grid's surplus only, showing the charge in quarters; an empty hand takes the cell back. A
recharger takes a stun baton and charges its cell at the cell's `chargerate` from the grid,
then the APC cell, and says when it is full. A mech bay power port charges the cell of a mech
on the tile it faces at 625 W (grid only) while powered and linked to its console; the console
(empty hand) opens `MechBayPowerConsole` with the mech's integrity and charge. Printed cells
are empty, as in tg (`/obj/item/stock_parts/power_store/cell/empty` designs): charge them here.

```sh
cargo test -p ss13_game --all-features --test it mecha::cell_charger   # charger and mech bay rates,
                                                                   # the console window (fixture)
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter he_pipes
                                                                   # a canister fills an HE loop
                                                                   # in space; it cools
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/mech_bay.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/mech_bay.txt            # the console window
```

## Functional test bots

Headless bots play scripted scenarios against real servers (`crates/ss13_bots`, binary
`ss13-bots`). A bot is a real client: `ss13_net`'s client over the real WebTransport transport,
no window, renderer or GPU, many per process. It acts only through `ClientIntent`s and sees what
a player sees: its replicated entities, chat, tgui windows and the HUDs (the HUDs are tgui
interfaces without windows). Assertions on server state go through the debug channel's test
hooks (`ss13_debug::test_hooks`, `DebugRequest::Test`), answered only with dev tools on: a debug
server build (see "Debug UI").

```sh
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --jobs 4      # every tests/cases/*.case
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter door --filter radio
cargo run -p ss13_bots --bin ss13-bots -- --list                           # cases and their tests
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --json report.json --junit junit.xml
cargo test -p ss13_bots                     # parser, conditions, scheduler, reports; the runner
                                            # end to end on fixture assets (2 cases, ~3 s)
SS13_ASSETS=$ASSETS cargo test -p ss13_bots --test seed -- --ignored --nocapture
                                            # the seed suite, --jobs 2 (~30 s)
```

Options: `--jobs N` (default: half the cores), `--filter TEXT` (case names containing it,
repeatable), `--list`, `--out DIR` (default `target/ss13-bots`: `logs/<case>.log` and the
reports), `--json FILE`, `--junit FILE` (relative to `--out`), `--assets`, `--source` (for
station maps). Paths after the options are case files or directories (default `tests/cases`).
Exit code 0 when every test passed, 1 when one failed, 2 on bad arguments or a case file that
does not parse (`file:line: message`). Each case prints a line, then one per test; a failure
names the step's line and text, expected vs actual, and the last 8 chat lines of every bot of
the case. `logs/<case>.log` has every step with its time, the server's log lines of that case
and the full chat of each bot.

Seed suite (`tests/cases/`, 34 cases): `walk_pickup`, `los` (nothing behind a wall
reaches a player's client, the open airlock shows the room behind it, closing hides it again, an
admin ghost sees through), `strip` (cuffing, the strip
menu takes the cuffs and the ID off, a backpack dumped onto the floor, a mob stuffed into a
closet), `storage` (backpack in/out, bulky item refused), `closet` (security locker: assistant
refused, officer unlocks), `door` (brig access, bolts dropped by pulsing the wire), `radio`
(local speech, headset, no headset), `medical` (toolbox hit, bruise pack heals the chest),
`vending` (window, vend, window status by distance), `fire` (plasma lit by a welder burns the
bot), `chem` (pour, ammonia reaction), `hunger` (nutrition drops, eating raises it), `janitor` (water poured into a mop bucket, the
mop wetted from it, a pool of blood mopped off, the floor left wet),
`research` (iron and glass into a protolathe, unlocking the R&D console, researching a node,
printing its design), `engine` (emitter on, the crystal's arcs charge a coil and a SMES, the
solar console links its panel), `atmos` (an air canister wrenched to a connector port, the vent
refills a vacuumed room to ~101 kPa, its air alarm's warnings, the canister window's valve),
`silicon` (the AI opens and bolts an airlock its camera sees, not after its AI wire is cut; the
AI gets a wrench behind a door only while the door is open; a camera console moves the view to a
far camera and back), `cyborg` (linked to the AI's laws, model
choice, a module out of the model and back, picks nothing up, cell drain, recharging station),
`surgery` (a cut bleeds until gauze soaks it up, tend wounds on an operating table through the
operation list, a defibrillator revives a fresh corpse), `brothers` (a blood brother's panel and
pocket flash, a flash converts a second bot, each one's antag HUD lists the other; exclusive),
`admin` (a player's admin verb refused; an adminhelp answered by an admin PM and resolved; jump,
get, spawn, make a traitor, kick), `events` (exclusive: a forced Grid Check cuts the room's APC, a Communications Blackout silences
the headsets),
`records` (the HoP gives a card security access in the ID console and it opens a security door;
an officer adds a crime in the security records console and its security HUD shows the wanted
icon), `vampire` (a vampire's panel, action buttons and blood counter, a bite drains the victim's
blood into usable blood, the glare spends a charge; 10x time), `changeling` (the panel, buttons
and chemical counter, the `:g` hivemind, an aggressive grab and the absorption husk bob and store
his profile, the emporium sells gene stim and it is used; 10x time), `cult` (the panel and buttons, Communion, the dagger refused at CentCom, two cultists cannot take the mindshielded target on an Offer rune, three sacrifice her), `heretic` (the research window and buttons, an influence replicated to the heretic and not the crew, drained by a right click into a pierced reality the crew sees, the Path of Ash researched), `disposals` (an item flushed from
the DisposalUnit window reaches an outlet, a wrapped
parcel tagged for medbay takes its sort junction's branch, a mob dragged into a bin rides to an
outlet), `mobs` (petting a corgi, a carp hunts a bot down and bites him), `mecha` (the exosuit fabricator prints a Ripley chassis; a clamp on a mapped Ripley,
climbing in, 3 tiles east for 320 J, the crate clamped into the cargo, ejecting), `he_pipes` (an
air canister wrenched to a port fills a heat exchanging loop on space tiles, which cools it).
Wall time on this machine (24 threads, debug
build): `--jobs 1` 51 s, `--jobs 4` 25 s; about 10 s of either is a server loading the
prototypes, which every worker does once.

### Parallelism and isolation

- `--jobs N` worker threads take cases from one queue. Each worker keeps one **pooled server**
  (in-process, the same plugins as `ss13_server`, a free loopback port, `skip_lobby`, map seed 1)
  for its whole run.
- A pooled server loads a 7x6 staging level (z 1, where connecting clients appear) and then
  **every pooled case's map on a level of its own**. A case runs at most once per server, so the
  level it runs on is fresh; its bots are placed there by `setup`, and after the case they
  disconnect (their bodies stay on the level nobody uses again). Cases on one server run one
  after the other; the tests of a case run in order on the case's level and share its state.
- Maps load only at startup (the game's load passes, power nets, atmos and doors initialise
  once), so a case's level is loaded with the server instead of when the case starts.
  Shortcut: levels are preloaded and abandoned, not loaded and dropped per case; load them per
  case once `ss13_mapload` and the game set up a template loaded after startup.
- Speech and radio are scoped by level (radio reaches its z only), so pooled cases cannot hear
  each other. Ghosts hear every radio (`CHAT_GHOSTRADIO`): a bot that dies hears other levels'
  radio, nothing else leaks. Area state (power, alarms) is per area **type**: every case map
  uses area types no other case map uses (the seed suite: one `/area/centcom/...` subtype each,
  which need no power).
- Servers tick at the real rate, but each tick is 50 ms of game time and every game RNG derives
  from the case's seed (`ss13_game::testing::deterministic`): a loaded machine slows a case
  down instead of making its ticks longer.
- `time_scale` is set for the case and reset after it (it is server-wide, but a server runs one
  case at a time).
- **Exclusive** cases (`exclusive`, a `seed`, or a station `map`) start a server of their own
  for just that case: round state (lobby, shuttle, round end) or global knobs. They go first in
  the queue.
- A case that panics (a server system included) fails with the panic message; that worker
  starts a new pooled server for its next case.

### Case file reference

One scenario per `.case` file, the map next to it. Line format like the visual check scripts
(one directive or step per line, `#` comments, `"quoted strings"` with `\"`): readable, diffable,
line numbers in every message, no extra parser dependency; YAML/TOML would need a nested
structure per step for the same content. Header (before the first `test`):

| Line | Meaning |
| --- | --- |
| `map room.dmm` / `map MetaStation` | the case's map: a `.dmm` relative to the case file, or a station of `--source` (exclusive) |
| `bot alice 3,4 [job "Security Officer"] [give /obj/item/x]... [antag brother] [admin "ADMIN BAN"]` | a bot on tile (3,4) of the map (x from the left, y from the bottom, as in the `.dmm`); `job`: a tg job title (its outfit, ID and access; default the assistant); `give`: items spawned into its hands, left first; `antag`: then an antagonist role (an admin's "make antag"; `brother`: a blood brother with a team of its own; `vampire`: a vampire; `changeling`: a changeling); `admin`: listed as an admin with these `admin_ranks.txt` keywords (`EVERYTHING`, `"ADMIN SPAWN"`, ...) before it connects |
| `exclusive` | own server |
| `seed 7` | map seed for spawners (makes the case exclusive) |
| `time_scale 10` | game speed during the case (0.1-10) |
| `timeout 5` | default `until` timeout in seconds (10) |
| any step | a setup step, run once after the bots are placed |
| `test name` | starts a test; the steps after it belong to it |

Case maps: real station types; no spawn landmarks (bots are placed by `bot`); area types unique
to the case; enclosed (walls, or space around), since the level is the case's whole world.

Steps:

| Step | Effect |
| --- | --- |
| `alice move E N` | one step per direction (`N S E W NE NW SE SW`), each until the server acks it (moved or blocked) |
| `alice click 4,3` | left click on the tile's center, like the client: the topmost thing alice sees there (plane, layer) whose icon or overlay is drawn at that pixel (atlas alpha; click-through atoms skipped), so a wall light on the tile's edge does not take the click |
| `alice click 4,3 /obj/machinery/door` | on the first thing there whose DM path starts with it (without a leading `/`: whose name contains it) |
| `alice click self` / `alice click bob` | on her own mob / bob's mob |
| `... shift ctrl alt right middle` | click modifiers after the target |
| `alice slot back` / `alice altslot suit_storage` | HUD slot click / alt-click: `left_hand right_hand id uniform shoes back head mask neck eyes ears gloves suit suit_storage belt left_pocket right_pocket` |
| `alice drag bob self` / `alice drag slot:back 2,3` / `alice drag 4,4 /obj/structure/closet 3,3` | click-drag (`ClientIntent::Drag`) from one end onto the other; an end is named like a click target, or `slot:<slot>` |
| `alice say "; текст"` | the say line (prefixes as for players) |
| `alice act Vending vend ref=0 premium=0` | tgui act on her newest open window of that interface; a value `@<subject>.<path>` is read when the step runs, e.g. `key=@alice.windows.Storage.data.items[-1].key`, `wire=@obj(5,3,/obj/machinery/door).wires.colors.Bolts` |
| `alice close Vending` | closes the window |
| `alice zone chest` | zone selector (tg zone names) |
| `alice swap` `drop` `combat` `throw` `resist` `stop_pull` `latejoin` | the matching intents |
| `alice teleport 3,3` | dev: moves her mob |
| `alice hurt oxy 150` | dev: damage to her mob (`brute`, `burn`, `tox`, `oxy`) |
| `alice admin kick bob "reason"` | an admin verb or adminhelp over the admin channel (`ss13_bots::admin::VERBS`: `ahelp`, `asay`, `pm bob <text>`, `ticket bob close\|resolve\|reopen`, `jump`/`get`/`follow bob`, `kick`, `mute`, `ban`, `aghost`, `spawn <path>`, `heal`, `smite`, `event <name>`, `endround`, `timescale`, `announce`, `antag bob traitor\|brother\|changeling`, `dynamic`, ...); bots are named by bot name, sent as ckeys |
| `spawn /obj/item/x 4,3` | dev: spawns an object |
| `event "Grid Check"` | dev: forces a random event by its tg name (`ss13_game::events::force_event`) |
| `wait 1.5` | real time |
| `until <condition> [timeout=S]` | re-checks every 50 ms until it holds |
| `expect <condition>` | holds now |
| `repeat 1 alice click bob until <condition> [timeout=S]` | the action every second until the condition holds |
| `save hp mob(bob).health.health` | remembers a value: conditions can compare to `$hp`, `$hp+5`, `$hp-5` |

Bot actions other than `move` wait two server ticks after sending; anything slower is an `until`.
Since the click queue, a mob applies one click per tick and respects `next_move` (0.8 s after an
attack), hence `repeat` for repeated hits.

Conditions: `<subject>.<path> <op> <value>`, or `<subject>.<path> exists|missing` (`null`
counts as missing). Ops: `==` `!=` (numbers within 1e-6), `<` `<=` `>` `>=`, `~` (a string
contains the text; an array has an element equal to the value or, for strings, containing it),
`!~`. Values: numbers, `true`/`false`/`null`, `"text"`, a tile `4,3` (`[4, 3]`), JSON arrays,
`$saved`; anything else is a string. Paths: `.key`, `[0]`, `[-1]` (last), `[]` (every element:
`entities[].path`), `.len` (length). Subjects:

- `alice` — her observation: `name`, `connected`, `pos` (`[x, y]`), `z`, `hands` (icon state
  names, `null` empty), `active`, `slots.<slot>`, `health` (the HUD readout), `combat`, `zone`,
  `immobile`, `hunger` (`state`, `bar`, `shown`), `alerts` (names), `throw_mode`, `pulling`,
  `windows.<Interface>` (`title`, `status`: `Interactive`/`Update`/`Disabled`, `data`, `static`
  as JSON for the interfaces `ss13_bots::bot::decode_ui` knows, HUD icons as icon state names),
  `chat` (lines since the test started), `chat_all`, `notices`, `sounds` (files of every sound
  sent to her, oldest first), `seen` (replicated entities on
  her level, or all of them while her view is elsewhere, e.g. a camera: `id`, `pos`, `icon`),
  `admin` (`rank`, `rights` (`null` for a player), `players`, `tickets`, `lists.<prototypes|events|dynamic>`,
  `replies` (`ok: ...` / `refused: ...`), `last`).
- `tile(x,y)` — server: `turf`, `area` (`name`, `powered`, `requires_power`, `has_apc`), `air`
  (`pressure`, `temperature`, `moles`, `gases.<tg id>` moles), `wet` (`TURF_WET_*` of the
  strongest wetness, 0 dry), `entities` (as below).
- `obj(x,y,/path)` — server: the first entity on the tile matching like a click target.
- `mob(alice)` — server: her mob.

An entity (server) has `path`, `name`, `net_id` (the asking bot's id, `null` when not
replicated to it), `pos`, `z`, and per system: `door` (`open`, `closed`, `bolted`, `welded`,
`panel_open`), `wires` (`colors.<wire>`, `cut`), `closet` (`open`, `locked`, `welded`),
`reagents` / `blood` / `stomach` (`total`, `max`, `temperature`, `list.<reagent path>`), `health`
(`health`, `max`, `brute`, `burn`, `tox`, `oxy`, `parts.<head|chest|larm|rarm|lleg|rleg>`
`.brute/.burn`), `stat`, `nutrition`, `fire_stacks`, `on_fire`, `burning`, `stock`
(`[{path, amount}]` by record index), `contents` (a storage's item paths), `held`, `slots`,
`back_contents`, `handcuffed` (the cuffs' path), `in_closet`, disposals: `disposal` of a bin
(`flush`, `full_pressure`, `pressure_charging`, `flushing`, `pressure` kPa, `contents`), `in_disposal`
(`"bin"` or `"pipe"`), `sort_tag` of a parcel, `dest_tag` of a tagger, `supermatter` (`energy` MeV,
`damage`, `integrity`, `countdown`), `emitter` (`active`, `powered`, `welded`), `accumulator`
(`stored`, `processed` J), `smes` (`charge` J, `input` J/tick, `inputting`), `powernet` of a power
machine (`avail`, `load` W, last tick), silicons: `ai` (an AI eye's core, as an entity), `cyborg`
(`model`, `charge`, `max_charge`, `low_power`, `connected_ai`), `laws` (`name`, `list`),
`station` (`open`, `occupied`), medicine: `blood_volume`, `blood_type`, `bleed_rate`,
`limbs.<zone>` (`kinds`, `wounds`, `gauze`, `surgery` state bits, `prepped`), `heart_damage`,
`buckled`, `defib` (`charge`, `paddles_out`). New fields go into `ss13_debug::test_hooks` (no protocol change:
commands and answers are JSON).

Example (`tests/cases/door.case`):

```text
map door.dmm
bot alice 4,3 give /obj/item/screwdriver give /obj/item/multitool
bot bob 3,3 job "Security Officer"

test assistant_is_denied
  alice move E
  wait 1
  expect alice.pos == 4,3
  expect obj(5,3,/obj/machinery/door).door.closed == true

test bolts_by_wire
  alice click 5,3 /obj/machinery/door
  until obj(5,3,/obj/machinery/door).door.panel_open == true
  alice swap
  alice click 5,3 /obj/machinery/door
  until alice.windows.Wires exists
  alice act Wires pulse wire=@obj(5,3,/obj/machinery/door).wires.colors.Bolts
  until obj(5,3,/obj/machinery/door).door.bolted == true
```

## Jobs and latejoin

`ss13-mapload convert` also writes `<assets>/jobs.ron`: every joinable tg job (`/datum/job` with
`JOB_NEW_PLAYER_JOINABLE`) with its department, positions, outfit (`/datum/outfit/job/*`, default
preferences: department backpack, jumpsuit) and ID access (trim `minimal | extra` and wildcard
lists, plus the access flag groups the captain's trim adds in `New()`, which `ss13-dm-extract`
dumps as `new_access_flags` / `access_flags`), and the `strings/names` lists. It prints how many
jobs resolve fully. Without the file the server knows only the assistant.

## Lobby and rounds

A connecting client is a tg `new_player`: no body, nothing replicated, tg's title screen
(`icons/runtime/default_title.dmi`) over the map with BandaStation's lobby buttons: the character
name (a random tg name, kept for the connection; the token stays the account), "Готов" before the
round starts / "Присоединиться" after, "Наблюдать" (asks first; a ghost at
`/obj/effect/landmark/observer_start`, no joining afterwards), "Настройка персонажа" (the jobs page
of tg's preferences menu: off/low/medium/high per job, one high at most, the assistant as an
on/off toggle, and what to do when no wanted job is free) and the round info box. Character setup
also has an appearance section (body type, skin tone, hair style and color, eye color, randomize;
tg's random tables as the choices, indexes checked on the server): the body the player joins with
wears it. It lasts for the connection (no preferences database); a new connection starts with a
random appearance. `scripts/visual/appearance.txt` (with `SS13_SKIP_LOBBY=0`) screenshots it. Ready needs at
least one job preference (BandaStation).

The server runs tg's ticker states: `PREGAME` counts the lobby down, `SETTING_UP` divides the jobs
among the ready players (tg `divide_occupations` with BandaStation's staffing priorities: overflow
toggles first, then high, medium, low, the most needed job first; the rest by their jobless role;
officer slots scaled to the population) and spawns them in one tick at their jobs'
`/obj/effect/landmark/start`, `PLAYING` lets lobby clients latejoin through the tgui
`JobSelection` window (slots checked on the server; latejoiners arrive at
`/obj/effect/landmark/latejoin`, there is no arrivals shuttle yet). A body wears its job's outfit
with an ID card "ID-карта «Name» (Assignment)"; every job gets the grey backpack with the survival
box. A job slot stays taken while the body exists.

The round ends when an admin ends it (the admin panel's "Закончить раунд", see "Admins", or in
debug builds `SS13_ROUND_SECONDS`): tg's round-end report goes to every chat, then "Перезагрузка мира
через ..." and, after `round_end_countdown`, the world reboots: the server process builds a fresh
server (map loaded again) on the same port with the same certificate, and clients reconnect on
their own (~1 s) into the new lobby, as BYOND clients do on `world.Reboot()`.

- `SS13_LOBBY_SECONDS` (default 120, tg `lobby_countdown`); `0` starts the round at once and
  clients still land in the lobby (tg `start_immediately`).
- `SS13_ROUND_END_SECONDS` (default 25, tg `round_end_countdown`).
- `SS13_SKIP_LOBBY=1` (dev): the round runs from the start and every client gets an Assistant
  body at once (at the assistant start, or `SS13_SPAWN_AT` / `SS13_DEV_SPAWNS`), as before the
  lobby. `scripts/visual-check.sh` sets it unless `SS13_SKIP_LOBBY=0` is given.
- `SS13_ROUND_SECONDS` (dev, debug builds only): the round ends that long after it starts.

```sh
cargo test -p ss13_game --all-features --test it jobs::        # latejoin menu, officer outfit/access/door,
                                                          # slots, observe; a round: lobby, ready,
                                                          # round start jobs, latejoin, end, reboot
cargo test -p ss13_game --all-features --lib round       # divide_occupations
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it jobs:: -- --ignored --nocapture
                                                          # MetaStation round as a Security Officer,
                                                          # open a security door
SS13_SKIP_LOBBY=0 SS13_LOBBY_SECONDS=15 SS13_ASSETS=$ASSETS scripts/visual-check.sh -o /tmp/vc \
    scripts/visual/lobby.txt                              # lobby, job preferences, ready, round start
SS13_SKIP_LOBBY=0 SS13_LOBBY_SECONDS=0 SS13_ASSETS=$ASSETS scripts/visual-check.sh -o /tmp/vc \
    scripts/visual/jobs.txt                               # latejoin window and the officer
```

### Emergency shuttle

The round ends the tg way when the emergency shuttle reaches CentCom (`crates/ss13_game/src/shuttle.rs`).
The map json's `"emergency"` shuttle (`_maps/map_jsons/*.json`; MetaStation: `emergency_meta`)
loads with the station onto its own z-level, which stands in for CentCom; `ss13-mapload convert`
also scans `_maps/shuttles` for its types and emits `dm_docking_port` (rerun it). The station's
`/obj/docking_port/stationary` with `shuttle_id = "emergency_home"` is where it docks.

- Communications console (`/obj/machinery/computer/communications`, tgui `CommunicationsConsole`:
  authentication and the emergency shuttle section): log in with an ID that has command access,
  call with a reason of at least 12 characters, recall while more than the base call time is
  left. Calls are refused for the first `SHUTTLE_REFUEL_DELAY` of the round.
- Modes: called -> docked after the call time (x2, green alert) -> engines start 10 s before the
  dock time ends -> escape (transit level) -> CentCom: round end report with "Коэффициент
  эвакуированных" / "На эвакуационном шаттле" and a personal "брошены на станции" for those left
  behind. Priority announcements in BandaStation's Russian go to every player's chat.
- Env (seconds): `SS13_SHUTTLE_CALL_SECONDS` (600), `SS13_SHUTTLE_DOCK_SECONDS` (180),
  `SS13_SHUTTLE_ESCAPE_SECONDS` (120), `SS13_SHUTTLE_REFUEL_SECONDS` (1200),
  `SS13_SHUTTLE_AUTOCALL_THRESHOLD` (off; tg's crew-loss auto call, `1` calls at once).

```sh
cargo test -p ss13_game --all-features --lib shuttle      # mode timers, recall, ordered turfs
cargo test -p ss13_game --all-features --test it shuttle::     # fixture: console login/call/recall,
                                                          # dock, board, launch, arrive, report
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it shuttle:: -- --ignored --nocapture
                                                          # MetaStation dock: move time, bytes
SS13_SHUTTLE_AUTOCALL_THRESHOLD=1 SS13_SHUTTLE_CALL_SECONDS=5 SS13_SHUTTLE_DOCK_SECONDS=600 \
    SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/shuttle_docked.txt  # docked shuttle
```

### Random events

`crates/ss13_game/src/events/`: tg's `SSevents`. While the round runs, every 2 s the scheduler
checks its next pick time, picks an event by weight among those whose gates pass (round time
over `earliest_start`, enough living players, under `max_occurrences`, shuttle not escaped),
schedules the next pick `rand(2.5, 7)` minutes later (BandaStation's config) and runs the pick
after tg's 20 s admin window, if it still may. Ported events, with tg's weights and BandaStation's
Russian announcements (chat only, no sounds): Grid Check (APCs fail for 1-4 minutes and come back
by themselves), Electrical Storm (light tubes break around a random spot), Communications
Blackout (headsets dead for ~5 minutes; intercoms only reach each other), Radiation Storm (hide in
maintenance; outside it hurts), Brand Intelligence (vending machines shoot their stock), Market
Crash (announcements only: vending is free), False Alarm, the meteor waves, Major Space Dust,
Space Dust: Minor and the Stray Meteor (meteors fly in from the edges of the station level and
break walls and floors where they hit), Carp Migration (a school of space carp from the carp
landmarks, see "Basic mobs and their AI").

- `SS13_RANDOM_EVENTS=0`: no random events (forced ones still run). Bot servers and the integration
  test servers turn them off (`EventsConfig::forced_only()`); a test forces the event it checks.
- `SS13_FORCE_EVENT="Grid Check,Meteor Wave: Normal"` (debug builds): forced at round start, the
  stand-in for the admin Force Event; the bots' `event "<name>"` step and the debug test hook
  `{"op":"event","name":...}` do the same.

```sh
cargo test -p ss13_game --all-features --lib events     # scheduler: weights, gates, frequency,
                                                        # admin window, lifecycle (seeded RNG)
cargo test -p ss13_game --all-features --test it power::grid_check   # grid check, electrical storm
cargo test -p ss13_game --all-features --test it radio::communications_blackout     # communications blackout
cargo test -p ss13_game --all-features --test it construction::a_meteor  # a meteor breaks a wall
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter events
SS13_FORCE_EVENT="Meteor Wave: Catastrophic" SS13_LIGHTING=0 \
    SS13_MAP=$PWD/scripts/visual/maps/meteors.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/meteors.txt   # meteors hitting a room in space
```

### Basic mobs and their AI

`crates/ss13_game/src/mobs/`, after tg `/mob/living/basic` and `/datum/ai_controller`.
`ss13-mapload convert` writes `<assets>/mobs.ron` (every `/mob/living/basic` type: health,
`physiology`, melee, verbs with BandaStation's `ru_verbs.toml`, `habitable_atmos`, temperature
limits, icons, factions, `ai_controller`) and a `dm_mob` component on their prototypes: rerun it
after updating. Map placements (Ian, Runtime, Renault, McGriff, Tom, ...) and runtime spawns
become basic mobs; 10 mice appear on exposed station wires at round start.

- One damage pool (every type becomes brute, scaled by `physiology`), dead at 0 health: the dead
  sprite, not dense, "перестает двигаться..."; a mouse leaves a dead mouse item, a cockroach
  vanishes. Every 2 s: tile gas moles against `habitable_atmos`, body temperature towards the
  air, burns outside the survivable range (carp live in space, corgis die there).
- Empty hand: "Вы гладите Ian." (and one time in three the pet's emote), in combat mode
  `response_harm` for 5-10; items hit with their force. A basic mob bites with
  `melee_damage_lower..upper` every `melee_attack_cooldown`.
- AI: behavior trees per tg controller (`ai/trees.rs`: carp, dog, corgi, cat, mouse, cockroach,
  fox; other controllers only wander), planned every 0.25 s; moves at the mob's `speed`, around
  walls by a bounded A*. As tg: off while dead or player-controlled; always on in station and
  shuttle areas; elsewhere on with a living player within 10 tiles, on-low for
  `RUN_WHILE_UNWATCHED` (migrating carp), else off. Carp hunt every non-carp
  (pets too) and flee at half health; cats catch mice; mice flee people and bite cables (a
  powered one usually kills them).
- Random event "Carp Migration": a carp (5% a megacarp) on every carp landmark, swimming to the
  station and out again.

```sh
cargo test -p ss13_game --all-features --lib mobs        # planning, aborts, targeting, flee,
                                                         # A*, body temperature, atmos checks
cargo test -p ss13_mapload --lib mobs                    # mobs.ron fields, translations
cargo test -p ss13_game --all-features --test it mobs::       # carp bites a player and is punched
                                                         # back; a mouse dies into its corpse;
                                                         # pets wander and stay alive; station AI
                                                         # runs, wild AI sleeps; carp migration
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it mobs:: -- --ignored --nocapture
                                                         # MetaStation: ~55 mobs, AI µs per pass
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter mobs
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/mobs.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/mobs.txt      # Ian, a carp, Runtime, Renault, Tom
```

### Tools and construction

Items with a tg `tool_behaviour` act before they hit (tg `base_item_interaction`, out of combat
mode): a click with an item in hand on a structure or turf runs its tool act, then the target's
item interaction, then an attack. Right click is tg's secondary click. Timed steps are a
`do_after` (delay × `toolspeed`; moving, dropping the item, a welder going out or the target
changing cancels; the state is checked again at the end). The welder lights and goes out with a
click on its hand slot (fuel 20, one unit per 5 s while lit, 15 burn force lit).

- Floors: a crowbar pries the tile (it drops, the turf becomes plating); a floor tile on plating
  puts it back.
- Iron sheets, right click on a floor: 4 s, 2 sheets -> displaced girder; wrench 4 s secures it
  (or loosens it); screwdriver 4 s on a displaced girder -> 2 sheets; 2 sheets on a girder 4 s
  -> wall; a lit welder on a wall 10 s -> 2 sheets + girder, the floor under it back.
- Grilles: right click with wirecutters cuts one into rods, a screwdriver (un)anchors it, rods
  repair a broken one, 2 glass sheets on it place an unanchored full-tile window; right click
  with 2 rods on a floor builds one (stand-in for tg's crafting menu).
- Windows: screwdriver 3 s, crowbar 10 s / 5 s, screwdriver 3 s unanchors, wrench 3 s -> glass;
  a lit welder repairs. Reinforced walls, windows and girders refuse with a message.
- Hits: windows, grilles and girders take the item's force through `damage_deflection` and melee
  armor (a full-tile window: 16 toolbox hits); a destroyed window leaves shards and opens its tile
  to the air, a grille breaks at 40% (a rod drops, it lets things through) and falls to rods.
- Vending machines: screwdriver toggles the panel, wrench 6 s with the panel open unanchors
  them; unanchored structures and machines can be pulled.
- Stacks: a stack in the other hand gives one unit to the empty active hand on a click of its
  slot; a stack clicked with the same stack in hand merges into it.

```sh
cargo test -p ss13_game --all-features --test it construction::  # pry and replace a tile; girder ->
                                                            # wall -> sliced; smash a window and
                                                            # the room vents
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/construction.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/construction.txt  # girders, plating, a broken window
```

`ss13-mapload convert` emits the construction types (`CONSTRUCTION_TYPES`) and the components
the code reads (`dm_tool`, `dm_stack`, `dm_turf_build`, `dm_obj`): rerun it after updating.

### Wires and hacking

Airlocks, APCs and vending machines carry tg `/datum/wires` (`crates/ss13_game/src/wires.rs`):
the wire colors are shuffled once per round per wire set (tg `wire_color_directory`, seeded from
`SS13_MAP_SEED`; airlocks take their area's set: maintenance, command, ...). A screwdriver opens
an airlock's or APC's panel (the vending panel as before); wirecutters or a multitool on an open
panel, or an empty hand on it, open tgui's `Wires` window: Cut/Mend needs wirecutters in a hand,
Pulse a multitool. Airlock wires follow tg: main/backup power (cut: no power, pulse: 60 s
outage), bolts (pulse drops them, raises them with power; cut drops them), ID scan (pulse
flashes the deny lights, cut: no ID opens it by hand), open (pulse opens or closes it when the ID
scan is cut or no access is needed), safety, timing, feedback (lights), the shock wire (cut:
electrified while powered, pulse: 30 s) and the zap wires. An electrified holder shocks a bare
hand or a bump: burn from the area's APC cell and powernet, a 4 s stun and 6 s paralysis 2 s
later; insulated gloves (`siemens_coefficient` 0) stop it. APC wires switch channels, short
the APC and unlock it; vending wires toggle shooting products at people, contraband, the ID scan
and electrification. Signalers on wires and wire reveal (blueprints) are not ported; the AI wire stops the AI (see Silicons and cameras).

```sh
cargo test -p ss13_game --all-features --lib wires        # color shuffle per seed, cut/pulse effects
cargo test -p ss13_game --all-features --test it wires::       # panel, Wires window, bolts, hack, shock
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/wires.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/wires.txt     # the Wires window and a bolted airlock
```

`ss13-mapload convert` emits `siemens_coefficient` in `dm_clothing`: rerun it after updating.

### Security

Status effects follow tg (`crates/ss13_game/src/status_effects.rs`): stun, knockdown, paralysis,
immobilization (a new one never shortens what is left), stamina (120 max; 10 s after the last
stamina damage it resets to 0; at 100 the mob enters stamina crit: lying, cannot move or act,
until it drops below 100), confusion (a confused mob's steps turn), daze, stagger. Knocked-down
mobs lie and crawl; stunned, paralyzed and stamcrit mobs cannot move (the client stops predicting
steps), click or use their hands; handcuffed mobs cannot use their hands.

- Stun baton (`/obj/item/melee/baton/security/loaded`, high-capacity cell = 10 stuns): a click on
  the active hand turns it on; an active baton stuns on a left click (60 stamina, 10 s confusion,
  a 5 s knockdown 2 s later; 2.5 s cooldown, per target and user); right click hits for its force
  and stuns too; off, it hits in combat mode and pokes out of it. Police baton: 55 stamina and a
  1.5 s knockdown at once; telescopic baton: extends on a hand click, dazes.
- Handcuffs (also cable cuffs, zipties): a click on a human starts 4 s of cuffing (the victim
  stepping away interrupts); a cuffed mob drops what it holds, wears the cuffs overlay, can be
  dragged (its own steps are refused) and breaks out with the resist key (B) after the cuffs'
  `breakouttime` (handcuffs 60 s, zipties 45 s, cable 30 s); someone else takes them off in
  4 s through the strip menu (drag the cuffed mob onto yourself, its handcuffs entry).
- Flash: a click on a mob facing it (or from the side, halved) gives confusion, 80-120 stamina
  and a 2.5-5 s knockdown, unless its worn clothing's `flash_protect` adds up to 1 (sunglasses,
  welding gear); a click on its hand slot flashes everyone within 3 tiles (confusion only); it may
  burn out with use.
- Right click with an empty hand on a mob (any mode) shoves it one tile away; against a wall it
  falls (2 s), into another human both fall; a knocked-down dazed mob is kicked (3 s paralysis).

```sh
cargo test -p ss13_game --all-features --test it security::   # baton -> stamcrit, cuffs, drag, strip,
                                                         # resist out; flash vs sunglasses; shove
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/security.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/security_officer.txt scripts/visual/security_victim.txt
                                                         # a cuffed victim lying in stamcrit
```

`ss13-mapload convert` emits `dm_baton`, `dm_handcuffs` and `flash_protect` in `dm_clothing`:
rerun it after updating.

### Guns and armor

Guns (`ss13_game::guns`, `ss13_game::projectile`, `ss13_game::armor`), from tg's gun, casing,
magazine, projectile and pin vars (converted per type: `ss13_mapload::guns`). A held gun fires at
anything clicked past reach, and point blank at an adjacent mob; the pin must authorize the user
(standard pin: anyone; mindshield pin: the mindshielded jobs; syndicate pin: operatives). Each
shot sets the click cooldown to 0.4 s. Ballistic guns: click a magazine onto the held gun to load
(a loaded gun swaps them), Z ejects an empty magazine / drops a locked bolt / racks, a click on
the gun in the other hand with an empty hand takes the magazine out; casings drop at the
shooter's feet; revolvers keep spent casings until emptied with Z and load from speedloaders and
ammo boxes. Energy guns: Z switches the fire mode, a recharger charges them. A shot aims at the clicked pixel
(the click carries the cursor's offset from the clicked tile's center, `ClickModifiers::pixel`;
the server clamps it to one tile) and spreads by the gun's `spread`, a casing's `variance`, and
the dual-wield bonus (in combat mode, light guns in the other hand fire too, 0.1 s later).
`recoil` (at least `min_recoil`) kicks the shooter's view back (`CameraEffect::Recoil`).
`WEAPON_HEAVY` guns (shotguns) need the other hand empty. Shotgun shells fire `pellets`
projectiles; pump shotguns keep the spent shell until racked with Z, automatic ones rack
themselves; loose shells load one at a time. Bullets bounce off walls and windows by their
`ricochet_*` vars, and hits roll wounds with the projectile's `wound_bonus`, `sharpness` and
per-tile falloff. Taser electrodes hook the victim to the gun: 60 stamina a second from the
cell while the firer holds it and the victim is within reach and sight; the firer clicking the
victim or the victim's resist (5 s) ends it. No gun sounds (the project has no sound). Projectiles fly
`speed` tiles per decisecond (sprite turned and offset per tick), hit what was clicked, standing
mobs on the way (lying ones only from a firer in combat mode), dense things they cannot pass, or
walls. Worn clothing armor reduces melee and projectile damage per covered bodypart (at most 90%).
The traitor uplink sells the Makarov case, the .357 revolver and their ammo; the nuclear uplink
the C-20r case and magazines.

```sh
cargo test -p ss13_game --all-features --test it guns::   # disabler to stamcrit, laser through a
                                                     # vest, pistol empty + reload, recharger,
                                                     # a shot stopped by a wall, pixel aim,
                                                     # shotgun pellets + pump + recoil kick,
                                                     # a ricochet, a bullet wound, a taser
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter guns
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/guns.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/guns.txt       # a laser beam in flight
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/shotgun.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/shotgun.txt    # a riot shotgun blast, view kicked back
```

### Sound

The server runs tg `playsound` (`ss13_game::sound`): each player mob in range gets the sound at its
own volume (distance falloff, the thinner air of the two tiles, deaf mobs skipped) and offset; the
client (`ss13_game::sound_client`) plays it with `bevy_audio`, panned left/right. `ss13-mapload
convert` writes the SFX keys (`<assets>/sfx.ron`) and copies the referenced `.ogg` files to
`<assets>/sound/` with their attribution/license files and `SOURCES.txt` (source path, commit).
`SS13_VOLUME=0..100` sets the client's master volume. Sounds with `ignore_walls = FALSE` reach only
mobs on tiles BYOND `view()` sees from the source (and ghosts). Running humans play footsteps every
second step from the turf's `footstep`/`barefootstep` key into tg's footstep tables, which
`ss13-dm-extract` dumps as `globals` and the converter writes into `sfx.ron`; walking is silent.
A crowbar opens or closes an unpowered airlock at once (`airlockforced`); blast doors and shutters
play their `animation_sound`; energy guns drop in pitch as the cell drains (played as speed).

```sh
cargo test -p ss13_game --all-features --test it sound::   # a shot: near yes; far, vacuum, earmuffs no;
                                                           # footsteps; a wall stops ignore_walls = FALSE
```

### Radio

Outfits put the job's headset (`/datum/outfit/job` `ears`) into the ears slot; its encryption
keys (`keyslot`, `keyslot2`) give its channels. `;` talks on the headset's frequency (Common),
`:h` on its first channel, a department key on that channel if the headset has it (a headset
without channels sends every key on Common); without anything in the ears slot a department key
is plain speech. Every radio that is on, listens and has the frequency hears it on the same
z-level; mobs within its `canhear_range` read `[Инженерия] Имя говорит, "..."` in the channel's
tg color (a headset: only its wearer; an intercom: 2 tiles), ghosts read every channel. A
whisper reaches the radio starred and in italics. Intercoms and station bounced radios follow
their map vars (`frequency`, `broadcasting`, `listening`, `freerange`) and their type's
`Initialize()`: a broadcasting one relays speech within its range, `:i` talks into every
intercom within 1 tile, an intercom without equipment power in its area does neither. Telecomms
machines are not modelled: every transmission is processed as if the station's telecomms worked.

```sh
cargo test -p ss13_game --all-features --test it radio::       # :e between engineers, ; Common, intercom
                                                          # relay and listening, unpowered intercom
SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/radio_engineer.txt \
    scripts/visual/radio_assistant.txt                    # colored radio lines in both chats
```

### Medicine

The zone selector at the bottom right (tg `zone_sel`) picks where attacks and treatments aim
(`zone_selected`, default the chest): a click on its body part, or tg's numpad keys (1 right
leg, 2 groin, 3 left leg, 4 right arm, 5 chest, 6 left arm, 7 eyes, 8 head/eyes/mouth in turn,
9 mouth). Brute and burn damage sit on six bodyparts (head and chest take up to 200, limbs up
to 50; the health doll shows each); melee hits land on the selected zone 80% of the time (90%
on a lying target), else on a random part. Medical stacks (`/obj/item/stack/medical`: bruise
packs, sutures, ointment, meshes, ...) clicked on a mob heal the selected zone's bodypart by
tg's `heal_brute`/`heal_burn` after `self_delay` (on yourself) or `other_delay`; a step,
dropping the stack or the patient moving cancels; left click moves on to the next hurt limb,
sutures and meshes repeat while there is more. A health analyzer clicked on a mob prints tg's
health scan (status, damage by type and by bodypart with its wounds, temperatures, blood level
and type); a click on its hand slot switches it to wound scans; the wound analyzer prints the
simple wound scan. Out of combat mode an empty hand on a lying mob in crit gives CPR (3 s, up to
7 oxyloss, repeated every 0.8 s while it stays in crit). Medkits on the map hold their tg
contents.

Advanced medicine (`ss13_game::medical::{blood, wounds, surgery, defib}`), as current tg:

- **Blood.** 560 units of a random tg blood type; regrows 0.25 × round(nutrition / 450, 0.2)
  per second when low (iron helps), drains through bleeding wounds and open vessels; low blood
  costs breath and stamina, faints and below 112 kills, missing blood becomes oxygen damage. A
  syringe right click draws a sample (someone else: 3 s); blood packs start full; injected blood
  joins the volume (an incompatible type adds toxin). An IV drip takes a pack clicked on it and
  is dragged onto a mob to attach (1 s); it injects or draws 5 u/s and tears out of an arm when
  the mob walks away; an empty-hand right click detaches, ejects or switches the mode.
- **Wounds.** Brute and burn hits of 5+ roll tg's `check_wounding`: cuts (bleed, clot, demote),
  punctures (internal bleeding when hit again), dislocations, hairline and compound fractures
  (limp, the compound one disables the limb until splinted), second to fourth degree burns
  (infection without sanitization). Gauze soaks bleeding and splints, sutures and cauteries stop
  bleeding, ointment and mesh dress burns, a bonesetter resets a dislocation, bone gel then
  surgical tape knit a fracture.
- **Surgery.** Drapes prepare the selected bodypart; out of combat mode a tool on a prepared,
  lying mob offers the operations it can do there (several: the `ListInputWindow`): incision,
  retraction, clamping, sawing, drilling, closing, fixing bones, tend wounds (brute/burn),
  dislocation/hairline/compound repairs, organ manipulation and amputation (both stubs). Times
  and tools follow tg; an operating table (1×), table (1.25×), bed (1.5×) or the floor (2×)
  scales them; failures follow BandaStation (+40% on a conscious patient, up to +60% in the
  dark, +5% for basic tools). Drag a mob (or yourself) onto an operating table or a bed to lie
  on it; an empty hand on the table or resisting gets up.
- **Defibrillators.** A unit worn in its slot clicked with an empty hand gives its paddles;
  wielded (the other hand empty) on a corpse's chest: 3 s + 2 s, then revival into crit if
  brute and burn are under 180 and the heart has not decayed (~6 minutes after death). Combat
  defibrillators burn and paralyze in combat mode.

```sh
cargo test -p ss13_game --all-features --test it medical::    # zone selection, a bruise pack on the
                                                          # chest (cancelled by a step, then 40
                                                          # brute after 2 s), health scan, CPR
cargo test -p ss13_game --all-features --test it medicine::    # a self-inflicted cut bleeds, a blood
                                                          # sample, an IV of O-, the operation
                                                          # list and tend wounds, a defib revival
SS13_SKIP_LOBBY=1 SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/surgery.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/surgery_surgeon.txt scripts/visual/surgery_patient.txt
                                                          # a head incision on an operating table
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/medical.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/medical_medic.txt scripts/visual/medical_patient.txt
                                                          # zone selector, a scan in chat
```

### Chemistry

`ss13-mapload convert` also writes `<assets>/reagents.ron`: every tg `/datum/reagent` (name,
color, `metabolization_rate`, `overdose_threshold`, pH, purity, flags, ...) and
`/datum/chemical_reaction` (reagents, results, catalysts, container, temperatures, pH range,
rates; randomized recipes and those with their own `pre_reaction_other_checks` are left out),
and the `dm_reagents` component of reagent containers. The chemistry is `crates/ss13_reagents`
(tg `/datum/reagents` and `/datum/equilibrium`: instant reactions at once, the others stepped
every 0.25 s at the rate their temperature and pH give; explosions and other effects of the
reactions' own procs are not modelled).

Every container holds its `list_reagents`; its fill overlay follows the contents. With an open
cup in hand a click on a container pours `amount_per_transfer_from_this` into it (or fills from a
drainable one; right click fills), on your mob drinks a gulp, on someone else feeds them (3 s);
right click in combat mode splashes the contents out. A click on the held container's hand slot
cycles the transfer amount. A syringe injects (someone else: 3 s, blocked by thick clothing on the
selected zone), right click draws from a container. Pills are swallowed or dissolved in a cup.
Examine shows the contents. Mobs metabolize every 2 s (stomach to blood, tg rates); libital,
aiuri, multiver, epinephrine, salbutamol, convermol, toxins, ethanol (drunkenness), water,
nutriment and every consumable's nutrition have their tg effects, other reagents only metabolize. No dispenser, chem master,
heater, sprays or blood yet.

```sh
cargo test -p ss13_reagents                               # holder math, reactions, metabolism
SS13_ASSETS=$ASSETS cargo test -p ss13_reagents --test real -- --ignored --nocapture
                                                          # ammonia, libital, screwdriver with
                                                          # the generated data
cargo test -p ss13_game --all-features --test it chem::        # pour, ammonia + mix message, drink,
                                                          # syringe of libital heals 3 brute/tick
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/chem.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/chem.txt       # fill levels, a reaction, examine
```

### Genetics

`crates/ss13_game/src/genetics` (tg `/datum/dna`, `/datum/mutation`, DNA console and scanner,
DNA injectors). Every human body has DNA: unique enzymes (a crew member's equal its record's
DNA hash), unique identity blocks of its look, and 8 mutation blocks with chipped gene sequences
drawn from the unlocked mutations. Ported mutations (tg values): Glowy (a light on the mob),
Pressure Adaptation (no pressure damage or alerts), Cough (drops held items), Epilepsy (20 s
paralysis), and Cold/Heat Adaptation, Insulated, Deafness, X-ray (numbers only). Mutator-added
mutations cost stability; at 0 a genetic breakdown ends in a meltdown after 60 s. Mutadone in the
blood removes all mutations. The radiation storm scrambles an identity block and half the time
adds a random mutation (90% negative).

DNA scanner: click with an empty hand to open/close (closing shuts in the human on its tile),
drag a human onto an open scanner, a step from inside opens it. DNA console (empty hand, next to
a scanner): `DnaConsole` window, subject status, `pulse_gene` on each base of a block (a
complete block activates its mutation and discovers it), activator/mutator printing (tg
cooldowns) and nullify. An injector clicked on yourself injects at once, on someone else after
3 s.

```sh
cargo test -p ss13_game --all-features --lib genetics     # blocks, sequences, instability
cargo test -p ss13_game --all-features --test it genetics::    # scan, sequence Glowy, mutator,
                                                          # mutadone; radiation storm
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter genetics
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/genetics.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/genetics_subject.txt scripts/visual/genetics.txt
```

### Hunger, food and the kitchen

Humans start with tg's 250-400 nutrition and lose `HUNGER_FACTOR` (0.04/s at tg's neutral
mood) every `SSmobs` fire; every consumable reagent in the blood feeds them as it metabolizes
(`nutriment_factor`). Over 550 they overeat and get fat after 200 s (slower walking); below 200
metabolism is sluggish. The hunger bar (burger and bar right of the alerts, tg `ui_hunger`)
shows while hungry, full or digesting. `ss13-mapload convert` emits `dm_food` for every
`/obj/item/food`: `food_reagents` (grown food: its seed at potency 50), bite size, eat time,
tastes, trash, the microwave result and the knife/rolling pin processing read from the
`AddElement` lines `ss13-dm-extract` dumps, and the food processor recipe.

Out of combat mode a held food clicked on yourself is eaten bite by bite (tg's fullness
messages, a taste message, the trash in your hand at the end); on someone else it is fed after
3 s. A knife on food on a table (bread, meat slabs, ...) cuts it into its pieces, which split its
reagents. Microwave: click it with an item to put it in, with an empty hand to cook (10 s, power),
right click ejects; an egg becomes a boiled egg, other food a burned mess; cooking dirties it.
Food processor: food with a recipe in, empty hand runs it. Grinder: a beaker in hand replaces its
beaker, food goes in, empty hand grinds it into the beaker, right click takes the beaker out.
There is no oven, griddle, crafting menu, grill or juicing yet.

```sh
cargo test -p ss13_mapload --lib food                 # microwave/processable/processor data
cargo test -p ss13_game --all-features --lib hunger   # decay, thresholds, overeating
cargo test -p ss13_game --all-features --test it food::    # nutrition drops, microwave egg, eat it
                                                      # (taste, trash, nutrition up), slice bread
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it food:: -- --ignored --nocapture
                                                      # MetaStation: an egg from the kitchen
                                                      # fridge cooked in its microwave, eaten
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/kitchen.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/kitchen.txt  # cooked egg, hunger bar, cut meat
```

### Janitorial: blood, cleaning, wet floors

A bleeding mob on a turf leaves blood (tg `bleed` -> `add_splatter_floor`): each bleed rolls
`sqrt(amount) × 90`%; up to 10 units is a drip (five drips on a tile, the sixth turns into a
pool), more grows the tile's pool. Blood dries after 5 minutes. Map-placed and spawned cleanables
(`/obj/effect/decal/cleanable`) wash off with their `clean_type`.

Mop bucket and janitorial cart: 100 u, filled by pouring a container into them. A right click on
one with a mop wets the mop (15 u). A click with the mop cleans the clicked thing after 1.5 s: a
turf loses its blood and dirt (when the mop holds 1 u of water or space cleaner) and gets wet
(`MakeSlippery(water, 10 s, +60 s)`, 5 minutes at most), the mop loses 1 u. Soap scrubs after its
`cleanspeed` (3.5 s; NT 2.8 s, syndicate 0.5 s) and wears out. A spray bottle clicked on an
adjacent thing sprays its transfer amount onto that tile: space cleaner washes the tile and
everything on it, water wets it. Wet floors dry every second by their air's temperature (10 s
of water last 15 s at 20 °C). A standing mob running onto a wet tile slips: 0.5 s paralyzed,
6 s knocked down; walking (`ClientIntent::ToggleMoveIntent`, 0.4 s per tile) is safe. The
client toggles it with tg's HUD button (`mov_intent` at `ui_movi`, `running`/`walking`, drawn
from the server's `InteractHudData::walking`); BandaStation leaves the hotkey unbound.

```sh
cargo test -p ss13_game --all-features --lib janitor     # wetness durations, drying, slip rules
cargo test -p ss13_game --all-features --test it janitor::    # bleed drips, bucket -> mop, mopping
                                                         # (clean, wet), walk safe / run slips,
                                                         # space cleaner spray, soap;
                                                         # HUD button click -> walking
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/janitor.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/janitor.txt   # blood on the floor, a mopped wet tile
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/janitor.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/move_intent.txt   # RUN, a click, WALK
```

### Hydroponics

`ss13-mapload convert` also writes `<assets>/plants.ron` (`ss13_mapload::plants`): every tg seed
type as its `Initialize` leaves it (stats, product, growing icon states, genes, `reagents_add`,
`mutatelist`), every `/datum/plant_gene/trait`, the tray types and the grown items' seeds; the
converter also emits the products, mutations and weeds reachable from the seeds on the maps.
Trays (`crates/ss13_game/src/botany/`) follow tg's `process()`: every 2 s a tray whose 20 s cycle
passed ages its plant, applies its nutrients' `on_hydroponics_apply` (E-Z-Nutrient, Left 4 Zed,
Robust Harvest, ammonia, mutagen, weed/pest killer, water, ash, ... ~40 reagents), drinks water,
checks the light (the tile's static lumcount from the light sources), toxins, pests, weeds,
pollinates its neighbours, mutates by instability (stats, traits, species from `mutatelist`),
dies at 0 health and becomes harvestable; the plant sprite and tg's status lights are overlays.
A seed pack on a tray plants it; a container pours its transfer amount (water fills the tray,
the rest feeds it), food is composted, a cultivator pulls weeds, a shovel digs the plant out
(5 s); an empty hand harvests (`getYield()` products with the seed's potency-scaled reagents),
removes a dead plant or examines. A plant analyzer prints tg's analyzer fields to chat; produce
in a seed extractor gives 2-5 seed packs with its stats. `SS13_HYDRO_SPEED=N` (dev) runs every
hydroponics timer N times faster. No autogrow, plant bags, grafting, seed storage UI, sprays,
biogenerator or product traits (squash, glow, ...).

```sh
cargo test -p ss13_game --all-features --lib botany      # cycle math, mutations, seed bounds
cargo test -p ss13_mapload --lib plants                  # seed/trait/tray data, closure
cargo test -p ss13_game --all-features --test it botany::     # plant, water, feed, grow, harvest,
                                                         # analyzer, seed extractor
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it botany:: -- --ignored --nocapture
                                                         # MetaStation trays, light, a tomato
SS13_HYDRO_SPEED=10 SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/botany.dmm \
    SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/botany.txt   # growth stages
```

### Breathing

Every 2 s (tg `SSmobs`) a human breathes on every 4th fire, or every fire after a failed breath,
from the open internals tank or 1.99/2500 of its tile's air (tg lungs: O2 below 16 kPa
suffocates, CO2 over 10 kPa, plasma over 0.05 kPa, N2O alert from 1 kPa), and the breath goes
back to the tile; pressure below 20 kPa or above 550 kPa hurts unless `STOPSPRESSUREDAMAGE`
clothing covers chest and head; the body temperature follows the air through clothing
insulation (space suits regulate it). tg's screen alerts show in a column at the top right.
A level without simulated air (fixtures without `initial_gas_mix`) breathes as station air.

```sh
cargo test -p ss13_game --all-features --test it breath::   # vacuum: oxyloss, pressure, alerts; mask + tank
SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/breath.txt   # officer in space: slots, alerts
```

### Atmos

Tile air follows tg LINDA (`crates/ss13_atmos`): open turfs get their `initial_gas_mix`, walls,
closed doors and windows (`can_atmos_pass`, border edges) block air, and only turfs whose air
differs from a neighbour's are processed, every 0.5 s. The server logs per z-level how many
turfs differed at load (`atmos z1: ... turfs with differing air at load`); MetaStation has none.
Visible gases (plasma, N2O, water vapor, ...) are drawn as tg's gas overlays. A gas analyzer in
the active hand prints tg's readout to chat when its hand slot is clicked (own tile) or when a
tile is clicked with it.

```sh
cargo test -p ss13_atmos                                   # share/compare/temperature math, LINDA lifecycle
cargo test -p ss13_mapload --test it atmos:: -- --nocapture     # fixture: airlock between rooms / to space
SS13_ASSETS=$ASSETS cargo test -p ss13_mapload --test it atmos:: -- --ignored --nocapture
                                                           # MetaStation: active turfs at load, idle
                                                           # fires, a window breach with fire times
cargo test -p ss13_atmos --test bench -- --ignored --nocapture
                                                           # 100x100 room breach, 1 and 8 levels
```

### Atmos machinery

Pipes and atmos machines follow tg (`crates/ss13_atmos/src/pipenet/`,
`crates/ss13_game/src/atmos_machinery/`). At startup every map pipe, manifold, layer adapter and
machine becomes a device of the pipe graph: nodes connect by direction, piping layer and
`pipe_color` (omni grey connects to every color), connected pipes form one pipeline with one
gas mixture of their summed volume (smart pipes 35 L per connection, at least 70), machines keep
a mixture per port that their pipeline reconciles by volume after they moved gas. Removing or
adding a device relinks and rebuilds only the touched pipelines, gas kept. Pipes start empty, as
in tg: on MetaStation "Air to Distro" fills the distro from the air chamber in the first
minute. Smart pipes take tg's runtime bitmask icon by connection; machines their state sprite
(`vent_out`, `pump_on-1`, ...), with pipe underlays on plating and on the floor plane under tiles.

Every SSair fire (0.5 s): pipenets, then machines (vents, scrubbers with widenet, pumps, volume
pumps, passive gates, filters, mixers, thermomachines, connectors, injectors, passive vents,
canisters and portable pumps and scrubbers, air alarms), then turfs. Clicks: an empty hand opens
the machine's window (`AtmosPump`, `AtmosFilter`, `AtmosMixer`, `ThermoMachine`, `Canister`,
`PortablePump`, `PortableScrubber`, `AirAlarm`) or turns a valve (1 s); ctrl+click toggles a
pump; alt+click maxes a pump or takes a portable's tank out; a wrench (dis)connects a portable
to the connector port under it; a gas tank goes into a portable; a welder welds a vent. Air
alarms rate their tile's air (tg thresholds), warn aloud every 10 s, start locked (an ID with
`atmos_equip`, or the access their mapping helper gives, toggles the lock; right click too) and
apply their mode to the vents and scrubbers of their area.

```sh
cargo test -p ss13_atmos pipenet                         # pipeline merge/split, reconcile, valves,
                                                         # connectors, layer adapters, tg pump math
cargo test -p ss13_game --all-features --lib atmos_machinery
                                                         # vents, scrubbers, pumps, filter, mixer,
                                                         # thermomachine, canister, alarm (tg numbers)
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it atmos_machinery:: -- --ignored --nocapture
                                                         # MetaStation: build time, µs per fire, a
                                                         # vacuumed room refilled, the SM loop
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter atmos
SS13_SKIP_LOBBY=1 SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/atmos.txt
                                                         # the atmos room, an air alarm window
```

### Fire and space wind

Every processed turf runs tg's gas reactions (`ss13_atmos::reactions`: plasma, hydrogen,
tritium and freon fires, steam condensation, N2O, BZ, freon and the other formations, in tg's
priority order); air that cannot react (station air) costs ~35 ns per processed turf and
nothing while asleep. Fires are tg hotspots: a lit welder (on the floor, in a hand) exposes its
tile every second (700 K), plasma, tritium or hydrogen with oxygen lights, the hotspot samples
and burns the air, grows, spreads by radiated heat and burns out without fuel or oxygen; it is
drawn with tg's `fire.dmi` stages and BandaStation's overlay, and lights its surroundings.
Mobs in a fire get fire stacks and burn (the burning overlay, the fire alert, the body heats
up and takes burn damage); B (resist) while burning stops, drops and rolls. Flammable
structures (wooden tables, ...) burn down. Gas tanks react their contents every second. A turf
that moves air pushes unanchored, unpulled things and mobs on it along the flow (tg
`high_pressure_movements`, `move_prob` from `pressure_resistance`).

```sh
cargo test -p ss13_atmos                                   # reactions (DM numbers), hotspot lifecycle
cargo test -p ss13_game --all-features --test it fire::         # welder lights plasma: heat, CO2, burns,
                                                           # a table burns down; stop drop and roll;
                                                           # a breach blows a mob and an item out
cargo test -p ss13_atmos --test bench react -- --ignored --nocapture   # reaction early-out cost
SS13_MAP=$PWD/scripts/visual/maps/fire.dmm SS13_ASSETS=$ASSETS scripts/visual-check.sh \
    scripts/visual/fire.txt                                # a plasma room catches fire, the player burns
```

### Cargo and the economy

`crates/ss13_game/src/cargo/`, after tg `SSeconomy`, `/datum/bank_account`, `SSshuttle.supply`,
`/datum/supply_pack`, `/datum/supply_order`, `/datum/export` and the supply consoles.
`ss13-mapload convert` writes `<assets>/cargo.ron` (every `/datum/supply_pack` with contents:
name, cost, contents, crate type, access, order flags, group; every `/datum/export` whose value
the base, stack or material procs give) and adds `paycheck`/`paycheck_department` to `jobs.ron`:
rerun it after updating.

- Economy: seven department budgets (4999 each, cargo 0, tg's `round(35000 / 7 - 1)`); every job
  with a `paycheck_department` gets a bank account at join (five free paychecks, the account on
  the job's ID card, also through a PDA holding it); every 5 minutes of a running round each
  budget gets 500 and each account one paycheck (at most 50) from its budget, with tg's bank card
  message. The round-end report has tg's economy summary (crew total, average, richest).
- Supply shuttle: the map json's `"cargo"` template (MetaStation: `cargo_box`) loads on its own
  level, which stands in for CentCom ("CentCom" dock); the station's `cargo_home` is where it
  lands. Sent from the console: 5.5 s ignition, 60 s transit. Leaving CentCom it buys the cart
  (paid from the cargo budget, private orders x1.1 from the buyer's account): one crate per order
  on a free floor tile, named `"<crate> - #<id>"`, locked with the pack's access when the crate
  is secure, the contents and the shipping manifest inside. Arriving at CentCom it sells
  everything unanchored aboard (crates 200, sheets, plasma, tools, ...; elasticity as tg) into the
  cargo budget; the report shows as the console's CentCom message. It does not leave the station
  with a living mob aboard.
- Consoles (`/obj/machinery/computer/cargo`, `.../request`; click with an empty hand or an ID):
  tgui `Cargo` with status, catalogue by group (pack contents behind the info button), cart,
  requests (approve/deny), "Buy Privately". Request consoles file requests with a reason; the
  server refuses sending, approving and cart edits from them. Goodies, packs with their own
  `fill()`/`generate()` (random and critter crates), contraband and emag packs are not offered.
- Env (seconds): `SS13_PAYDAY_SECONDS` (300), `SS13_SUPPLY_CALL_SECONDS` (60),
  `SS13_SUPPLY_IGNITION_SECONDS` (5.5).

```sh
cargo test -p ss13_mapload --lib cargo                  # packs and exports from the dump
cargo test -p ss13_game --all-features --lib cargo      # budgets, paydays, order cost, exports
cargo test -p ss13_game --all-features --test it cargo::     # payday; order, deliver, open with
                                                        # access, stowaway, sell (fixture)
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it cargo:: -- --ignored --nocapture
                                                        # MetaStation delivery to the cargo bay
SS13_PAYDAY_SECONDS=3 SS13_SUPPLY_CALL_SECONDS=3 SS13_SUPPLY_IGNITION_SECONDS=1 \
    SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/cargo_qm.txt \
    scripts/visual/cargo_bay.txt                        # console, order, the delivered crate
```

### Disposals and mail sorting

`crates/ss13_game/src/disposals/`, after tg `code/modules/recycling/disposal/` and
`sortingmachinery.dm`. `ss13-mapload convert` gives mail junctions and the mail sorting mapping
helpers their sort tags (`dm_mail_sort`): rerun it after updating.

- Bins: an item clicked onto a bin outside combat mode goes in, a thrown one lands inside 75% of
  the time, a living mob dragged onto it goes in after 2 s (dragging oneself climbs in). Every
  2 s a bin with area equipment power and its pump on draws air from its tile towards
  `SEND_PRESSURE` (5% of an atmosphere, ~1.5 minutes from empty; map bins start full); a full
  bin with contents flushes by itself every minute, an engaged handle flushes it once full. A
  click with an empty hand opens tgui `DisposalUnit` (handle, eject, pump). A step of a mob
  inside climbs out unless it is flushing. `bin/tagger` bins hold a destination tagger (right
  click takes it, right click with one mounts it). Delivery chutes flush whatever enters them
  from their front (walking mobs, thrown items, clicks).
- A flush takes 1.5 s, then the contents and the bin's air move one pipe per 0.05 s (at most
  1000 pipes): junctions split 50/50, mail sorting junctions divert parcels whose tag they
  hold, an outlet opens for 2 s and throws everything 2 tiles out of its front, a bin throws it
  around itself, a pipe end throws it 10 tiles on. Mobs ride along (their view follows) and
  "clong" when they try to move. MetaStation: every station bin ends in the Delivery Office;
  its chutes go to the departments and waste disposal, tagged parcels to their department.
- Mail: the package wrapper wraps an item (on the floor or in the other hand) into a parcel in
  the free hand; the destination tagger (use in hand: `DestinationTagger`, "TagMaster 2.4")
  sets a parcel's tag when used on it; a parcel used in hand unwraps after 1.5 s.

```sh
cargo test -p ss13_game --all-features --lib disposals  # pipe dirs, junction and sort routing
cargo test -p ss13_game --all-features --test it disposals:: # flush to an outlet, a tagged parcel's
                                                        # sort branch, a dragged mob (fixture)
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it disposals:: -- --ignored --nocapture
                                                        # MetaStation: build time, every bin,
                                                        # chutes, a cargo parcel, µs per step
SS13_SKIP_LOBBY=1 SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/disposals.dmm \
    SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/disposals.txt
                                                        # the bin window, the outlet throwing
```

MetaStation (debug build): 1724 pipes (32 mail junctions), 60 bins, 11 chutes, 14 outlets, every
machine on a trunk, linked in ~9 ms at startup; a holder step costs ~4-8 µs.

### Research and development

`crates/ss13_game/src/science/`, after tg `/datum/techweb`, `/datum/techweb_node`, `/datum/design`,
`SSresearch`, the R&D server, console, production machines and destructive analyzer.
`ss13-mapload convert` writes `<assets>/research.ron` (every `/datum/techweb_node` and
`/datum/design`, the silo materials and the stacks carrying them, the R&D lathes' build types,
department flags and board parts, `SSresearch`'s point tables, the consoles' lock) and converts
the printed and ejected types: rerun it after updating.

- Techweb: one station techweb; the starter nodes are researched at round start, hidden nodes
  stay hidden until a design they unlock is researched or the destructive analyzer reveals them.
  Every second it gains 1 point per second while an R&D server is powered (MetaStation: two in
  the server room). A node needs its prerequisites researched and its points; required
  experiments count as done (no experiment framework), discount experiments as not done.
  Researching pays the science budget 250.
- R&D console (`/obj/machinery/computer/rdconsole`; click with an empty hand): tgui `Techweb`
  with points, queue, tabs (researched, available, future), node cards with progress, unlocked
  designs and details. The base console starts locked (BandaStation) and unlocks with research
  access; MetaStation maps the unlocked one. Nodes can be queued (research director first) and
  are researched once affordable.
- Lathes (protolathe, circuit imprinter, departmental protolathes/techfabs/imprinters): a click
  with a stack out of combat mode inserts it into the ore silo of the lathe's level (or the
  lathe's own storage, 37.5 sheets per matter bin, without a silo); the station silo needs an ID
  with a bank account to print or eject. Click with an empty hand: tgui `Fabricator` with the
  researched designs the machine and its department allow, by category, x1/x5/x10/max, and the
  materials with sheet eject buttons. Tier 1 parts: cost and time x1; items come out one per
  `(construction_time * lathe_time_factor) ** 0.8` deciseconds (1.6 s for most), on the lathe.
- Destructive analyzer: a click with an item loads it; tgui `DestructiveAnalyzer` destroys it for
  `techweb_point_items` points (once per type) or to reveal a hidden node that needs the item.

```sh
cargo test -p ss13_mapload --lib research               # nodes, designs, materials, lathes
cargo test -p ss13_game --all-features --lib science    # unlock rules, points, queue, costs
cargo test -p ss13_game --all-features --test it science::   # lock, research, insert, print,
                                                        # silo ID check, analyzer (fixture)
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it science:: -- --ignored --nocapture
                                                        # MetaStation servers, consoles, lathes
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter research
SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/research.txt
                                                        # Fabricator and Techweb windows, a node
                                                        # researched (waits ~40 s for points)
```

### Exosuits

`crates/ss13_game/src/mecha/`, after tg `/obj/vehicle/sealed/mecha`, its equipment, the chassis
construction and `/obj/machinery/mecha_part_fabricator`. `ss13-mapload convert` writes
`<assets>/mecha.ron` (mechs, equipment, constructions, wreckages, cells, fabricators) and emits
the mech, part and wreckage types: rerun it after updating.

- Exosuit fabricator: a click with sheets inserts them (the level's ore silo, as the lathes; the
  maintenance one has its own storage); a click with an empty hand opens tgui
  `ExosuitFabricator`: the researched exosuit designs by category, `+` queues, `▶` (or a click on
  the design) builds now, the queue on the right (start/stop, clear, remove). A part takes its
  materials when it starts and comes out south of the machine after its `construction_time`.
- Building: click a chassis with its parts (any order), then follow the steps it prints in chat:
  wrench, screwdriver, 5 cable, wirecutters, the central and peripheral boards (screwdriver after
  each), scanning module, capacitor, servo, power cell (screwdriver after each), 5 iron
  (wrench, lit welder), rods for the Ripley's cockpit (lit welder). The step's back tool undoes
  it. A printed cell is empty (tg): a mech needs a charged one to move.
- Driving: drag yourself onto a mech with empty hands (Ripley 1 s, others 2 s). Your steps move
  it: the first step in a new direction turns it, every attempt costs `step_energy_drain` (80 J
  with a tier 1 servo), a Ripley steps every 0.29 s at station pressure. Doors open with your
  ID. Alt+click on the mech toggles strafing, a middle click the equipment safety. B ejects
  (Ripley 1 s). The HUD shows the charge and integrity alerts.
- Equipment: click the mech with equipment (right click: the right arm). Facing an adjacent
  target, a click uses the left arm (right click: the right arm): the hydraulic clamp loads an
  unanchored object into the cargo compartment (1.5 s, 15 slots), pushes mobs (squeezes in
  combat mode); a drill takes the other clicks and drills walls (6 s), objects and mobs.
  Without equipment the mech punches.
- Damage: items, punches and emitter beams hit the mech, through its armor and facing (front
  ×0.5, back ×1.5). At 0 it becomes a wreckage (the cell and some equipment inside: crowbar;
  a welder cuts salvage) and the pilot falls out.

```sh
cargo test -p ss13_mapload --lib mecha                   # mechs, equipment, constructions
cargo test -p ss13_game --all-features --lib mecha       # steps, movement energy, armor, alerts
cargo test -p ss13_game --all-features --test it mecha::      # print a chassis, build a Ripley, drive,
                                                         # clamp a crate, eject; hits, wreckage
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter mecha
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/mecha.dmm SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/mecha.txt     # a Ripley with its pilot, driven
```

### Silicons and cameras

`crates/ss13_game/src/silicon/`, after tg `/obj/machinery/camera` with `SScameras`, the AI
(`/mob/living/silicon/ai`, its eye), cyborgs (`/mob/living/silicon/robot`, `/obj/item/robot_model`,
the recharging station) and `/datum/ai_laws`. `ss13-mapload convert` emits `dm_camera`
(`c_tag`, `network`, `view_range`; map edits included) and `dm_camera_console`, writes
`<assets>/silicon.ron` (every lawset, the cyborg models with their modules) and converts the
silicon mobs and module items: rerun it after updating.

- Camera network: 8x8 chunks know what the cameras within `MAX_CAMERA_RANGE` see (`view()` with
  opacity); doors and walls changing opacity update them. The server logs `cameranet: N cameras,
  N chunks, ...` at startup (MetaStation, debug build: 306 cameras, 366 chunks, ~64 KiB, ~21 ms).
- Camera consoles (`/obj/machinery/computer/security` and telescreens; empty hand): tgui
  `CameraConsole220`, the cameras of the console's networks; picking one moves the player's view
  (the replication viewer) to the camera, with static (dark tiles) where it does not see; closing
  the window or walking away brings the view back.
- Under static nothing is sent: a client looking through a console's camera or the AI's eye
  gets only the entities on tiles that camera (the AI: the network) sees, plus its own eye;
  light blocks carry no lights of hidden tiles and show them opaque (`ss13_net::server::ViewMask`,
  fed by the camera chunks; a chunk change walks only its flipped tiles). MetaStation, AI eye
  flying a 360-tile square from the core (debug build): ~650 -> ~260 entities known, 151 -> 68 KB
  of replication messages.
- AI (job "AI"): the core spawns on the primary AI start (MetaStation: the AI chamber); the client
  moves the AI's eye anywhere on the level, the view follows it, tiles no camera sees are static.
  Clicks reach only what the cameras see: on airlocks shift opens/closes, ctrl drops/raises the
  bolts, alt electrifies/restores (refused while the AI wire is cut, the airlock maps
  `aiControlDisabled` or all power is cut); on APCs ctrl toggles the breaker, shift the lighting,
  ctrl+shift the environment (refused while `aidisabled`). Shift elsewhere examines; a click on
  the own core prints the laws. A plain click opens the machine's window (tg `attack_ai`,
  `ss13_game::silicon::interact`): an airlock's `AiAirlock` (power disruption, electrify, ID
  scan, emergency access, bolts, feedback, safeties, timing, open/close; every act refused
  without AI control), an APC's `Apc` (breaker, charge mode, channels, the silicon interface
  lock), air alarms (controls without the ID lock) and atmos machines, the solar control and
  the mech bay console. The AI's windows stay interactive while the cameras see the machine and
  close when they do not; a cyborg opens the same windows with an empty hand, adjacent or up to
  7 tiles away, interactive within 15 tiles. Humans open the APC window with an empty hand
  (locked without silicon access). `:b` talks in binary, the department keys and `:o` on the
  integrated radio.
- Cyborgs (job "Cyborg"): link to the AI with the fewest cyborgs and take its laws; the model list
  (`ListInputModal`) gives the model's sprite, radio channels and modules, kept in the back slot:
  click it to open the modules, Q puts the active one back. The cell loses 1 W while conscious;
  a recharging station (empty-hand click opens/closes it, closing takes in the cyborg on it)
  charges it at 1 kW while powered.
- `SS13_DEV_JOB=AI` (dev, debug builds, with the lobby skipped): every client joins as that job.

```sh
cargo test -p ss13_game --all-features --lib silicon     # cameranet visibility, laws, console
cargo test -p ss13_mapload --lib silicon                 # dm_camera fields, lawsets, models
cargo test -p ss13_game --all-features --test it silicon::    # AI eye, camera view, door clicks and
                                                         # the AI wire, AiAirlock and Apc windows,
                                                         # none out of view; nothing replicated under
                                                         # static; console viewer and its mask;
                                                         # cyborg link, model, station (fixture)
SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it silicon:: -- --ignored --nocapture
                                                         # MetaStation: AI replication cost
cargo test -p ss13_net --all-features masked_view        # the interest mask over WebTransport
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter silicon --filter cyborg
SS13_DEV_JOB=AI SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/ai.txt
SS13_DEV_JOB=AI SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/ai_apc.txt
                                                         # the AI opens its chamber's APC window
SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/camera_console.txt
```

### Traitors

The lobby has BandaStation's "Антагонисты" checkbox (`antag_roll`, on by default) and the
character setup window "Предатель", "Кровный брат", "Вампир" and "Генокрад" toggles (tg `be_special`, off by default);
both are kept for the connection. At round start (`crate::antag::dynamic`, BandaStation's `SSdynamic` with
`config/dynamic.toml`): a dry run of the job division gives the candidates and the population, a
tier is rolled by weight, it gives the number of roundstart rulesets, rulesets are picked by
weight among all of BandaStation's roundstart rulesets (only the traitor, blood brothers, the
vampire, the changeling and the blood cult are implemented; other picks spawn nobody), and each traitor ruleset
picks up to `ceil(population / 24)` opted-in candidates (blood brothers and changelings:
`ceil(population / 29)`, vampires: `ceil(population / 24)`), who cannot get protected or blacklisted jobs (security, captain, ... —
`JOB_ANTAG_PROTECTED`/`JOB_ANTAG_BLACKLISTED` from `jobs.ron`, plus AI and cyborg; a vampire not
the chaplain or the head of personnel either, the ruleset's `blacklisted_roles`).

A traitor gets tg's greeting, the info panel (`AntagInfoTraitor`: employer, objectives, uplink
code; reopened with the button at the top left) and the antag HUD icon over its own mob (only
its client draws it). Objectives: two of assassinate / maroon / steal (tg's high-value items from
`antag.ron`) with real targets, then escape (or martyr). The uplink hides in the PDA (tg's default
`uplink_loc`): set the messenger ringtone to the code (`123 Альфа`) to open the `Uplink` window
(20 TC); using the PDA in hand then opens the uplink until "Закрыть" locks it. Without a PDA it
hides in the headset: say `:z <code>` (three words; the code never goes out on the radio);
`ss13-mapload convert` writes the catalog to `<assets>/antag.ron` (a curated list of items that
exist here). The round-end report lists every traitor, their objectives (green/red) and purchases.

- `SS13_DEV_TRAITOR=1` (dev, debug builds, with the lobby skipped): every joining body becomes a
  traitor; `unlocked` also unlocks and opens its uplink; `brother` makes it a blood brother,
  `vampire` a vampire.

```sh
cargo test -p ss13_game --all-features --lib antag        # dynamic counts, restricted jobs,
                                                          # objective checks, purchases, report
cargo test -p ss13_game --all-features --test it traitor::     # lobby prefs -> forced traitor ruleset,
                                                          # greeting, :z code (headset, no PDA),
                                                          # buy, round-end report
SS13_DEV_TRAITOR=unlocked SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/traitor.txt
```

### Blood brothers

`Roundstart Blood Brothers` (`crate::antag::brother`, tg `/datum/antagonist/brother` and
`/datum/team/brother_team`): each selected player becomes a brother with a team of their own. The
team may convert two more (three at 10%); joining spends one, so a roundstart brother converts one
crew member (two at 10%). The team's objectives, forged when it starts and shared by every
member: convert a brother, two kill/maroon/steal objectives with real targets (one more for a team
of three, one less for a hijacker), then escape (or hijack, 10%). While conversions are left, a
brother carries a plain flash (a pocket, else the backpack) and may flash from behind; a
successful targeted flash (any flash) on a conscious crew member with a client converts them into
the team, unless they are the target of a team objective, a brother already or mindshielded (the
jobs whose tg outfit implants a mindshield); the flash burns out. A brother gets the greeting,
the objectives, the `AntagInfoBrother` panel (the antag info button reopens it) and the brother
HUD icon over itself and over the team mates it sees. The round-end report lists every team: its
members and its objectives (green/red).

```sh
cargo test -p ss13_game --all-features --lib antag::brother   # team, forging, conversion, report
cargo test -p ss13_game --all-features --test it brothers::        # lobby prefs -> forced brother
                                                              # ruleset, flash conversion, HUD,
                                                              # round-end report
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter brothers
SS13_DEV_TRAITOR=brother SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/brother.txt
```

### Vampires

`Roundstart Vampire` (`crate::antag::vampire`, BandaStation's `modular_bandastation/vampire`):
each selected player becomes a vampire. It gets the fork's greeting, the objectives (drink
150-400 blood; pick a specialization, 50%, else steal, or protect someone, 5%; assassinate;
make a lair; escape, or survive, 20%), the `AntagInfoVampire` panel (information and
specializations tabs; the antag info button reopens it), the vampire HUD icon, the usable blood
counter on the left edge and its spells as action buttons in tg's top-left action row
(`crate::actions`: background, icon and border, red while unavailable, the cooldown's seconds or
the glare's charges; a click is the button's `trigger`).

- Biting: in combat mode, the head selected and an empty hand, a click on a human starts a loop
  of 5 s `do_after`s (moving, the hand or the zone ends it); each drains 25 of the victim's blood
  and, while the victim lives and is a crew body, gives `min(20, drained) × 0.5` (10) blood,
  total and usable, at most 200 per victim; warnings at 392 and 224 units of the victim's blood.
- Powers by total blood: rejuvenate (stuns and stamina off; 20 s) and glare (radius 1: stamina,
  confusion and knockdown by where the victims stand relative to the vampire's facing; two
  charges recharging in 30 s) at 0, vision at 100, the specialization and the lair at 150
  (pointed at a coffin next to the vampire with free tiles around it: a 10 s ritual makes it the
  vampire's), regeneration at 200 (rejuvenate also heals over 14 s), advanced vision at 500.
  The panel's pick (once) grants the specialization's powers by blood: umbrae's cloak (alpha 40
  and faster in the dark, 200 in light), gargantua's blood rush (250: faster for 10 s, up from a
  knockdown), dantalion's subspace swap (250, pointed, range 7); full power at 8 victims and
  1000 blood. A spell costs its blood × `(1 + nullified / 100)` and fails dead, at 100
  nullification, in the chapel or without the blood.
- Weaknesses each `Life()`: starlight on an unshaded space tile (10 usable blood and burns; without
  blood the vampire burns to death), the chapel (burns while holding blood), holy water in the
  blood (nullification, lost blood, burning), fire stacks (5 usable blood).
- The round-end report lists every vampire with its specialization, blood and victims, and its
  objectives (green/red).

```sh
cargo test -p ss13_game --all-features --lib vampire          # blood accounting, thresholds,
                                                              # costs, cooldowns, glare charges
cargo test -p ss13_game --all-features --test it vampire::         # lobby prefs -> forced vampire
                                                              # ruleset, bite, glare, 150 blood,
                                                              # lair, umbrae and the cloak, report
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter vampire
SS13_DEV_TRAITOR=vampire SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/vampire.txt
```

### Changelings

`Roundstart Changeling` (`crate::antag::changeling`, tg `/datum/antagonist/changeling`; weight 3,
`min_pop` 15, `ceil(population / 29)`, the protected and blacklisted jobs barred): each selected
player becomes a changeling with a hivemind name (`Mr. Alpha`) and hive. It gets the greeting, the
objectives (absorb 6-8 genes (at most the other players), steal 60%, kill 70% or maroon with identity
theft, escape or escape with identity; absorb most / absorb a changeling when another one exists),
the `AntagInfoChangeling` panel (the antag info button reopens it), the antag HUD icon, the
chemical counter on the left edge (`icons/hud/screen_changeling.dmi` `power_display`; the storage
while hovered) and its powers as action buttons (`crate::actions`).

- Chemicals: 20 of 75, +1 per second on each `Life()` (2 s), a quarter of that while burning, up
  to half the storage while dead; a power costs its chemicals when used. `STABLE` powers need a
  conscious changeling; in stasis only the stasis button works.
- Absorb DNA: pull the victim (ctrl+click) and upgrade to an aggressive grab (ctrl+click again;
  tg needs a kill grab, grabs stop at aggressive here), then the button: three 15 s `do_after`s
  (moving or the victim moving interrupts), 40 brute at the third; the victim dies husked and
  bloodless, its profile (name and body) is stored, +10 chemicals, a true absorption (a player's
  body) gives a readapt. An absorbed changeling gives half its points and storage and falls.
- Transform (5): a `ListInputWindow` of the profiles; the name (speech, examine) and the body
  change, the ID and worn items stay.
- Reviving Stasis (15): dead-looking (`health::DeathComa`: no ghost on moving, alive for
  objectives) for 40 s, then the "Revive" button heals fully. Regenerate (10): blood and wounds.
- The cellular emporium (the first button, `CellularEmporium`, 10 genetic points): gene stim (2:
  stuns and stamina off, 4 units of changeling adrenaline), fleshmend (2: 10 s of healing), the arm
  blade (2: `/obj/item/melee/arm_blade` in the active hand, stuck there, the button takes it back)
  and chitinous armor (1: suit and helmet, chemicals regenerate 0.125/s slower). Readapt refunds
  every point, once per true absorption.
- The hivemind: `:g` (`:п`) is read by every changeling, the dead and ghosts as `<id>: message`;
  nobody hears it aloud.
- The round-end report lists every changeling with its extracted genes and objectives.
- `SS13_DEV_TRAITOR=changeling` (debug builds, lobby skipped): every joining body becomes one;
  admins: `antag <bot> changeling` / the player panel's "Changeling" button.

The changeling's items (arm blade, chitin) are spawned at runtime: `ss13-mapload convert` adds them
to the prototypes (`ss13_mapload::antag::CHANGELING_RUNTIME_TYPES`), so regenerate the assets.

```sh
cargo test -p ss13_game --all-features --lib changeling       # chemicals, purchases, readapt,
                                                              # use checks, absorb stages, profiles
cargo test -p ss13_game --all-features --test it changeling::      # lobby prefs -> forced ruleset,
                                                              # hivemind, absorb, transform, stasis,
                                                              # emporium, arm blade, readapt, report
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter changeling
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/arena.dmm SS13_DEV_TRAITOR=changeling SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/changeling.txt scripts/visual/changeling_victim.txt
```

### Nuclear operatives

`Roundstart Nukeops` (`crate::antag::nukeop`, tg `/datum/dynamic_ruleset/roundstart/nukies`):
`ceil(population / 18) + 1` players who opted in to "Ядерный оперативник" (all of them or the
ruleset fails) get no station job. The infiltrator (`_maps/shuttles/infiltrator/
infiltrator_basic.dmm`) loads with the station onto its own level, which stands in for tg's nukie
base (its dock is `syndicate_away`, "syndicate base"); the operatives spawn aboard in
`/datum/outfit/syndicate`: ID with syndicate access, the syndicate headset on its frequency, the
nuclear uplink in the backpack (25 TC + `CEILING(players / 5, 5)`, opens in hand, the
`UPLINK_NUKE_OPS` catalog) and a syndicate pinpointer. The first is the leader: a title, the code
paper and the war declaration item (inert). Names are "<alias> <team name>", the team name a
random last name. Everyone knows the code of the syndicate bomb.

- The infiltrator's console (`ShuttleConsole`, syndicate access) flies to the station's
  `syndicate_*` docks (MetaStation: `syndicate_nw`) and back: 5.5 s ignition, 10 s transit.
- The bomb (`NuclearBomb`): a real disk in (click with it, or eject/insert in the window), the
  code, a timer (90-3600 s) takes the safety off, ARM counts down, ARM again (disk in, code
  entered) disarms, C with the safety off puts it back on. 10 s after the timer the detonation:
  on a station level (not in space) every living mob on the station dies and the round ends; on
  the infiltrator's level it is tg's "fluke".
- The disk: out of bounds (tg `atom_in_bounds`) or destroyed, it reappears on a station floor.
  Pinpointers point at it (syndicate ones at the infiltrator while a bomb counts down).
- Round end: tg `get_result` (nuke win, no survivors, wrong station, crew win, disk lost or
  stolen, fluke) with BandaStation's texts, the operatives and the TC they spent.

```sh
cargo test -p ss13_game --all-features --lib nukeop           # keypad, timer, result, helpers
cargo test -p ss13_game --all-features --test it nukeops::         # lobby -> forced ruleset, gear,
                                                              # flight, code, arm/disarm,
                                                              # detonation, report
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter nukeops
SS13_SKIP_LOBBY=1 SS13_DEV_TRAITOR=nukeop SS13_ASSETS=$ASSETS scripts/visual-check.sh \
    scripts/visual/nukeop_bomb.txt scripts/visual/nukeop_console.txt
```

### Blood cult

`Roundstart Blood Cult` (`crate::antag::cult`, tg `/datum/antagonist/cult`, `/datum/team/cult`;
weights 0/1/3/3, `min_pop` 30, high impact, exactly `ceil(population / 20) + 1` opted-in players or
nobody, never the head of personnel or the chaplain): one team. Each cultist gets the greeting, the
cult's objectives (sacrifice a target, unconvertible crew first; summon Nar'Sie in one of three
station areas with `VALID_TERRITORY`), the `AntagInfoGeneric` panel, the cult HUD icon over
itself and the cult mates it sees (`cultmaster` for the leader, the first roundstart cultist), a
ritual dagger and ten runed metal, and the Communion and Prepare Blood Magic buttons.

- The dagger in the active hand opens the rune list (`ListInputWindow` "Sigils of Power"); the
  rune is drawn on the cultist's tile after tg's `scribe_delay` (4 s; Nar'Sie 50 s) for
  `scribe_damage` brute to an arm (0.1; Nar'Sie 40.1), not in space, not on another rune, only in
  areas with `CULT_PERMITTED` (CentCom refuses). The teleport rune asks for a keyword
  (`TextInputModal`). The dagger on a rune erases it; on a cultist it turns their holy water
  into unholy water.
- An empty hand on a rune invokes it with the conscious cultists around it: Offer converts a
  living, convertible player on it with two (heals 75% of brute/burn, uncuffs, a new dagger) and
  sacrifices with three a living human or the sacrifice target (one for the rest; the body is
  destroyed, its items fall); mindshielded jobs, the chaplain, holy water in the blood and the
  sacrifice target are not convertible. Empower (blood magic next to it), Teleport (to another
  teleport rune, everything unanchored on it), Revive (a dead cultist for every three
  sacrifices), Barrier (a dense shield, toggled), Summon Cultist (two, 10 brute each), Nar'Sie
  (nine, in a summoning area, after the sacrifice; CentCom is told when it is drawn).
- Blood magic: one spell without an Empower rune next to the cultist (10 s, 40 blood), four with
  it (4 s, 8 blood); stun (16 s paralyze, 0.4 when risen, 0.1 when ascended), teleport to a rune,
  shadow shackles (3 s, 4 charges), twisted construction (plasteel into runed metal), summon dagger.
- Communion: the message is whispered aloud and read by every cultist and the dead as
  `Acolyte Name: text` (`Master` for the leader).
- Red eyes at 20% cultists of the other living players, halos and CentCom's announcement at 30%
  (10%/20% at 70+ players). Holy water in a cultist's blood clears the spells at once and
  deconverts after a minute.
- Nar'Sie rises 4 s after the invocation (a sprite: she does not move or consume); after
  BandaStation's announcements (15 + 50 + 5 + 60 + 12 s) the round ends; the report lists the
  victory or defeat, the objectives and the cultists.
- `SS13_DEV_TRAITOR=cult` (debug builds, lobby skipped): every joining body becomes a cultist with
  the dagger; admins: `antag <bot> cult` / the player panel's "Cultist" button (no equipment).

The cult's runes, dagger, auras and Nar'Sie are spawned at runtime
(`ss13_mapload::antag::CULT_RUNTIME_TYPES`) and the summoning areas need `DmPower::area_flags`:
regenerate the assets (`ss13-mapload convert`).

```sh
cargo test -p ss13_game --all-features --lib cult             # rune table, invokers, revives,
                                                              # blood magic limits, stages,
                                                              # holy water, objectives, ruleset caps
cargo test -p ss13_game --all-features --test it cult::            # lobby prefs -> forced ruleset,
                                                              # Communion, Offer rune: 1 vs 2
                                                              # invokers, mindshield, sacrifice,
                                                              # holy water, Nar'Sie ends the round
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter cult
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/arena.dmm SS13_DEV_TRAITOR=cult SS13_ASSETS=$ASSETS \
    scripts/visual-check.sh scripts/visual/cult.txt
```

### Heretics

`Roundstart Heretics` (`crate::antag::heretic`, tg `/datum/antagonist/heretic`; weight 3,
`min_pop` 30, `ceil(population / 24)` opted-in players, repeatable, no blacklist): each heretic
gets one knowledge point, the start knowledge (Mansus grasp, living heart, the sacrifice, the
Feast of Owls), the "Pick a path to pursue." objective, the `AntagInfoHeretic` window (the
introduction, the eight paths, the knowledge tree and shop), the heretic HUD icon and its spell
buttons, and a point every 20 minutes. The knowledge table (`antag/heretic/data.rs`) is generated
from the DM dump: `python3 -I scripts/gen-heretic-data.py <dm_types.json>`.

- Influences: `max(5 - n + 1, 1)` for the n-th heretic on random station floors. They are
  replicated only to heretics' clients (`VisibleToGroups`/`SeeGroups` in `ss13_net`) and ghosts.
  A right click with an empty hand drains one in 10 s for a point (a Codex Cicatrix: two); a
  pierced reality everyone sees takes its place.
- A pen or crayon on a floor with the grasp in the other hand draws the transmutation rune
  (20 s, open floor around, no rune within 3 tiles); the grasp on a rune erases it. An empty
  hand on the rune lists the rituals; a ritual takes what lies on and around the rune.
- Researching a path's starting knowledge picks the path: its tree, three drafts per tier and
  the shop, and the objectives (research, minor sacrifices, a head of staff with two heads
  alive). Final knowledge asks for confirmation (no more blade breaking).
- The living heart's button lists the sacrifice targets (a head, security, the own department,
  random crew up to five) and says how far and where. The sacrifice ritual takes a target in
  soft crit or worse: 2 points (a head 3); the victim, cult-cuffed and revived, lies paralysed
  for 12 s and wakes at a random station tile (no shadow realm).
- Path of Ash: the grasp marks (the ashen blade triggers it: stamina and burn that spreads),
  Volcano Blast (2.5 s channel, chains fire between the burning), Fiery Rebirth (drains burning
  crew within 14 tiles), the blade upgrade ignites, and the Ritual of the Ash Lord (three burning
  or husked corpses, all objectives done) ascends: CentCom's announcement, the shuttle, the Oath
  of Flame and the Fire Cascade. The other paths are in the tree without effects.
- `SS13_DEV_TRAITOR=heretic` (debug builds, lobby skipped): every joining body is a heretic;
  admins: `antag <bot> heretic` / the player panel's "Heretic" button.

Influences, the rune, the grasp's hand and the rituals' results are spawned at runtime
(`ss13_mapload::antag::HERETIC_RUNTIME_TYPES`): regenerate the assets (`ss13-mapload convert`).

```sh
cargo test -p ss13_game --all-features --lib heretic          # knowledge tree, drafts, recipes,
                                                              # objectives, heart, announcement
cargo test -p ss13_game --all-features --test it heretic::         # lobby prefs -> forced ruleset,
                                                              # influences only for the heretic,
                                                              # drain, research, living heart,
                                                              # grasp, rune, sacrifice, ascension
SS13_ASSETS=$ASSETS cargo run -p ss13_bots --bin ss13-bots -- --filter heretic
SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/heretic.dmm SS13_DEV_TRAITOR=heretic \
    SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/heretic.txt
```

### PDAs

Jobs spawn with the PDA of their tg outfit (belt or pocket, `crate::modpc`), imprinted with the
owner's name and assignment: "КПК «Имя» (Ассистент)". Clicking it in the active hand turns it on
and opens tg's NtOS window (`NtosMain`): header (round time, signal, battery, power off or
minimize/close), the details (flashlight, "Вытащить ID", "Привязать ID") and the programs. A
program opens in the same window; minimize keeps it in the background (header icon), close returns
to the menu. Programs: Direct Messenger, Notepad, Plexagon Crew List, and AtmoZphere on
engineering/atmos/science/CE PDAs.

- Messenger: every imprinted PDA on the station (name, job; no telecomms, like the radio). Pick a
  contact, type, Enter sends (one message per second, tg's cooldown). The recipient's holder gets
  "PDA message from Имя (Job), "..."" and everyone near the PDA hears `*beep*` (its ringtone,
  set in the contacts screen; "Ringer: Off" silences it). Chats keep the log, unread counts,
  close/delete.
- ID slot: an ID card in the active hand clicked on the PDA (in the other hand, a worn slot or on
  the floor) goes in; alt-click on the PDA's slot or "Вытащить ID" takes it out. A PDA with an ID in
  the ID slot (or in a hand) opens doors, lockers and vendors like the card (tg `GetID`).
- The flashlight lights the holder, or the PDA on the floor.

```sh
cargo test -p ss13_game --all-features --lib modpc        # program switching, imprint, act
                                                          # parsing, messenger routing, cooldown
cargo test -p ss13_game --all-features --test it pda::         # two players message each other; ID in
                                                          # the PDA in the ID slot opens a door;
                                                          # the traitor's ringtone code opens the
                                                          # uplink
SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/pda_receiver.txt \
    scripts/visual/pda_sender.txt                         # NtosMain, contacts, the conversation
```

### ID console and crew records

The identification console (`/obj/machinery/modular_computer/preset/id`, tg's NtOS console)
opens its NtOS window on a click with an empty hand; an ID card clicked on it goes into the main
slot, a right click (or "Вставить ID" in the program) into the secondary one. Plexagon Access
Management (tgui `NtosCard`): "Log In" with the main card: `ACCESS_CHANGE_IDS` edits every
station region and applies every job template; a head's office access edits its region only
(tg `sub_department_managers_tgui`). The secondary card's name, age and assignment, template,
termination and accesses per region and wildcard slot (tg trims: a trim's access is free, the
rest takes the card's wildcard slots: grey cards 4 common, silver/platinum/gold as tg). Every
edit is checked against the main card as it is at that moment. Closing the program writes the
card into its crew record (`GLOB.manifest.modify`).

ID lookups resolve a PDA to the card in its slot by reference (`crate::inventory::get_id`), so a
card changed while in a PDA opens doors at once.

Every crew member who joins gets a crew record and a locked record (`crate::records`): name,
rank, age 30, species, gender, blood type, tg-style random fingerprint and DNA hashes. The crew
list program reads them. The security records console (`SecurityRecords`; log in with security
or HoP access): wanted status (Нет, Подозрение, Арест — needs a crime, Заключение, УДО,
Оправдано), crimes and citations with author and round time, voiding (the author or armory
access; the last valid crime discharges), notes and fields. The medical records console
(`MedicalRecords`; medical, detective or genetics access): physical and mental status, cause of
death, blood type, up to three notes. Security HUD glasses (`/obj/item/clothing/glasses/hud/security`
and subtypes, `.../medsechud`) show the wearer the job icon of every seen human's worn ID and its
record's wanted icon; medical HUD glasses (`.../hud/health`, `.../medsechud`) the health bar and
status. `ss13-mapload convert` writes the trims' wildcard and template access and SecHUD icons
into `jobs.ron`: rerun it after updating.

```sh
cargo test -p ss13_game --all-features --lib id_access records  # wildcard slots, who edits what,
                                                                # record edits, HUD icons
cargo test -p ss13_game --all-features --test it records::    # HoP gives access over the console, the
                                                         # door opens, also through a PDA; Arrest
                                                         # shows on the officer's security HUD
SS13_DEV_JOB="Head of Personnel" SS13_LIGHTING=0 SS13_MAP=$PWD/scripts/visual/maps/records.dmm \
    SS13_ASSETS=$ASSETS scripts/visual-check.sh scripts/visual/records.txt
                                                         # the ID console, the records console
```

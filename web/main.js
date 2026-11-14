// Landing -> download everything in data/files.json and both wasm binaries -> server worker
// (server-worker.js) -> client on the canvas. Message shapes: docs/RUNNING.md, "Browser demo".
const $ = (id) => document.getElementById(id);
const PARALLEL_FETCHES = 6;
// Gzipped by web-build.sh (Cloudflare Pages' 25 MiB file limit); inflated here.
const CLIENT_WASM = "client/ss13_web_client_bg.wasm.gz";
const SERVER_WASM = "server/ss13_web_server_bg.wasm.gz";
const SIDES = new Set(["server", "client", "both"]);

const brands = navigator.userAgentData?.brands ?? [];
if (!brands.some((b) => b.brand === "Google Chrome" || b.brand === "Chromium")) {
  $("browser-note").hidden = false;
}

$("start").addEventListener(
  "click",
  () => {
    unlockAudio();
    run().catch(fail);
  },
  { once: true },
);

// The click is the page's user activation: an AudioContext started in it lets the ones bevy_audio
// creates later play without another gesture.
function unlockAudio() {
  try {
    const ctx = new AudioContext();
    ctx.resume().finally(() => ctx.close());
  } catch {
    // No Web Audio: the game runs silent.
  }
}

function show(id) {
  for (const el of ["landing", "loader", "error", "game"]) $(el).hidden = el !== id;
}

function fail(e) {
  console.error(e);
  $("error-text").textContent = e?.message ?? String(e);
  show("error");
}

const mb = (bytes) => (bytes / 1048576).toFixed(1);

function wasmSizes() {
  try {
    return JSON.parse(document.body.dataset.wasmBytes);
  } catch {
    return {}; // unbuilt page (placeholder left): progress counts the data only
  }
}

// A pack gzipped by web-pack.py: inflated before its files are cut out.
const isGz = (path) => path.endsWith(".gz");

const isGzip = (bytes) => bytes[0] === 0x1f && bytes[1] === 0x8b;

// Inflates a gzip body unless the host already decoded it (Content-Encoding: gzip).
async function gunzip(bytes) {
  if (!isGzip(bytes)) return bytes;
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function download(url, size, onBytes, exact = true) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  const reader = res.body.getReader();
  const chunks = [];
  let got = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    got += value.length;
    onBytes(value.length);
  }
  // A mismatch means a cached file from another build: mixing builds fails later and obscurely.
  if (exact && size !== undefined && got !== size) {
    throw new Error(`${url}: ожидалось ${size} байт, получено ${got}. Обновите страницу.`);
  }
  if (chunks.length === 1) return chunks[0];
  const out = new Uint8Array(got);
  let offset = 0;
  for (const c of chunks) {
    out.set(c, offset);
    offset += c.length;
  }
  return out;
}

async function inParallel(items, limit, fn) {
  let next = 0;
  const lane = async () => {
    while (next < items.length) await fn(items[next++]);
  };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, lane));
}

async function run() {
  show("loader");
  const bar = $("loader-bar");
  const text = $("loader-text");

  const listRes = await fetch("data/files.json");
  if (!listRes.ok) throw new Error(`data/files.json: HTTP ${listRes.status}`);
  const { packs } = await listRes.json();
  for (const p of packs) {
    if (!SIDES.has(p.side)) throw new Error(`data/files.json: ${p.url}: side "${p.side}"`);
  }
  const now = packs.filter((p) => !p.later);
  const later = packs.filter((p) => p.later);

  const sizes = wasmSizes();
  // Not exact: a host that decodes gzip itself delivers more bytes than the file size.
  const jobs = [
    { url: CLIENT_WASM, size: sizes.client, exact: false },
    { url: SERVER_WASM, size: sizes.server, exact: false },
    ...now.map((p) => ({ url: `data/${p.url}`, size: p.size, pack: p })),
  ];
  const total = jobs.reduce((sum, j) => sum + (j.size ?? 0), 0);
  let loaded = 0;
  const onBytes = (n) => {
    loaded += n;
    bar.value = total ? Math.min(loaded / total, 1) : 0;
    text.textContent = `${mb(loaded)} / ${mb(total)} МБ`;
  };
  onBytes(0);
  const serverFiles = new Map();
  const clientFiles = new Map();
  const transfer = [];
  // A later pack's files are listed now (the sound list is read at the start) and filled in when
  // the pack arrives: until then a read finds no bytes and the sound is skipped.
  for (const p of later) for (const [path] of p.files) clientFiles.set(path, null);
  const got = new Map();
  await inParallel(jobs, PARALLEL_FETCHES, async (j) => {
    if (!j.pack) {
      got.set(j.url, await download(j.url, j.size, onBytes, j.exact));
      return;
    }
    const bytes = await fetchPack(j.pack, onBytes);
    unpack(j.pack, bytes, (path, file) => {
      if (j.pack.side !== "client") serverFiles.set(path, file);
      if (j.pack.side !== "server") clientFiles.set(path, file);
    });
    if (j.pack.side === "server") transfer.push(bytes.buffer);
  });
  const serverWasm = await gunzip(got.get(SERVER_WASM));
  const clientWasm = await gunzip(got.get(CLIENT_WASM));
  got.clear();

  $("loader-title").textContent = "Запуск сервера";
  bar.removeAttribute("value");
  text.textContent = "";
  const worker = new Worker("server-worker.js", { type: "module" });
  let client = null;
  let onReady;
  const ready = new Promise((resolve) => (onReady = resolve));
  worker.onmessage = ({ data: msg }) => {
    switch (msg.t) {
      case "progress":
        text.textContent = msg.stage;
        break;
      case "ready":
        onReady();
        break;
      case "error":
        worker.terminate();
        fail(new Error(`Сервер: ${msg.message}`));
        break;
      default:
        client?.on_worker_message(msg);
    }
  };
  worker.onerror = (e) => fail(new Error(`server-worker.js: ${e.message || "не загрузился"}`));
  // `wasm` (not in files.json): the worker instantiates these bytes instead of fetching again.
  worker.postMessage({ t: "init", files: serverFiles, wasm: serverWasm }, [
    ...transfer,
    serverWasm.buffer,
  ]);
  await ready;

  $("loader-title").textContent = "Запуск клиента";
  text.textContent = "";
  const mod = await import("./client/ss13_web_client.js");
  await mod.default({ module_or_path: clientWasm });
  show("game"); // the canvas needs its size before the client builds its window
  client = mod;
  try {
    mod.start("#game", clientFiles, (msg) =>
      worker.postMessage(msg, msg.b instanceof ArrayBuffer ? [msg.b] : []),
    );
  } catch (e) {
    // winit's web event loop may leave `run` through this exception on purpose.
    if (!String(e?.message ?? e).includes("Using exceptions for control flow")) throw e;
  }
  $("game").focus();
  // The client reads `clientFiles` on demand: filling it in is all a later pack needs.
  inParallel(later, 2, async (p) => {
    const bytes = await fetchPack(p, () => {});
    unpack(p, bytes, (path, file) => clientFiles.set(path, file));
  }).catch((e) => console.error("sounds:", e));
}

// A pack of files.json, inflated, its size checked.
async function fetchPack(pack, onBytes) {
  const url = `data/${pack.url}`;
  const gz = isGz(pack.url);
  const bytes = await download(url, pack.size, onBytes, !gz);
  // Still gzip: the host sent the file as stored, so the size check of `download` applies.
  if (gz && isGzip(bytes) && bytes.length !== pack.size) {
    throw new Error(`${url}: ожидалось ${pack.size} байт, получено ${bytes.length}. Обновите страницу.`);
  }
  return gz ? gunzip(bytes) : bytes;
}

// Every file of `pack` as a view into `bytes` (no copy).
function unpack(pack, bytes, put) {
  for (const [path, offset, length] of pack.files) {
    if (offset + length > bytes.length) throw new Error(`${pack.url}: ${path} за концом пакета`);
    put(path, bytes.subarray(offset, offset + length));
  }
}


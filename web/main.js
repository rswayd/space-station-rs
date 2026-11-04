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

// A data file gzipped by web-build.sh (atlas pages): inflated and mounted without `.gz`.
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
  const list = await listRes.json();
  for (const f of list) {
    if (!SIDES.has(f.side)) throw new Error(`data/files.json: ${f.path}: side "${f.side}"`);
  }

  const sizes = wasmSizes();
  // Not exact: a host that decodes gzip itself delivers more bytes than the file size.
  const jobs = [
    { url: CLIENT_WASM, size: sizes.client, exact: false },
    { url: SERVER_WASM, size: sizes.server, exact: false },
    ...list.map((f) => ({ url: `data/${f.path}`, size: f.size, gz: isGz(f.path) })),
  ];
  const total = jobs.reduce((sum, j) => sum + (j.size ?? 0), 0);
  let loaded = 0;
  const onBytes = (n) => {
    loaded += n;
    bar.value = total ? Math.min(loaded / total, 1) : 0;
    text.textContent = `${mb(loaded)} / ${mb(total)} МБ`;
  };
  onBytes(0);
  const got = new Map();
  await inParallel(jobs, PARALLEL_FETCHES, async (j) => {
    const bytes = await download(j.url, j.size, onBytes, !j.gz && (j.exact ?? true));
    // Still gzip: the host sent the file as stored, so the size check of `download` applies.
    if (j.gz && isGzip(bytes) && bytes.length !== j.size) {
      throw new Error(`${j.url}: ожидалось ${j.size} байт, получено ${bytes.length}. Обновите страницу.`);
    }
    got.set(j.url, j.gz ? await gunzip(bytes) : bytes);
  });

  const serverFiles = new Map();
  const clientFiles = new Map();
  const transfer = [];
  for (const f of list) {
    const bytes = got.get(`data/${f.path}`);
    const path = isGz(f.path) ? f.path.slice(0, -3) : f.path;
    if (f.side !== "client") serverFiles.set(path, bytes);
    if (f.side !== "server") clientFiles.set(path, bytes);
    if (f.side === "server") transfer.push(bytes.buffer);
  }
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
}

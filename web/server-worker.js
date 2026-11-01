// Module worker running the server wasm (docs/WEB.md). The page posts {t: "init", files, wasm?}
// once; the server posts progress/ready/error and loopback messages through `post`; every later
// page message goes to the server.
let server = null;

const post = (msg) => postMessage(msg, msg.b instanceof ArrayBuffer ? [msg.b] : []);

onmessage = async ({ data: msg }) => {
  if (msg.t !== "init") {
    server?.on_page_message(msg);
    return;
  }
  try {
    const mod = await import("./server/ss13_web_server.js");
    await mod.default(msg.wasm ? { module_or_path: msg.wasm } : undefined);
    server = mod;
    mod.start(msg.files, post);
  } catch (e) {
    postMessage({ t: "error", message: String(e?.message ?? e) });
  }
};

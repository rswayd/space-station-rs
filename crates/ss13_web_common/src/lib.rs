//! Browser demo (`docs/WEB.md`): what the worker entry (`ss13_web_server`) and the page entry
//! (`ss13_web_client`) share. Messages are plain objects: `{t: "open", id}`, `{t: "frame", id, b}`,
//! `{t: "dgram", id, b}` (`b`: an `ArrayBuffer`, transferred), `{t: "close", id, code, reason}`.
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::HashMap;

use js_sys::{Array, ArrayBuffer, Function, Map, Object, Reflect, Uint8Array};
use ss13_net::loopback::LoopbackEnd;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// The global one: a worker has no `window`, node has neither.
    #[wasm_bindgen(js_name = setTimeout)]
    fn set_timeout(handler: &Function, ms: i32) -> JsValue;
    #[wasm_bindgen(js_name = setInterval)]
    fn set_interval(handler: &Function, ms: i32) -> JsValue;
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

/// Runs `f` once, after the current JS task: lets the caller return and posted messages go out.
pub fn after(ms: i32, f: impl FnOnce() + 'static) {
    set_timeout(&Closure::once_into_js(f).unchecked_into(), ms);
}

/// Runs `f` every `ms` for the life of the page or worker.
pub fn every(ms: i32, f: impl FnMut() + 'static) {
    let f = Closure::<dyn FnMut()>::new(f).into_js_value();
    set_interval(f.unchecked_ref(), ms);
}

/// The JS `post(msg, transfer)` callback handed to `start`.
#[derive(Clone)]
pub struct Post(Function);

impl Post {
    pub fn new(f: Function) -> Self {
        Self(f)
    }

    /// Posts `{t, ...fields}`; `bytes` goes out as the transferred `b`.
    pub fn send(&self, t: &str, fields: &[(&str, JsValue)], bytes: Option<&[u8]>) {
        let msg = Object::new();
        let set = |k: &str, v: &JsValue| {
            let _ = Reflect::set(&msg, &k.into(), v);
        };
        set("t", &t.into());
        for (k, v) in fields {
            set(k, v);
        }
        let transfer = Array::new();
        if let Some(bytes) = bytes {
            let buffer = Uint8Array::from(bytes).buffer();
            set("b", &buffer);
            transfer.push(&buffer);
        }
        if let Err(e) = self.0.call2(&JsValue::NULL, &msg, &transfer) {
            console_error(&format!("post {t}: {e:?}"));
        }
    }
}

thread_local! {
    static FILES: RefCell<Option<Map>> = const { RefCell::new(None) };
    static PANIC_POST: RefCell<Option<Post>> = const { RefCell::new(None) };
}

/// Serves every `ss13_data` read from `files` (path → `Uint8Array`, as `data/files.json` names
/// them) and then `extra`. Bytes are copied into wasm memory only when read: the page keeps the
/// atlas pages in JS until a page is first drawn.
pub fn mount_files(files: Map, extra: Vec<(String, Vec<u8>)>) {
    let mut keys: Vec<String> = files
        .keys()
        .into_iter()
        .filter_map(|k| k.ok()?.as_string())
        .collect();
    keys.extend(extra.iter().map(|(k, _)| k.clone()));
    FILES.with(|f| *f.borrow_mut() = Some(files));
    let mounted = ss13_data::mount(keys, move |key| {
        let from_js = FILES.with(|f| {
            let value = f.borrow().as_ref()?.get(&key.into());
            value
                .dyn_ref::<Uint8Array>()
                .map(Uint8Array::to_vec)
                .or_else(|| {
                    value
                        .dyn_ref::<ArrayBuffer>()
                        .map(|b| Uint8Array::new(b).to_vec())
                })
        });
        from_js.or_else(|| extra.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()))
    });
    if !mounted {
        console_error("data bundle already mounted");
    }
}

/// Panics go to the console and, with `post`, out as `{t: "error", message}`.
pub fn set_panic_hook(post: Option<Post>) {
    PANIC_POST.with(|p| *p.borrow_mut() = post);
    std::panic::set_hook(Box::new(|info| {
        let message = info.to_string();
        console_error(&message);
        // A panic inside a `post` call (a JS exception) must not recurse.
        let post = PANIC_POST.with(|p| p.try_borrow().ok().and_then(|p| p.clone()));
        if let Some(post) = post {
            post.send("error", &[("message", message.into())], None);
        }
    }));
}

/// A message field.
pub fn field(msg: &JsValue, name: &str) -> JsValue {
    Reflect::get(msg, &name.into()).unwrap_or(JsValue::UNDEFINED)
}

pub fn msg_type(msg: &JsValue) -> Option<String> {
    field(msg, "t").as_string()
}

pub fn msg_id(msg: &JsValue) -> Option<u32> {
    field(msg, "id")
        .as_f64()
        .filter(|id| id.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(id))
        .map(|id| id as u32)
}

/// One side of the loopback bridge: the local ends of the connections the other side mirrors.
/// It never decodes: frames go through as the bytes `ss13_net` produced and are validated by
/// the receiving `ss13_net`.
pub struct Bridge {
    post: Post,
    ends: HashMap<u32, LoopbackEnd>,
}

impl Bridge {
    pub fn new(post: Post) -> Self {
        Self {
            post,
            ends: HashMap::new(),
        }
    }

    pub fn post(&self) -> &Post {
        &self.post
    }

    pub fn insert(&mut self, id: u32, end: LoopbackEnd) {
        self.ends.insert(id, end);
    }

    /// Applies a `frame`, `dgram` or `close` from the other side; `false` for any other message.
    /// Unknown ids are ignored: the connection already closed here.
    pub fn apply(&mut self, msg: &JsValue) -> bool {
        let Some(t) = msg_type(msg) else {
            return false;
        };
        if !matches!(t.as_str(), "frame" | "dgram" | "close") {
            return false;
        }
        let Some(id) = msg_id(msg) else {
            return true;
        };
        let Some(end) = self.ends.get(&id) else {
            return true;
        };
        let bytes = || {
            let b = field(msg, "b");
            b.dyn_ref::<ArrayBuffer>()
                .map(|b| Uint8Array::new(b).to_vec())
        };
        match t.as_str() {
            "frame" => {
                if let Some(b) = bytes() {
                    end.send_frame(&b);
                }
            }
            "dgram" => {
                if let Some(b) = bytes() {
                    end.send_datagram(b);
                }
            }
            _ => {
                let code = field(msg, "code").as_f64().unwrap_or(0.0) as u32;
                let reason = field(msg, "reason").as_string().unwrap_or_default();
                end.close(code, &reason);
                self.ends.remove(&id);
            }
        }
        true
    }

    /// Forwards what the local ends received; a closed end goes out as `close` and is dropped.
    pub fn pump(&mut self) {
        let post = &self.post;
        self.ends.retain(|&id, end| {
            let id_field = ("id", JsValue::from(id));
            while let Some(datagram) = end.recv_datagram() {
                post.send("dgram", std::slice::from_ref(&id_field), Some(&datagram));
            }
            loop {
                match end.recv_frame() {
                    Ok(Some(frame)) => {
                        post.send("frame", std::slice::from_ref(&id_field), Some(&frame))
                    }
                    Ok(None) => return true,
                    Err((code, reason)) => {
                        post.send(
                            "close",
                            &[id_field, ("code", code.into()), ("reason", reason.into())],
                            None,
                        );
                        return false;
                    }
                }
            }
        });
    }
}

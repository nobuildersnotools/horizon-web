//! A server connection on the web, where pages have no TCP: a WebSocket to the
//! server's own port (Horizon's servers take WebSockets beside Minecraft's
//! TCP), read and written as a blocking byte stream.
//!
//! The socket lives on the page, whose thread hands the browser its events;
//! the threads that read it are workers, which wait for its bytes. A worker
//! asks the page to open, flush or close a socket by a message. What threads
//! write goes into one queue the page sends from, so the bytes keep their
//! order whichever thread wrote them.
//!
//! A socket opened by URL (an Eaglercraft server's) is read and written a
//! message at a time instead ([`WebSocket::send`], [`WebSocket::recv`]).

use crate::time::Duration;
use std::{
    collections::{HashMap, VecDeque},
    io::{self, Read, Write},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
const sockets = new Map();
let onEvent = null, take = null;
function open(id, url, host, port) {
    if (!url) {
        const scheme = location.protocol === "https:" ? "wss" : "ws";
        const name = host.includes(":") ? `[${host}]` : host;
        url = `${scheme}://${name}:${port}/`;
    }
    let ws;
    try {
        ws = new WebSocket(url);
    } catch (e) {
        onEvent(id, 2, null, String(e.message ?? e));
        return;
    }
    ws.binaryType = "arraybuffer";
    sockets.set(id, ws);
    ws.onopen = () => {
        onEvent(id, 0, null, "");
        flush(id);
    };
    ws.onmessage = ({ data }) => {
        if (data instanceof ArrayBuffer) onEvent(id, 1, new Uint8Array(data), "");
        else onEvent(id, 3, null, data);
    };
    ws.onclose = ({ code, reason }) => {
        sockets.delete(id);
        onEvent(id, 2, null, reason || (code === 1006 ? "Connection refused" : `closed (${code})`));
    };
}
function flush(id) {
    const ws = sockets.get(id);
    if (ws && ws.readyState === WebSocket.OPEN) {
        for (const data of take(id)) ws.send(data);
    }
}
function close(id) {
    const ws = sockets.get(id);
    sockets.delete(id);
    ws?.close();
}
function onPage(data) {
    if (data.socket === "open") open(data.id, data.url, data.host, data.port);
    else if (data.socket === "flush") flush(data.id);
    else close(data.id);
}
export function horizon_socket_install(events, taker) {
    onEvent = events;
    take = taker;
    // What workers ask of the page arrives at the worker objects the page
    // made (workers.rs), which hand it here.
    globalThis.horizonSocket = onPage;
}
export function horizon_socket_ask(data) {
    if (typeof window === "undefined") self.postMessage(data);
    else onPage(data);
}
"#)]
extern "C" {
    fn horizon_socket_install(events: &JsValue, take: &JsValue);
    fn horizon_socket_ask(data: &JsValue);
}

/// One WebSocket message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Binary(Vec<u8>),
    Text(String),
}

/// What the page knows of a socket, and what its threads wrote.
#[derive(Default)]
struct State {
    open: bool,
    /// Why it closed, once it has.
    closed: Option<String>,
    incoming: VecDeque<Message>,
    /// How much of the first incoming message a byte read has taken.
    taken: usize,
    outgoing: Vec<Message>,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    ready: Condvar,
}

fn sockets() -> &'static Mutex<HashMap<u32, Arc<Shared>>> {
    static SOCKETS: OnceLock<Mutex<HashMap<u32, Arc<Shared>>>> = OnceLock::new();
    SOCKETS.get_or_init(Mutex::default)
}

fn find(id: u32) -> Option<Arc<Shared>> {
    crate::sync::lock(sockets())
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .cloned()
}

fn ask(what: &str, id: u32, address: Option<(&str, &str, u16)>) {
    let data = js_sys::Object::new();
    let set = |key: &str, value: JsValue| {
        let _ = js_sys::Reflect::set(&data, &JsValue::from_str(key), &value);
    };
    set("socket", JsValue::from_str(what));
    set("id", JsValue::from(id));
    if let Some((url, host, port)) = address {
        set("url", JsValue::from_str(url));
        set("host", JsValue::from_str(host));
        set("port", JsValue::from(port));
    }
    horizon_socket_ask(&data);
}

/// Hands the page's socket code the game's side of it. Called on the page
/// before the game starts.
pub(crate) fn install() {
    let events = Closure::<dyn FnMut(u32, u32, JsValue, String)>::new(
        |id: u32, kind: u32, data: JsValue, reason: String| {
            let Some(shared) = find(id) else { return };
            let mut state = crate::sync::lock(&shared.state).unwrap_or_else(|e| e.into_inner());
            match kind {
                0 => state.open = true,
                1 => state
                    .incoming
                    .push_back(Message::Binary(js_sys::Uint8Array::new(&data).to_vec())),
                3 => state.incoming.push_back(Message::Text(reason)),
                _ => {
                    state.closed.get_or_insert(reason);
                    crate::sync::lock(sockets())
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&id);
                }
            }
            drop(state);
            shared.ready.notify_all();
        },
    );
    let take = Closure::<dyn FnMut(u32) -> JsValue>::new(|id: u32| {
        let messages = js_sys::Array::new();
        let Some(shared) = find(id) else {
            return messages.into();
        };
        let mut state = crate::sync::lock(&shared.state).unwrap_or_else(|e| e.into_inner());
        for message in state.outgoing.drain(..) {
            messages.push(&match message {
                // A copy outside the shared memory, which `WebSocket.send`
                // refuses.
                Message::Binary(data) => js_sys::Uint8Array::from(&data[..]).into(),
                Message::Text(text) => JsValue::from_str(&text),
            });
        }
        messages.into()
    });
    horizon_socket_install(events.as_ref(), take.as_ref());
    events.forget();
    take.forget();
}

/// Closes the socket when its last clone goes, as a TCP socket closes.
struct Handle {
    id: u32,
    shared: Arc<Shared>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        close(self.id, &self.shared);
    }
}

fn close(id: u32, shared: &Shared) {
    let mut state = crate::sync::lock(&shared.state).unwrap_or_else(|e| e.into_inner());
    if state.closed.is_none() {
        state.closed = Some("Connection closed".into());
        drop(state);
        shared.ready.notify_all();
        crate::sync::lock(sockets())
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        ask("close", id, None);
    }
}

/// A WebSocket to `host:port`, read and written like a `TcpStream`. Reads
/// wait, so only workers read it.
pub struct WebSocket {
    handle: Arc<Handle>,
    timeout: Mutex<Option<Duration>>,
}

impl WebSocket {
    /// Opens the socket (`wss:` on an `https:` page, else `ws:`) and waits
    /// until it is open. Only on a worker: the page can't wait.
    pub fn connect(host: &str, port: u16, timeout: Duration) -> io::Result<Self> {
        Self::open(("", host, port), timeout)
    }

    /// Opens a socket to a `ws:` or `wss:` URL, to read a message at a time.
    pub fn connect_url(url: &str, timeout: Duration) -> io::Result<Self> {
        Self::open((url, "", 0), timeout)
    }

    fn open(address: (&str, &str, u16), timeout: Duration) -> io::Result<Self> {
        if !crate::workers::in_worker() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "the page's thread can't wait for a connection",
            ));
        }
        static NEXT: AtomicU32 = AtomicU32::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let shared = Arc::new(Shared::default());
        crate::sync::lock(sockets())
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, shared.clone());
        let socket = Self {
            handle: Arc::new(Handle { id, shared }),
            timeout: Mutex::new(None),
        };
        ask("open", id, Some(address));
        let shared = &socket.handle.shared;
        let state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let (state, timed_out) =
            crate::sync::wait_timeout_while(&shared.ready, state, timeout, |s| {
                !s.open && s.closed.is_none()
            });
        if let Some(reason) = &state.closed {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                reason.clone(),
            ));
        }
        if timed_out {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Connection timed out",
            ));
        }
        drop(state);
        Ok(socket)
    }

    pub fn try_clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            timeout: Mutex::new(*self.timeout.lock().unwrap_or_else(|e| e.into_inner())),
        }
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) {
        *self.timeout.lock().unwrap_or_else(|e| e.into_inner()) = timeout;
    }

    /// Closes the socket: reads then see the end of the stream.
    pub fn shutdown(&self) {
        close(self.handle.id, &self.handle.shared);
    }

    /// Sends one message.
    pub fn send(&self, message: Message) -> io::Result<()> {
        let handle = &self.handle;
        let mut state = crate::sync::lock(&handle.shared.state).unwrap_or_else(|e| e.into_inner());
        if state.closed.is_some() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        // The page sends all that is queued: one request covers the messages
        // written before it gets to it.
        let first = state.outgoing.is_empty();
        state.outgoing.push(message);
        drop(state);
        if first {
            ask("flush", handle.id, None);
        }
        Ok(())
    }

    /// The next whole message, waiting for it as a read does (with the read
    /// timeout); `None` once the socket closed.
    pub fn recv(&self) -> io::Result<Option<Message>> {
        let mut state = self.wait()?;
        state.taken = 0;
        Ok(state.incoming.pop_front())
    }

    /// The state once a message came or the socket closed.
    fn wait(&self) -> io::Result<std::sync::MutexGuard<'_, State>> {
        let shared = &self.handle.shared;
        let mut state = crate::sync::lock(&shared.state).unwrap_or_else(|e| e.into_inner());
        if crate::workers::in_worker() {
            let pending = |s: &mut State| s.incoming.is_empty() && s.closed.is_none();
            match *self.timeout.lock().unwrap_or_else(|e| e.into_inner()) {
                None => {
                    while pending(&mut state) {
                        state = shared.ready.wait(state).unwrap_or_else(|e| e.into_inner());
                    }
                }
                Some(timeout) => {
                    let timed_out;
                    (state, timed_out) =
                        crate::sync::wait_timeout_while(&shared.ready, state, timeout, pending);
                    if timed_out {
                        return Err(io::ErrorKind::TimedOut.into());
                    }
                }
            }
        } else if state.incoming.is_empty() && state.closed.is_none() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(state)
    }
}

impl Read for WebSocket {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let mut state = self.wait()?;
        // A byte stream has no use for text messages.
        while let Some(Message::Text(_)) = state.incoming.front() {
            state.incoming.pop_front();
            if state.incoming.is_empty() {
                drop(state);
                state = self.wait()?;
            }
        }
        let taken = state.taken;
        let Some(Message::Binary(data)) = state.incoming.front() else {
            return Ok(0);
        };
        let n = out.len().min(data.len() - taken);
        out[..n].copy_from_slice(&data[taken..taken + n]);
        if taken + n == data.len() {
            state.incoming.pop_front();
            state.taken = 0;
        } else {
            state.taken += n;
        }
        Ok(n)
    }
}

impl Write for WebSocket {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.send(Message::Binary(data.to_vec()))?;
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

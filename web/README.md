# Horizon in the browser

An experimental build of the Horizon client for the web: the same game code
compiled to WebAssembly, drawing through the browser's WebGPU. It creates and
opens single-player worlds, joins offline-mode Horizon and Eaglercraft
servers, plays sound,
reads controllers, and keeps the worlds and
the options in the browser's storage (see [What's missing](#whats-missing)).

## Build and run

You need the prepared resources in `.local/horizon-client/26.3` (see the main
README's *Build from source*), and once:

```sh
rustup target add wasm32-unknown-unknown
rustup toolchain install nightly-2026-10-14 --component rust-src   # threads: std rebuilt with atomics
cargo install wasm-bindgen-cli --version 0.2.129   # the version in Cargo.lock
sudo pacman -S binaryen       # wasm-opt, for the release build (or your system's binaryen package)
```

Sounds come with the prepared resources when your Minecraft install has them
(`horizon-prepare sounds` adds them to resources prepared without).

Then:

```sh
web/build.sh        # the game to web/dist/pkg, the resources to web/dist/fs
python3 web/serve.py
```

and open <http://localhost:8080> in a browser with WebGPU. Chrome and Edge have it
on Windows, macOS and ChromeOS. On Linux, Chromium-based browsers need it switched
on: `brave --enable-unsafe-webgpu --enable-features=Vulkan http://localhost:8080`
(or the same flags for `chromium` / `google-chrome`).

`web/build.sh` makes the release build: LTO, then `wasm-opt` when it is installed
(slow to build; what players should get). `PROFILE=fast web/build.sh` skips both and
rebuilds in about a minute while you work. `THREADS=0 web/build.sh` builds with
stable Rust and no threads.

### Browsers

The game needs WebGPU, and the threaded build a cross-origin isolated page
(`web/serve.py` sends the headers; any host must send
`Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp`):

| Browser | Platforms |
|---|---|
| Chrome, Edge 113+ | Windows, macOS, ChromeOS |
| Chrome 121+ | Android |
| Safari 26 | macOS, iOS, iPadOS |
| Firefox 141+ | Windows |
| Chrome, Edge, Brave | Linux, with `--enable-unsafe-webgpu --enable-features=Vulkan` |

A browser without WebGPU, or with no GPU it can use, gets a message saying so
and this list instead of the game.

The address passes the game its command-line arguments:
<http://localhost:8080/?args=--world%20Test%20--creative> opens (or creates) the
world `Test` in creative mode at start.

## How it fits together

- **`horizon-os`** is what the game sees of the operating system: `fs`, `time`,
  `thread`, `sync`, `env` and `process`. On native they are std's own; in the
  browser, `fs` is a virtual filesystem in memory, `time` is the browser's
  clock, threads are Web Workers, and scoped threads run one after another. The game's crates call `horizon_os::fs`
  where they called `std::fs`, and `path.fs_exists()` (`PathExt`) where they
  called `path.exists()`.
- **The resources** go to the page as one archive: `web/bundle.py` packs
  everything the game reads to reach the title screen and open a world into
  `boot.tar.gz` (about 15 MB). The rest (other languages, the world generator's
  bundle) is listed in `manifest.txt` and fetched the first time it is read.
- **Startup** (`web/main.js`): check for WebGPU, start the service worker, load
  the WebAssembly, download and install the archive, load the player's saved
  files, then `horizon_start()`. The first renderer opens asynchronously
  (`Loading::resumed`), because the page can't wait for the GPU.
- **Threads** (`horizon-os/src/workers.rs`, `web/worker.js`): the module is
  built with atomics on a shared memory (std rebuilt on nightly, the linker's
  `--shared-memory`). Each spawned thread is a module worker that instantiates
  the same module on that memory and runs the closure it was handed. Workers
  are always started by the page, since a worker's own JS is busy running its
  thread. The page's thread never waits: browsers trap a wait there, so
  `sleep` returns at once and `join` spins. std's `recv_timeout` and
  `wait_timeout_while` read a clock std doesn't have here: `horizon_os::sync`
  has versions that use the browser's.
- **Locks the page shares with workers**: std's `Mutex` waits once it has spun
  briefly, which traps on the page (`Atomics.wait cannot be called in this
  context`; winit swallows it and the frame loop stops). Those locks go through
  `horizon_os::sync::lock` and `write`, which spin on the page instead: the
  chunk queue and the virtual filesystem so far.
- **Single player** runs as on native: the server thread, its local connection
  and the client's network thread are Web Workers. The window's calls into its
  server (`Integrated::call`) spin on the page for the reply
  (`horizon_os::sync::recv_reply`).
- **Saving**: what the game writes outside `/tmp` and `/.local` (options,
  `saves/`, `servers.dat`) is also kept in the origin's private file system
  (OPFS), in a `horizon` folder laid out as in the virtual filesystem. The
  page loads that folder before the game starts; the virtual filesystem notes
  each path the game changes, and every 2 s (and when the tab is hidden) the
  page takes those paths with their state now (`horizon_take_changes`) and
  writes them with OPFS's async API, which the page's thread may use. Each
  file is swapped in whole, but a batch isn't: a tab closed mid-batch can keep
  some files of a save and not others. What the game hasn't saved yet (between
  autosaves) is lost with the tab, as with a crash on native.
- **Pointer lock**: browsers lock the pointer only during a click or a key
  press, and winit's request throws through the frame when refused. The game
  asks through `horizon_os::web::pointer_lock`, and the canvas takes the lock
  on the player's next click. Escape while the pointer is locked only releases
  the lock and never reaches the page, so the game takes a lost lock during
  play as that Escape and pauses (`horizon_os::web::pointer_lock_lost`).
- **Sound**: cpal's Web Audio backend (`audio.rs` mixes as on native). Its
  `AudioContext` starts suspended until the player clicks or presses a key;
  `web/audio.js` wraps the class before the game's glue loads (the glue keeps
  the one it finds) and resumes every context on the first click or key.
  Sound files are in `manifest.txt`, fetched when a sound first plays, by the
  decoder and music workers. The mixer and decoded-sound locks are shared
  between the page (cpal's callback) and those workers, so they go through
  `horizon_os::sync::lock`.
- **Controllers**: gilrs reads the browser's Gamepad API. The DualSense's own
  output (rumble, adaptive triggers, lights) needs HID, which pages don't
  have: `horizon-dualsense` builds without it, as on iOS.
- **Caching** (`web/sw.js`): `build.sh` writes `build.json` with the content
  hashes of the WebAssembly and `boot.tar.gz`, and the page asks for them as
  `…?v=<hash>`. The service worker keeps those until a build changes them;
  everything else goes to the network first and falls back to its copy offline.
  Files fetched when read (`fs/res/`) aren't kept.
- **Multiplayer**: pages have no TCP, so the client reaches a server through a
  WebSocket to the server's own port (`horizon_os::net`): `ws://host:port/`,
  or `wss:` when the page is `https:`. The socket lives on the page, whose
  thread gets its events; the network and ping workers read it like a socket,
  waiting on a condvar, and ask the page by a message to open, flush or close
  it. All writes go into one queue the page sends from, so bytes keep their
  order whoever wrote them (the window's packets and the network thread's
  keep-alives). Horizon's servers (dedicated, and a world opened to LAN) take
  WebSockets beside Minecraft's TCP on the same port: a connection starting
  with `GET ` is upgraded (`horizon-protocol/src/websocket.rs`), then carries
  the same bytes in binary messages. Join from *Multiplayer* or with
  `?args=--server%20host:port`. Without threads (`THREADS=0`) there is no
  network worker, so no multiplayer.
- **Eaglercraft servers**: an address that is a `ws://` or `wss://` URL
  (`servers::websocket_url`) is an EaglerXServer plugin in front of a proxy,
  whose ViaVersion takes 26.3's protocol (777). The socket then goes a message
  at a time, and `horizon-protocol/src/eagler.rs` stands between it and the
  connection's byte stream. It reads the client's own handshake and login
  start, runs the plugin's login (Eagler protocol 5, which lifts the default
  1.12 cap on Minecraft protocols: client version → server version → request
  login → allow login → finish login, no skin sent), and answers with the
  login-finished packet; it drops the client's login acknowledgement, which the
  plugin sends itself. After that each game packet is one binary message
  without its length (never compressed or encrypted), and the plugin's own
  messages (`0xEE` first) are skipped. A refusal (version mismatch, denied
  login, redirect, error) becomes the login disconnect, so the client shows
  the reason. The server list's status is the plugin's `Accept: motd` text
  query, turned into a vanilla status. Checked against
  `wss://eagler.carrot-craft.org` (EaglercraftXServer 1.1.1 on Velocity):
  login, configuration, play and keep-alives.
- **Chunk work** (`streaming.rs`): chunk workers are Web Workers. Without
  threads (`THREADS=0`), the frame runs queued generation, lighting and meshing
  jobs itself for up to 12 ms per update.
- **Shaders**: browsers enforce WGSL rules that native wgpu doesn't
  (derivative uniformity, parentheses between `<<` and `|`); `hud.wgsl` and the
  entity shader turn the uniformity check off, which changes nothing they draw.

## What's missing

- **Other page-side locks.** Only the locks seen trapping use
  `horizon_os::sync::lock`; any other lock the page shares with a worker can
  still stop the game.
- **Screenshots and world icons**: reading a frame back needs the page to wait
  for the GPU. They report an error on the web; the world list shows no icon.
- **Moving worlds in and out**: no export or import of a world folder (a
  `.zip`) yet; a world stays in the browser that made it.
- **Online-mode servers**: Microsoft sign-in needs a backend, so the page
  joins offline-mode servers only. Java servers with neither Horizon nor the
  Eaglercraft plugin take no WebSockets: they need a proxy in front (none
  written yet).
- **Eaglercraft extras**: no skin or cape is sent (the plugin gives the
  default ones), the server list shows no icon (the plugin's 64×64 RGBA icon
  isn't read), Eagler passwords (`authEnabled`) and its cookies aren't
  offered, and the plugin's messages (skins, voice, its pause menu) are
  dropped. Servers whose plugin only offers Eagler 3/4, or caps
  `max_minecraft_protocol_v5` under 777, refuse the version. The native
  client can't join a `ws:` address.
- **Secure pages and plain servers**: an `https:` page may only open `wss:`,
  so a page served over HTTPS reaches a server through a TLS proxy (Caddy,
  nginx) in front of its port.
- **Sounds kept in memory**: every sound file fetched stays in the virtual
  filesystem for the session, as do the decoded samples' 64 MB cache.
- **Controller output**: rumble through gilrs' force feedback has no thread
  on the web, and the DualSense's own output needs HID.
- **Mods** that run WebAssembly through wasmtime can't run inside the page.

# Horizon in the browser

An experimental build of the Horizon client for the web: the same game code
compiled to WebAssembly, drawing through the browser's WebGPU. It creates and
opens single-player worlds; they live in memory and are gone on reload (see
[What's missing](#whats-missing)).

## Build and run

You need the prepared resources in `.local/horizon-client/26.3` (see the main
README's *Build from source*), and once:

```sh
rustup target add wasm32-unknown-unknown
rustup toolchain install nightly-2026-10-14 --component rust-src   # threads: std rebuilt with atomics
cargo install wasm-bindgen-cli --version 0.2.129   # the version in Cargo.lock
```

Then:

```sh
web/build.sh        # the game to web/dist/pkg, the resources to web/dist/fs
python3 web/serve.py
```

and open <http://localhost:8080> in a browser with WebGPU. Chrome and Edge have it
on Windows, macOS and ChromeOS. On Linux, Chromium-based browsers need it switched
on: `brave --enable-unsafe-webgpu --enable-features=Vulkan http://localhost:8080`
(or the same flags for `chromium` / `google-chrome`).

`PROFILE=release web/build.sh` builds with LTO (slower to build, smaller and faster).
`THREADS=0 web/build.sh` builds with stable Rust and no threads.

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
- **Startup** (`web/main.js`): load the WebAssembly, download and install the
  archive, then `horizon_start()`. The first renderer opens asynchronously
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
- **Pointer lock**: browsers lock the pointer only during a click or a key
  press, and winit's request throws through the frame when refused. The game
  asks through `horizon_os::web::pointer_lock`, and the canvas takes the lock
  on the player's next click.
- **Chunk work** (`streaming.rs`): chunk workers are Web Workers. Without
  threads (`THREADS=0`), the frame runs queued generation, lighting and meshing
  jobs itself for up to 12 ms per update.
- **Shaders**: browsers enforce WGSL rules that native wgpu doesn't
  (derivative uniformity, parentheses between `<<` and `|`); `hud.wgsl` and the
  entity shader turn the uniformity check off, which changes nothing they draw.

## What's missing

- **Untested in worlds**: mouse look under pointer lock (headless browsers
  can't lock it), pausing, Save and Quit to Title, superflat.
- **Other page-side locks.** Only the locks seen trapping use
  `horizon_os::sync::lock`; any other lock the page shares with a worker can
  still stop the game.
- **Screenshots and world icons**: reading a frame back needs the page to wait
  for the GPU. They report an error on the web; the world list shows no icon.
- **Saving.** Files the game writes live in memory and are gone on reload
  (options, worlds). They belong in the browser's private file storage (OPFS).
- **Network.** Browsers have no raw sockets: multiplayer needs a WebSocket
  endpoint on the server or a proxy, and Microsoft sign-in needs a backend.
- **Sound.** cpal's Web Audio backend is not enabled yet.
- **Mods** that run WebAssembly through wasmtime can't run inside the page.

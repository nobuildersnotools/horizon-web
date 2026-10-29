# Horizon in the browser

An experimental build of the Horizon client for the web: the same game code
compiled to WebAssembly, drawing through the browser's WebGPU. Today it boots to
the title screen; worlds don't open yet (see [What's missing](#whats-missing)).

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
- **Chunk work** (`streaming.rs`): chunk workers are Web Workers. Without
  threads (`THREADS=0`), the frame runs queued generation, lighting and meshing
  jobs itself for up to 12 ms per update.
- **Shaders**: browsers enforce WGSL rules that native wgpu doesn't
  (derivative uniformity, parentheses between `<<` and `|`); `hud.wgsl` and the
  entity shader turn the uniformity check off, which changes nothing they draw.

## What's missing

- **Worlds.** Opening one starts the server thread, its local connection and
  the client's network thread. They can start as workers now, but the window
  still waits on them in places (`Integrated::call`'s `recv_timeout`, joins
  on quit), and the network code still uses std's `recv_timeout`. The world
  rest loader and the startup loader still run inline on the page.
- **Locks the page shares with workers.** std's `Mutex` spins briefly, then
  waits. A long hold by a worker (the filesystem, the chunk queue) would trap
  the page.
- **Saving.** Files the game writes live in memory and are gone on reload
  (options, worlds). They belong in the browser's private file storage (OPFS).
- **Network.** Browsers have no raw sockets: multiplayer needs a WebSocket
  endpoint on the server or a proxy, and Microsoft sign-in needs a backend.
- **Sound.** cpal's Web Audio backend is not enabled yet.
- **Mods** that run WebAssembly through wasmtime can't run inside the page.

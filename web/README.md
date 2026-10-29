# Horizon in the browser

An experimental build of the Horizon client for the web: the same game code
compiled to WebAssembly, drawing through the browser's WebGPU. Today it boots to
the title screen; worlds don't open yet (see [What's missing](#whats-missing)).

## Build and run

You need the prepared resources in `.local/horizon-client/26.3` (see the main
README's *Build from source*), and once:

```sh
rustup target add wasm32-unknown-unknown
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

## How it fits together

- **`horizon-os`** is what the game sees of the operating system: `fs`, `time`,
  `thread`, `env` and `process`. On native they are std's own; in the browser,
  `fs` is a virtual filesystem in memory, `time` is the browser's clock, and
  scoped threads run one after another. The game's crates call `horizon_os::fs`
  where they called `std::fs`, and `path.fs_exists()` (`PathExt`) where they
  called `path.exists()`.
- **The resources** go to the page as one archive: `web/bundle.py` packs
  everything the game reads to reach the title screen and open a world into
  `boot.tar.gz` (about 15 MB). The rest (other languages, the world generator's
  bundle) is listed in `manifest.txt` and fetched the first time it is read.
- **Startup** (`web/main.js`): load the WebAssembly, download and install the
  archive, then `horizon_start()`. The first renderer opens asynchronously
  (`Loading::resumed`), because the page can't wait for the GPU.
- **Chunk work** (`streaming.rs`): without threads, the frame runs queued
  generation, lighting and meshing jobs itself for up to 12 ms per update.
- **Shaders**: browsers enforce WGSL rules that native wgpu doesn't
  (derivative uniformity, parentheses between `<<` and `|`); `hud.wgsl` and the
  entity shader turn the uniformity check off, which changes nothing they draw.

## What's missing

- **Threads.** The page runs on one thread. Real worker threads need Web Workers
  with shared memory (a nightly `+atomics` build and the cross-origin isolation
  headers `serve.py` already sends). Until then, the singleplayer server, chunk
  workers, occlusion search, log writer and others either run inline or are off.
- **Saving.** Files the game writes live in memory and are gone on reload
  (options, worlds). They belong in the browser's private file storage (OPFS).
- **Network.** Browsers have no raw sockets: multiplayer needs a WebSocket
  endpoint on the server or a proxy, and Microsoft sign-in needs a backend.
- **Sound.** cpal's Web Audio backend is not enabled yet.
- **Mods** that run WebAssembly through wasmtime can't run inside the page.

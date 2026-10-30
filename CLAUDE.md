@AGENTS.md

# This branch: Horizon in the browser

Branch `web` ports the client to the browser: `horizon-client` compiled for `wasm32-unknown-unknown`,
drawing through WebGPU, with threads as Web Workers. It boots to the title screen, opens single-player worlds
and joins offline-mode Horizon servers over a WebSocket, with sound and controllers (options and saves are kept in the browser's OPFS; a service worker keeps the large
files). Design and status:
[web/README.md](web/README.md). Everything above still applies; this section adds what the port needs.

The port's roadmap (phases, gates, what's next) is the Claude doc
[Horizon Web Port Roadmap](https://claude.ai/artifact/K226zHbJth7LrVbYptsazz): read it with the docs
tools, and tick its items and add a result line when a phase moves. `ROADMAP.md` is the whole project's
roadmap, not the port's.

## Rules that bite

- **Native behaviour must not change.** Every web difference sits behind
  `#[cfg(target_arch = "wasm32")]` (or `cfg!`), or in `horizon-os`, which is std's own on native. Upstream
  merges stay small: don't reformat or restructure code beyond what the port needs.
- **Go through `horizon_os`, not std**, in the client's crates (std's versions panic or fail in the browser):

  | Instead of | Use |
  |---|---|
  | `std::fs::*` | `horizon_os::fs::*` |
  | `path.exists()`, `.is_dir()`, `.is_file()`, `.metadata()`, `.canonicalize()` on a `Path` | `path.fs_exists()`... (`use horizon_os::fs::PathExt;`) |
  | `std::time::{Instant, SystemTime, UNIX_EPOCH}` | `horizon_os::time::*`; `horizon_os::time::to_std` for chrono |
  | `std::thread::*` | `horizon_os::thread::*` |
  | `rx.recv_timeout(t)`, `condvar.wait_timeout_while(g, t, f)` | `horizon_os::sync::recv_timeout(&rx, t)`, `horizon_os::sync::wait_timeout_while(&condvar, g, t, f)` (std's read std's clock, which panics on the web; plain `wait_timeout`, `park_timeout` and `sleep` are fine) |
  | `std::process::id()` | `horizon_os::process::id()` |
  | `std::env::{args, current_dir, current_exe, temp_dir}` | `horizon_os::env::*` (the page's `?args=` on the web) |

  `Metadata`/`DirEntry`/`FileType` keep std's own `.is_dir()`, `.metadata()`... (they aren't paths).
  `horizon-ctl` (`src/bin/horizon-ctl/`) is native-only and keeps std.
- **The page's thread never blocks.** No `pollster::block_on` on something that waits for the browser
  (use `horizon_os::block_on`, which only takes a ready future on the web), no `Condvar` waits,
  `recv()` on an empty channel or `thread::sleep` loops: browsers trap a wait on the page's thread. Workers
  may block (`horizon_os::workers::in_worker()` tells them apart).
- **A lock the page shares with workers goes through `horizon_os::sync::lock` / `write`**, which spin on the
  page. std's `Mutex` waits once contended, the browser traps it (`Atomics.wait cannot be called in this
  context`), winit swallows the error and the frame loop stops with no message. A world that freezes silently:
  find the trap with CDP `Debugger.setPauseOnExceptions` (`state: "all"`, armed after startup; skip winit's
  `AbortError`s). A reply the page needs from a running worker: `horizon_os::sync::recv_reply`.
- **Threads are Web Workers** in the default (shared-memory) build: `horizon_os::thread::spawn` and
  `Builder::spawn` start one ([crates/horizon-os/src/workers.rs](crates/horizon-os/src/workers.rs)). A worker
  starts only once the page's thread returns to the browser, so the page must never wait for one it just
  spawned (`join` on the page spins; scoped threads still run in sequence). `THREADS=0 web/build.sh` builds
  without threads, where spawns fail ("no thread on the page for …"): keep the paths that work without them,
  as `startup_menu::spawn_loader`, the world rest loader in `game.rs` and `Streaming`'s `pump` do.
- **Shaders must pass browsers' WGSL rules**, which naga doesn't enforce: derivative uniformity (sample
  textures in uniform control flow, or start the module with `diagnostic(off, derivative_uniformity);`
  as `hud.wgsl` and `entity_shader()` do) and parentheses when mixing `<<`/`>>` with `|`/`&`/`^`.
- **On the web, `run_app` never returns.** Errors that native reports after the event loop are logged in
  `App::exiting` instead.
- Never commit `web/dist/` (it holds the prepared resources, Mojang's files). It is gitignored.

## Build and check

```sh
web/build.sh                       # the browser build into web/dist (release: LTO + wasm-opt; PROFILE=fast to iterate; THREADS=0 for stable Rust)
python3 web/serve.py               # http://localhost:8080, cross-origin isolated
cargo check --target wasm32-unknown-unknown -p horizon-client --bin horizon-client --no-default-features --features audio,controllers
cargo clippy --target wasm32-unknown-unknown -p horizon-client --bin horizon-client --no-default-features --features audio,controllers -- -D warnings
# the threaded build's own check (the flags web/build.sh sets)
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS='--cfg getrandom_backend="wasm_js" -C target-feature=+atomics,+bulk-memory,+mutable-globals' \
  cargo +nightly-2026-10-14 -Z build-std=std,panic_abort clippy --target wasm32-unknown-unknown -p horizon-client --bin horizon-client --no-default-features --features audio,controllers -- -D warnings
cargo clippy --workspace --all-targets -- -D warnings      # native must stay clean too
```

- The web build keeps `controllers` (gilrs reads the Gamepad API); `horizon-dualsense` builds without
  hidapi there, as on iOS. `.cargo/config.toml` sets getrandom's
  `wasm_js` backend for wasm only, so don't export it in `RUSTFLAGS` (it breaks native builds).
- The threaded build needs the pinned nightly with `rust-src` (`rustup component add rust-src --toolchain
  nightly-2026-10-14`): std is rebuilt with atomics, and the module imports a shared memory the page hands
  each worker (`web/worker.js`).
- After a change, check that **both** the native client and the page reach the title screen
  (`Horizon: first frame after … ms` in the log or the browser console).
- `web/bundle.py` decides what is downloaded up front (`boot.tar.gz`) and what is fetched when read
  (`manifest.txt`). A file read at startup that is only in the manifest costs a blocking request.

## Testing in a browser

- Needs WebGPU. Chromium-based browsers on Linux need it switched on: `--enable-unsafe-webgpu
  --enable-features=Vulkan`. Headless works (`--headless=new --remote-debugging-port=…` with a scratch
  `--user-data-dir`) and can be driven over the DevTools protocol; read WebGPU validation errors from its
  `Log` domain, they aren't console messages.
- An adapter-less browser stops at `No available adapters`: that's the browser, not the game.
- The game logs to the browser console; a panic logs `Horizon panicked: …` with the Rust message. A worker's
  console is its own DevTools target: attach to it (`Target.setAutoAttach` with `flatten`) to see its logs.
  A panic aborts only that worker (`RuntimeError: unreachable`); its `join` then never returns.

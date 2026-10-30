#!/usr/bin/env bash
# Builds Horizon for the browser into web/dist: the game as WebAssembly, the
# page, and the prepared resources packed for download.
#
#   web/build.sh            optimized, no LTO (the `fast` profile)
#   PROFILE=release web/build.sh
#   THREADS=0 web/build.sh  stable Rust, no threads (everything on the page's)
#
# Threads are Web Workers on shared memory, which std only supports when it is
# rebuilt with atomics: that build needs the pinned nightly below with its
# rust-src (rustup component add rust-src --toolchain <it>).
# Needs: rustup target add wasm32-unknown-unknown, and the wasm-bindgen CLI of
# the version in Cargo.lock (cargo install wasm-bindgen-cli --version <it>).
set -euo pipefail
cd "$(dirname "$0")/.."

profile="${PROFILE:-fast}"
resources="${HORIZON_RESOURCES:-.local/horizon-client/26.3}"
dist=web/dist

nightly=nightly-2026-10-14
cargo=(cargo)
if [ "${THREADS:-1}" != 0 ]; then
    # Replaces .cargo/config.toml's flags for the target, so it repeats them.
    # The linker flags give the module a shared memory the page hands each
    # worker, and the thread-local storage symbols wasm-bindgen sets up per thread.
    flags=(
        '--cfg getrandom_backend="wasm_js"'
        -C target-feature=+atomics,+bulk-memory,+mutable-globals
        -C link-arg=--shared-memory -C link-arg=--import-memory
        -C link-arg=--max-memory=4294967296
        -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size
        -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base
    )
    export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="${flags[*]}"
    cargo=(cargo "+$nightly" -Z build-std=std,panic_abort)
fi
"${cargo[@]}" build --target wasm32-unknown-unknown --profile "$profile" \
    -p horizon-client --bin horizon-client --no-default-features --features audio

target_dir="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
mkdir -p "$dist/pkg"
wasm-bindgen --target web --no-typescript --out-dir "$dist/pkg" \
    "$target_dir/wasm32-unknown-unknown/$profile/horizon-client.wasm"
cp web/index.html web/main.js web/audio.js web/worker.js "$dist/"

if [ ! -f "$dist/fs/boot.tar.gz" ] || [ "$resources/horizon-resources.json" -nt "$dist/fs/boot.tar.gz" ]; then
    python3 web/bundle.py "$resources" "$dist/fs"
fi
ls -l "$dist/pkg"/*.wasm
echo "Serve it with: python3 web/serve.py, then open http://localhost:8080"

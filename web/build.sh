#!/usr/bin/env bash
# Builds Horizon for the browser into web/dist: the game as WebAssembly, the
# page, and the prepared resources packed for download.
#
#   web/build.sh            optimized, no LTO (the `fast` profile)
#   PROFILE=release web/build.sh
#
# Needs: rustup target add wasm32-unknown-unknown, and the wasm-bindgen CLI of
# the version in Cargo.lock (cargo install wasm-bindgen-cli --version <it>).
set -euo pipefail
cd "$(dirname "$0")/.."

profile="${PROFILE:-fast}"
resources="${HORIZON_RESOURCES:-.local/horizon-client/26.3}"
dist=web/dist

cargo build --target wasm32-unknown-unknown --profile "$profile" \
    -p horizon-client --bin horizon-client --no-default-features --features audio

target_dir="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
mkdir -p "$dist/pkg"
wasm-bindgen --target web --no-typescript --out-dir "$dist/pkg" \
    "$target_dir/wasm32-unknown-unknown/$profile/horizon-client.wasm"
cp web/index.html web/main.js "$dist/"

if [ ! -f "$dist/fs/boot.tar.gz" ] || [ "$resources/horizon-resources.json" -nt "$dist/fs/boot.tar.gz" ]; then
    python3 web/bundle.py "$resources" "$dist/fs"
fi
ls -l "$dist/pkg"/*.wasm
echo "Serve it with: python3 web/serve.py, then open http://localhost:8080"

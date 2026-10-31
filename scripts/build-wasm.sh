#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export CARGO_PROFILE_RELEASE_LTO=false
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
export CARGO_PROFILE_RELEASE_OPT_LEVEL=2
export RUSTFLAGS="${RUSTFLAGS:-} --cfg getrandom_backend=\"wasm_js\""
cargo rustc --locked -p entwurf-wasm --target wasm32-unknown-unknown --release -- -C strip=debuginfo
wasm_input="${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/entwurf_wasm.wasm"
wasm_bindgen_bin="${WASM_BINDGEN:-wasm-bindgen}"
"$wasm_bindgen_bin" "$wasm_input" --target web --out-dir apps/desktop/public/wasm --out-name entwurf_wasm
if [[ "${1:-}" == "--test" ]]; then
  vfs_test_binary=$(mktemp "${TMPDIR:-/tmp}/entwurf-vfs-tests.XXXXXX")
  trap 'rm -f -- "$vfs_test_binary"' EXIT
  rustc --edition=2021 --test --cfg 'feature="browser"' crates/entwurf-core/src/files.rs -o "$vfs_test_binary"
  "$vfs_test_binary"
  "$wasm_bindgen_bin" "$wasm_input" --target nodejs --out-dir crates/entwurf-wasm/pkg-node --out-name entwurf_wasm
  node crates/entwurf-wasm/tests/smoke.cjs
  node crates/entwurf-wasm/tests/blank-pdf.cjs
  node crates/entwurf-wasm/tests/generated-images.cjs
fi

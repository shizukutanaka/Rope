#!/usr/bin/env bash
# clippy を crates.io 無しで走らせる。
#
#   tools/offline-typecheck/lint.sh
#
# `clippy-driver` は rustup の toolchain に同梱されていて **registry を必要と
# しない**。`cargo clippy` が使えなくても、rustc と同じ引数で直接叩けば
# `src/` 全体に lint を掛けられる。
#
# ## 🔴 CI の clippy とは**バージョンが違う**
#
# ここの clippy は toolchain 同梱の最新版 (1.94 系)。
# CI は `.github/ci.yml.disabled` で **1.75.0 に固定**されている。
#
# つまり:
# - **ここで出る lint が CI では出ないことがある** (新しい lint)
# - **その提案に従うと MSRV 1.75 を壊すことがある**
#
# 実例 (2026-08-18): `clippy::manual_is_multiple_of` が
# `net/inference.rs` の 3 箇所で `x % y != 0` を `!x.is_multiple_of(y)` に
# 直せと言うが、**`is_multiple_of` は Rust 1.87 で安定化**した API であり、
# **1.75 ではコンパイルできない**。CI の clippy 1.75 にはこの lint 自体が
# 無いので、**直さないのが正しい**。
#
# → **提案を機械的に適用しないこと。** 1.75 に存在する API かを必ず確認する。
#
# 既定では「新しすぎて CI に無い」lint を抑止する。全部見たい場合は
# `ROPE_LINT_ALL=1` を付ける。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${ROPE_LINT_OUT:-$ROOT/tools/offline-typecheck/.build-lint}"
EDITION=2021

rm -rf "$OUT"
mkdir -p "$OUT"

# shim を毎回ビルドし直す (古いものを使うと嘘の結果になる)
SHIM="$OUT/shims"
ROPE_TYPECHECK_OUT="$SHIM" "$ROOT/tools/offline-typecheck/check.sh" >/dev/null 2>&1 || {
    echo "❌ shim のビルド (= 型検査) に失敗。check.sh を直接実行して原因を見ること"
    exit 1
}

CARGO_PKG_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
CARGO_PKG_NAME="rope"
export CARGO_PKG_VERSION CARGO_PKG_NAME

LIB_EXTERNS=(
    --extern serde="$SHIM/libserde.rlib"
    --extern serde_derive_shim="$SHIM/libserde_derive_shim.so"
    --extern serde_json="$SHIM/libserde_json.rlib"
    --extern chrono="$SHIM/libchrono.rlib"
    --extern uuid="$SHIM/libuuid.rlib"
    --extern anyhow="$SHIM/libanyhow.rlib"
    --extern blake3="$SHIM/libblake3.rlib"
    --extern hex="$SHIM/libhex.rlib"
    --extern dirs="$SHIM/libdirs.rlib"
    --extern tracing="$SHIM/libtracing.rlib"
    --extern base64="$SHIM/libbase64.rlib"
    --extern rand="$SHIM/librand.rlib"
    --extern ed25519_dalek="$SHIM/libed25519_dalek.rlib"
)

# CI (clippy 1.75) に存在しない、新しすぎる lint。
# ここで騒いでも CI では出ず、従うと MSRV を壊す。
NEW_LINTS=(-A clippy::manual_is_multiple_of)
if [ "${ROPE_LINT_ALL:-0}" = "1" ]; then
    NEW_LINTS=()
    echo "ℹ️  ROPE_LINT_ALL=1 — CI に無い新しい lint も表示します"
fi

run_lint() {
    local label="$1"
    shift
    echo "── $label ──"
    local log="$OUT/${label// /_}.log"
    clippy-driver --edition "$EDITION" "${NEW_LINTS[@]}" "$@" >"$log" 2>&1
    local rc=$?
    if grep -qE "^(warning|error)" "$log"; then
        grep -E "^(warning|error)" -A 6 "$log" | head -60
    fi
    if [ "$rc" -ne 0 ]; then
        echo "  ❌ clippy が失敗"
        return 1
    fi
    local n
    n=$(grep -cE "^warning: " "$log" || true)
    # 「N warnings emitted」の行も数に入るので 1 引く
    if [ "$n" -gt 0 ]; then
        echo "  ⚠️  警告あり (CI は -D warnings なので落ちる)"
        return 1
    fi
    echo "  OK"
    return 0
}

status=0

run_lint "lib" --crate-type lib --crate-name rope --emit=metadata \
    "$ROOT/src/lib.rs" "${LIB_EXTERNS[@]}" -L "$SHIM" --out-dir "$OUT" || status=1

run_lint "lib tests" --test --crate-name rope_lib_lint --emit=metadata \
    "$ROOT/src/lib.rs" "${LIB_EXTERNS[@]}" -L "$SHIM" --out-dir "$OUT" || status=1

run_lint "bin" --crate-type bin --crate-name rope_bin --emit=metadata \
    "$ROOT/src/main.rs" \
    --extern rope="$SHIM/librope.rlib" \
    --extern clap="$SHIM/libclap.rlib" \
    --extern clap_derive_shim="$SHIM/libclap_derive_shim.so" \
    "${LIB_EXTERNS[@]}" -L "$SHIM" --out-dir "$OUT" || status=1

run_lint "bin tests" --test --crate-name rope_bin_lint --emit=metadata \
    "$ROOT/src/main.rs" \
    --extern rope="$SHIM/librope.rlib" \
    --extern clap="$SHIM/libclap.rlib" \
    --extern clap_derive_shim="$SHIM/libclap_derive_shim.so" \
    "${LIB_EXTERNS[@]}" -L "$SHIM" --out-dir "$OUT" || status=1

echo
if [ "$status" -eq 0 ]; then
    echo "✅ clippy 警告ゼロ (この toolchain の clippy $(clippy-driver --version | awk '{print $2}'))"
    echo "   ⚠️  CI は clippy 1.75。バージョン差で結果が違いうる (冒頭の注意を参照)。"
else
    echo "❌ clippy に指摘あり — CI は -D warnings なので直すこと"
    echo "   ただし**提案が MSRV 1.75 で使えない API かどうかを必ず確認**すること。"
fi
exit "$status"

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
# CI は `.github/ci.yml.disabled` で **`Cargo.toml` の `rust-version` と
# 同じ版に固定**されている。
#
# つまり:
# - **ここで出る lint が CI では出ないことがある** (新しい lint)
# - **その提案に従うと MSRV を壊すことがある**
#
# 実例 (2026-08-18): `clippy::manual_is_multiple_of` が
# `net/inference.rs` の 3 箇所で `x % y != 0` を `!x.is_multiple_of(y)` に
# 直せと言うが、**`is_multiple_of` は Rust 1.87 で安定化**した API であり、
# **MSRV ではコンパイルできない**。CI の古い clippy にはこの lint 自体が
# 無いので、**直さないのが正しい**。
#
# → **提案を機械的に適用しないこと。** MSRV に存在する API かを必ず確認する。
#
# 既定では「新しすぎて CI に無い」lint を抑止する。全部見たい場合は
# `ROPE_LINT_ALL=1` を付ける。
#
# ## ✅ MSRV も**ここで検証できる**
#
# `rustup` は古い toolchain を取得できない (static.rust-lang.org 到達不能) が、
# **clippy の `incompatible_msrv` は toolchain を必要としない** — 各 API の
# 安定化バージョンを内部表から引いて、`clippy.toml` の `msrv` と比べるだけ。
# リポジトリ直下の `clippy.toml` に `msrv` を置いてある
# (`Cargo.toml` の `rust-version` と一致させること)。
#
# これで「MSRV でコンパイルできるか」を、その toolchain を持たずに検査できる。
# `selftest.sh` が 1.87 の API を注入して**検出力を毎回実証する**。

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
# clippy.toml (msrv) の置き場所。cargo 経由でないので明示的に教える。
CLIPPY_CONF_DIR="${ROPE_CLIPPY_CONF_DIR:-$ROOT}"
export CARGO_PKG_VERSION CARGO_PKG_NAME CLIPPY_CONF_DIR

# Cargo.toml の rust-version と clippy.toml の msrv がずれていたら、
# MSRV 検査は嘘をつく。ここで一致を確かめる。
declared_msrv="$(sed -n 's/^rust-version = "\([^"]*\)".*/\1/p' "$ROOT/Cargo.toml" | head -1)"
conf_msrv="$(sed -n 's/^msrv = "\([^"]*\)".*/\1/p' "$CLIPPY_CONF_DIR/clippy.toml" 2>/dev/null | head -1)"
if [ -z "$conf_msrv" ]; then
    echo "❌ $CLIPPY_CONF_DIR/clippy.toml に msrv がありません — MSRV 検査が無効になります"
    exit 1
fi
case "$conf_msrv" in
    "$declared_msrv" | "$declared_msrv".*) ;;
    *)
        echo "❌ MSRV 不一致: Cargo.toml=$declared_msrv / clippy.toml=$conf_msrv"
        exit 1
        ;;
esac
echo "ℹ️  MSRV $conf_msrv を検査対象にします (toolchain は不要)"

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
    --extern reqwest="$SHIM/libreqwest.rlib"
    --extern tokio="$SHIM/libtokio.rlib"
)

# CI の古い clippy に存在しない、新しすぎる lint。
# ここで騒いでも CI では出ず、従うと MSRV を壊す。
# `incompatible_msrv` を有効化。`manual_is_multiple_of` は逆に抑止する —
# **その提案に従うと MSRV 違反になる**ため (両者は同じコードを指す)。
NEW_LINTS=(-W clippy::incompatible_msrv -A clippy::manual_is_multiple_of)
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

# CI は `clippy --all-targets --features http` も回す
run_lint "lib (http)" --cfg 'feature="http"' --crate-type lib --crate-name rope_http \
    --emit=metadata "$ROOT/src/lib.rs" "${LIB_EXTERNS[@]}" -L "$SHIM" --out-dir "$OUT" || status=1

run_lint "lib tests (http)" --cfg 'feature="http"' --test --crate-name rope_http_lint \
    --emit=metadata "$ROOT/src/lib.rs" "${LIB_EXTERNS[@]}" -L "$SHIM" --out-dir "$OUT" || status=1

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
    echo "✅ clippy 警告ゼロ + MSRV $conf_msrv 適合 (clippy $(clippy-driver --version | awk '{print $2}'))"
    echo "   ⚠️  CI の clippy は $declared_msrv 同梱版。バージョン差で結果が違いうる。"
    echo "       MSRV 検査は API の安定化バージョン表に基づくもので、"
    echo "       $declared_msrv で実際にビルドしたわけではない。"
else
    echo "❌ clippy に指摘あり — CI は -D warnings なので直すこと"
    echo "   ただし**提案が MSRV $declared_msrv で使えない API かどうかを必ず確認**すること。"
fi
exit "$status"

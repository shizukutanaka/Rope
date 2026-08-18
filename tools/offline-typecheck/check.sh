#!/usr/bin/env bash
# Rope オフライン型検査ハーネス
#
# crates.io に到達できない環境 (docs/SURPLUS_AND_GAPS.md §0) で、
# `src/lib.rs` (core/ + net/) を **rustc に通す**。依存 crate は
# `shims/` の型検査専用スタブで置き換える。
#
#   使い方:  tools/offline-typecheck/check.sh
#   終了コード: 0 = 型検査 PASS / 非 0 = FAIL
#
# ⚠️ これは `cargo check` の代用であって `cargo test` の代用ではない。
#    何を検出でき、何を検出できないかは README.md を必ず読むこと。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HERE="$ROOT/tools/offline-typecheck"
SHIMS="$HERE/shims"
OUT="${ROPE_TYPECHECK_OUT:-$HERE/.build}"
EDITION=2021

# `env!("CARGO_PKG_VERSION")` (net/cashu_mint.rs) は cargo が渡す環境変数を
# コンパイル時に読む。cargo を経由しないので Cargo.toml から自前で渡す。
CARGO_PKG_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
CARGO_PKG_NAME="rope"
export CARGO_PKG_VERSION CARGO_PKG_NAME

rm -rf "$OUT"
mkdir -p "$OUT"

fail() {
    echo "❌ $1"
    exit 1
}

echo "── shim をビルド ──"

# proc-macro が最初 (serde が re-export する)
rustc --edition "$EDITION" --crate-type proc-macro \
    --crate-name serde_derive_shim "$SHIMS/serde_derive_shim.rs" \
    -L "$OUT" --out-dir "$OUT" 2>&1 | sed 's/^/  /'
[ -f "$OUT/libserde_derive_shim.so" ] || fail "serde_derive_shim のビルド失敗"

rustc --edition "$EDITION" --crate-type lib --crate-name serde "$SHIMS/serde.rs" \
    --extern serde_derive_shim="$OUT/libserde_derive_shim.so" \
    -L "$OUT" --out-dir "$OUT" 2>&1 | sed 's/^/  /'
[ -f "$OUT/libserde.rlib" ] || fail "serde shim のビルド失敗"

# serde に依存する shim
for c in chrono uuid serde_json; do
    rustc --edition "$EDITION" --crate-type lib --crate-name "$c" "$SHIMS/$c.rs" \
        --extern serde="$OUT/libserde.rlib" \
        --extern serde_derive_shim="$OUT/libserde_derive_shim.so" \
        -L "$OUT" --out-dir "$OUT" 2>&1 | sed 's/^/  /'
    [ -f "$OUT/lib$c.rlib" ] || fail "$c shim のビルド失敗"
done

rustc --edition "$EDITION" --crate-type proc-macro \
    --crate-name clap_derive_shim "$SHIMS/clap_derive_shim.rs" \
    -L "$OUT" --out-dir "$OUT" 2>&1 | sed 's/^/  /'
[ -f "$OUT/libclap_derive_shim.so" ] || fail "clap_derive_shim のビルド失敗"

rustc --edition "$EDITION" --crate-type lib --crate-name clap "$SHIMS/clap.rs" \
    --extern clap_derive_shim="$OUT/libclap_derive_shim.so" \
    -L "$OUT" --out-dir "$OUT" 2>&1 | sed 's/^/  /'
[ -f "$OUT/libclap.rlib" ] || fail "clap shim のビルド失敗"

# 独立した shim
for c in anyhow blake3 hex dirs tracing base64 rand ed25519_dalek; do
    rustc --edition "$EDITION" --crate-type lib --crate-name "$c" "$SHIMS/$c.rs" \
        -L "$OUT" --out-dir "$OUT" 2>&1 | sed 's/^/  /'
    [ -f "$OUT/lib$c.rlib" ] || fail "$c shim のビルド失敗"
done

EXTERNS=(
    --extern serde="$OUT/libserde.rlib"
    --extern serde_derive_shim="$OUT/libserde_derive_shim.so"
    --extern serde_json="$OUT/libserde_json.rlib"
    --extern chrono="$OUT/libchrono.rlib"
    --extern uuid="$OUT/libuuid.rlib"
    --extern anyhow="$OUT/libanyhow.rlib"
    --extern blake3="$OUT/libblake3.rlib"
    --extern hex="$OUT/libhex.rlib"
    --extern dirs="$OUT/libdirs.rlib"
    --extern tracing="$OUT/libtracing.rlib"
    --extern base64="$OUT/libbase64.rlib"
    --extern rand="$OUT/librand.rlib"
    --extern ed25519_dalek="$OUT/libed25519_dalek.rlib"
)

status=0

echo
echo "── src/lib.rs を型検査 (core/ + net/、http feature 無し) ──"
rustc --edition "$EDITION" --crate-type lib --crate-name rope \
    "$ROOT/src/lib.rs" "${EXTERNS[@]}" \
    -L "$OUT" --out-dir "$OUT" || status=1

echo
echo "── src/lib.rs のテストコードも型検査 (--test) ──"
# --test はテスト関数本体も型検査対象に含める。実行はしない
# (shim のデシリアライズは unimplemented! のため実行はできない)。
rustc --edition "$EDITION" --test --crate-name rope_tests \
    --emit=metadata "$ROOT/src/lib.rs" "${EXTERNS[@]}" \
    -L "$OUT" --out-dir "$OUT" || status=1

echo
echo "── src/main.rs を型検査 (4 動詞の CLI 層) ──"
rustc --edition "$EDITION" --crate-type bin --crate-name rope_bin \
    --emit=metadata "$ROOT/src/main.rs" \
    --extern rope="$OUT/librope.rlib" --extern clap="$OUT/libclap.rlib" \
    --extern clap_derive_shim="$OUT/libclap_derive_shim.so" \
    --extern anyhow="$OUT/libanyhow.rlib" \
    -L "$OUT" --out-dir "$OUT" || status=1

echo
echo "── src/main.rs のテストコードも型検査 (--test) ──"
rustc --edition "$EDITION" --test --crate-name rope_bin_tests \
    --emit=metadata "$ROOT/src/main.rs" \
    --extern rope="$OUT/librope.rlib" --extern clap="$OUT/libclap.rlib" \
    --extern clap_derive_shim="$OUT/libclap_derive_shim.so" \
    --extern anyhow="$OUT/libanyhow.rlib" \
    -L "$OUT" --out-dir "$OUT" || status=1

echo
if [ "$status" -eq 0 ]; then
    echo "✅ オフライン型検査 PASS"
    echo "   ⚠️  これは型検査のみ。実 crate との差分・実行時挙動・暗号的性質は"
    echo "       一切検証していない (README.md「検出できないもの」を参照)。"
else
    echo "❌ オフライン型検査 FAIL"
fi
exit "$status"

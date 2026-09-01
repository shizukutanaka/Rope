#!/usr/bin/env bash
# 依存ゼロのモジュールのテストを **実際に実行する**。
#
#   tools/offline-typecheck/run-tests.sh
#
# `check.sh` は型検査しかしない (スタブのデシリアライズが `unimplemented!()` の
# ため実行できない)。しかし `src/net/` の 4 モジュールは**外部 crate を一切
# 使わない**ので、rustc に直接渡せば**テストが本当に走る**。
#
# 走るもの:
#   - net/inference.rs — Transformer の数値・チェックポイント検証・A9 の上限
#   - net/mdns.rs      — DNS-SD ワイヤ形式・実 UDP マルチキャストでの発見
#   - net/wire.rs      — フレーム形式・署名・敵性入力
#   - net/transport.rs — 実 TCP 越しのジョブ往復と支払い
#
# さらに **`core/` のテストも走る** (2026-08-18〜)。`shims/` の serde/chrono/
# uuid/rand/blake3/hex/base64 を「型が合うだけ」から「実際に動く」ものへ
# 引き上げたため。JSON は `serde.rs` のミニ実装を通る。
#
# **`--features http` も走る** (2026-08-18)。`reqwest`/`tokio` をスタブ化した。
# ただし `reqwest` の `send()` は**必ず失敗する** — ネットワークに触らないことを
# 型ではなく挙動で示している。実 mint との通信は CI でも実行しない。
#
# 走らないもの: MSRV 適合の実ビルド (rustup が古い toolchain を取得できない)、
# **実 crate との挙動差**、暗号的性質。clippy は `lint.sh` で走る。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${ROPE_TESTS_OUT:-$ROOT/tools/offline-typecheck/.build-tests}"
EDITION=2021

rm -rf "$OUT"
mkdir -p "$OUT"

# 依存順に並べる (transport は wire と inference を使う)
cat >"$OUT/root.rs" <<EOF
#[path = "$ROOT/src/net/wire.rs"]
pub mod wire;
#[path = "$ROOT/src/net/inference.rs"]
pub mod inference;
#[path = "$ROOT/src/net/transport.rs"]
pub mod transport;
#[path = "$ROOT/src/net/mdns.rs"]
pub mod mdns;
EOF

echo "── 依存ゼロモジュールをテストビルド ──"
if ! rustc --edition "$EDITION" --test -O \
    --crate-name rope_offline_tests "$OUT/root.rs" \
    -o "$OUT/tests" 2>&1 | sed 's/^/  /'; then
    echo "❌ テストのビルドに失敗"
    exit 1
fi
[ -x "$OUT/tests" ] || {
    echo "❌ テストバイナリが生成されなかった"
    exit 1
}

: > "$OUT/counts.txt"

# 実行して結果を表示しつつ、件数を `$OUT/counts.txt` に記録する。
#
# **なぜ件数を書き出すか**: 文書 (CLAUDE.md 等) が「テスト N 件」と書いており、
# それが実際の数と食い違ったまま古びていた (§1.23)。
# `tools/check-test-counts.sh` がここで書いた実測値と文書を突き合わせる。
run_suite() {
    local key="$1" bin="$2" out rc
    out="$("$bin" --test-threads=1 2>&1)"
    rc=$?
    echo "$out"
    echo "$key $(printf '%s' "$out" | sed -n 's/^test result: ok\. \([0-9]*\) passed.*/\1/p' | head -1)" \
        >> "$OUT/counts.txt"
    return $rc
}

echo
echo "── 実行 (依存ゼロモジュール) ──"
# --test-threads=1: 実ソケットを使うテストがポートとカレント環境変数を触るため
run_suite net "$OUT/tests"
status=$?

# ------------------------------------------------------------------
# core/ — スタブ経由で実際に走らせる
# ------------------------------------------------------------------
echo
echo "── core/ + net/ をスタブ経由でテストビルド ──"

# shim は**毎回ビルドし直す**。使い回すと、shim を直したのに古いものが
# 使われて「直したはずのテストが落ちる」という嘘の結果になる (実際に踏んだ)。
SHIM="$OUT/shims"
ROPE_TYPECHECK_OUT="$SHIM" "$ROOT/tools/offline-typecheck/check.sh" >/dev/null 2>&1 || {
    echo "❌ shim のビルド (= 型検査) に失敗。check.sh を直接実行して原因を見ること"
    exit 1
}

# env!("CARGO_PKG_VERSION") は cargo が渡す。ここでは自前で渡す。
CARGO_PKG_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
CARGO_PKG_NAME="rope"
export CARGO_PKG_VERSION CARGO_PKG_NAME

if rustc --edition "$EDITION" --test --crate-name rope_lib_tests "$ROOT/src/lib.rs" \
    --extern serde="$SHIM/libserde.rlib" \
    --extern serde_derive_shim="$SHIM/libserde_derive_shim.so" \
    --extern serde_json="$SHIM/libserde_json.rlib" \
    --extern chrono="$SHIM/libchrono.rlib" \
    --extern uuid="$SHIM/libuuid.rlib" \
    --extern anyhow="$SHIM/libanyhow.rlib" \
    --extern blake3="$SHIM/libblake3.rlib" \
    --extern hex="$SHIM/libhex.rlib" \
    --extern dirs="$SHIM/libdirs.rlib" \
    --extern tracing="$SHIM/libtracing.rlib" \
    --extern base64="$SHIM/libbase64.rlib" \
    --extern rand="$SHIM/librand.rlib" \
    --extern ed25519_dalek="$SHIM/libed25519_dalek.rlib" \
    -L "$SHIM" -o "$OUT/libtests" 2>&1 | sed 's/^/  /'; then
    echo
    echo "── 実行 (core/ + net/) ──"
    run_suite default "$OUT/libtests" || status=1
else
    echo "❌ core/ のテストビルドに失敗"
    status=1
fi

# ------------------------------------------------------------------
# --features http — `#[cfg(feature = "http")]` 配下も走らせる
# ------------------------------------------------------------------
echo
echo "── --features http 相当でテストビルド ──"

if rustc --edition "$EDITION" --cfg 'feature="http"' --test \
    --crate-name rope_http_tests "$ROOT/src/lib.rs" \
    --extern serde="$SHIM/libserde.rlib" \
    --extern serde_derive_shim="$SHIM/libserde_derive_shim.so" \
    --extern serde_json="$SHIM/libserde_json.rlib" \
    --extern chrono="$SHIM/libchrono.rlib" \
    --extern uuid="$SHIM/libuuid.rlib" \
    --extern anyhow="$SHIM/libanyhow.rlib" \
    --extern blake3="$SHIM/libblake3.rlib" \
    --extern hex="$SHIM/libhex.rlib" \
    --extern dirs="$SHIM/libdirs.rlib" \
    --extern tracing="$SHIM/libtracing.rlib" \
    --extern base64="$SHIM/libbase64.rlib" \
    --extern rand="$SHIM/librand.rlib" \
    --extern ed25519_dalek="$SHIM/libed25519_dalek.rlib" \
    --extern reqwest="$SHIM/libreqwest.rlib" \
    --extern tokio="$SHIM/libtokio.rlib" \
    -L "$SHIM" -o "$OUT/httptests" 2>&1 | sed 's/^/  /'; then
    echo
    echo "── 実行 (--features http) ──"
    run_suite http "$OUT/httptests" || status=1
else
    echo "❌ --features http のテストビルドに失敗"
    status=1
fi

echo
if [ "$status" -eq 0 ]; then
    echo "✅ テストは実際に実行され、全て PASS した"
    echo "   ⚠️  ただし **スタブ経由**である。実 crate との挙動差・clippy・"
    echo "       MSRV の実ビルドは未確認 (CI が必要)。"
    echo "       特に暗号 (blake3/ed25519/rand) は本物ではない — README.md 参照。"
else
    echo "❌ テスト失敗"
fi
exit "$status"

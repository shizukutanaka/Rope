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
# 走らないもの: `core/` (serde/chrono/uuid に依存)、`--features http`、
# clippy、MSRV 1.75 適合。それらは CI でしか確認できない
# (`README.md`「検出できないもの」)。

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

echo
echo "── 実行 ──"
# --test-threads=1: 実ソケットを使うテストがポートとカレント環境変数を触るため
"$OUT/tests" --test-threads=1
status=$?

echo
if [ "$status" -eq 0 ]; then
    echo "✅ 依存ゼロモジュールのテストは実際に実行され、全て PASS した"
    echo "   ⚠️  core/ のテスト・clippy・MSRV・--features http は未確認 (CI が必要)"
else
    echo "❌ テスト失敗"
fi
exit "$status"

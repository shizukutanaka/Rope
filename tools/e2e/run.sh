#!/usr/bin/env bash
# 2 プロセス E2E — v1 のループをプロセス境界を越えて回す。
#
#   tools/e2e/run.sh
#
# ## なぜこれが要るのか (ソクラテス問答の記録)
#
# 「ジョブは実際に TCP を渡り、相手のマシンで実行されて返る」という主張に
# 「それを独立したプロセス間で確認したか?」と問うたら、答えは**否**だった。
# `transport` のテストは全て **1 プロセス内の 2 スレッド**である。
#
# スレッド間とプロセス間はソケット継承・環境変数・グローバル状態の共有で
# 挙動が違う。「相手のマシン」を名乗るなら最低でもプロセス境界は越えるべきで、
# しかも**この環境で検証できた** — やっていなかっただけだった。
#
# ⚠️ 署名器はテスト用の決定論的スタブ。ここで確かめるのは
# **プロトコルとプロセス境界**であって暗号強度ではない。
# ⚠️ マルチキャストが届かない環境では発見できない。その場合は
# **スキップして理由を出す** (「届かない」を「通った」と混同しない)。

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${ROPE_E2E_OUT:-$ROOT/tools/e2e/.build}"
BIN="$OUT/e2e"
NODE_ID="e2e-lender-$$"

mkdir -p "$OUT"
echo "── ビルド ──"
if ! rustc --edition 2021 -O "$ROOT/tools/e2e/main.rs" -o "$BIN" 2>&1 | sed 's/^/  /'; then
    echo "❌ E2E のビルドに失敗"
    exit 1
fi
[ -x "$BIN" ] || { echo "❌ バイナリが生成されなかった"; exit 1; }

export ROPE_ALLOW_PLAINTEXT=1   # 転送は既定で無効。E2E は明示的に有効化する

LENDER_LOG="$OUT/lender.log"
BORROWER_LOG="$OUT/borrower.log"
rm -f "$LENDER_LOG" "$BORROWER_LOG"

echo
echo "── lender プロセス起動 ──"
# **時間上限を必ず掛ける。** 借り手が発見に失敗すると貸し手は
# accept() で永久に待ち、wait が返らなくなる (実際に踏んだ)。
timeout 25 "$BIN" lender "$NODE_ID" >"$LENDER_LOG" 2>&1 &
LENDER_PID=$!
# 待受け開始を待つ (READY 行が出るまで)
for _ in $(seq 1 50); do
    grep -q "LENDER_READY" "$LENDER_LOG" 2>/dev/null && break
    kill -0 "$LENDER_PID" 2>/dev/null || break
    sleep 0.1
done
if ! grep -q "LENDER_READY" "$LENDER_LOG" 2>/dev/null; then
    echo "❌ lender が待受けに入らなかった:"
    sed 's/^/  /' "$LENDER_LOG"
    kill "$LENDER_PID" 2>/dev/null
    exit 1
fi
sed 's/^/  /' "$LENDER_LOG"

echo
echo "── borrower プロセス起動 (実マルチキャストで発見させる) ──"
timeout 20 "$BIN" borrower "$NODE_ID" >"$BORROWER_LOG" 2>&1
B_STATUS=$?
sed 's/^/  /' "$BORROWER_LOG"

# 借り手が失敗した場合、貸し手はまだ accept() で待っている。先に落とす。
if [ "$B_STATUS" -ne 0 ]; then
    kill "$LENDER_PID" 2>/dev/null
fi
wait "$LENDER_PID" 2>/dev/null
L_STATUS=$?
echo
echo "── lender の最終出力 ──"
sed 's/^/  /' "$LENDER_LOG"

echo
if grep -q "mdns で lender が見つからない" "$BORROWER_LOG" 2>/dev/null; then
    echo "⏭  スキップ: この環境ではマルチキャストが届かない。"
    echo "   **通ったのではなく、確かめられなかった** — 結果を緑と混同しないこと。"
    exit 0
fi

fail=0
check() {
    if grep -q "$2" "$1"; then echo "  ✅ $3"; else echo "  ❌ $3 (見つからない: $2)"; fail=1; fi
}
echo "── 判定 ──"
check "$BORROWER_LOG" "BORROWER_FOUND" "借り手が実マルチキャストで貸し手を発見した"
check "$BORROWER_LOG" "BORROWER_DONE text=bbbb" "借り手が**相手のプロセスで計算された**結果を受け取った"
check "$BORROWER_LOG" "delivery=Sent" "支払いが送達された"
check "$LENDER_LOG"   "LENDER_DONE accepted=true" "貸し手がジョブを受理して実行した"
check "$LENDER_LOG"   "paid=7" "貸し手が 7 sats を受け取った"
[ "$B_STATUS" -eq 0 ] || { echo "  ❌ borrower の終了コード $B_STATUS"; fail=1; }
[ "$L_STATUS" -eq 0 ] || { echo "  ❌ lender の終了コード $L_STATUS"; fail=1; }

echo
if [ "$fail" -eq 0 ]; then
    echo "✅ 2 プロセス E2E 通過 — 発見・依頼・実行・支払いがプロセス境界を越えた"
    echo "   ⚠️  署名はテスト用スタブ。暗号強度は検証していない。"
    exit 0
fi
echo "❌ 2 プロセス E2E 失敗"
exit 1

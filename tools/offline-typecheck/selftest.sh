#!/usr/bin/env bash
# オフライン型検査ハーネスの自己検証。
#
# `check.sh` が **何も検出しないのに PASS する** (= 無害に見えて最悪の状態) に
# 陥っていないことを確かめる。既知の 4 種類の型エラーを注入し、
# 全て検出されること + 元に戻したら PASS することを確認する。
#
# 作業ツリーは一切汚さない (一時ディレクトリへコピーして実行する)。
#
#   使い方:  tools/offline-typecheck/selftest.sh
#   終了コード: 0 = ハーネスは健全 / 非 0 = ハーネス自体が壊れている

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cp -r "$ROOT/src" "$ROOT/tools" "$ROOT/Cargo.toml" "$TMP/"
CHECK="$TMP/tools/offline-typecheck/check.sh"
TARGET="$TMP/src/core/intent.rs"
PRISTINE="$TMP/intent.pristine"
cp "$TARGET" "$PRISTINE"

failures=0

# $1 = ケース名, $2 = 期待するエラーコード, $3 = python の書き換えスクリプト
expect_caught() {
    local name="$1" code="$2" patch="$3"
    cp "$PRISTINE" "$TARGET"
    TARGET="$TARGET" python3 -c "$patch" || {
        echo "  ⚠️  $name: 注入スクリプト自体が失敗 (対象コードが変わった可能性)"
        failures=$((failures + 1))
        return
    }
    local out
    out="$("$CHECK" 2>&1)"
    if [ $? -eq 0 ]; then
        echo "  ❌ $name: 注入したエラーを検出できなかった (PASS してしまった)"
        failures=$((failures + 1))
    elif ! grep -q "$code" <<<"$out"; then
        echo "  ⚠️  $name: FAIL はしたが $code ではない — 別の理由で落ちている"
        failures=$((failures + 1))
    else
        echo "  ✅ $name ($code)"
    fi
}

echo "── 注入したエラーを検出できるか ──"

expect_caught "match 網羅性" "E0004" '
import os
p=os.environ["TARGET"]; s=open(p,encoding="utf-8").read()
old="    /// Batch inference job (many prompts)."
assert old in s
open(p,"w",encoding="utf-8").write(s.replace(old,"    Injected { x: u32 },\n"+old,1))
'

expect_caught "型不一致" "E0308" '
import os
p=os.environ["TARGET"]; s=open(p,encoding="utf-8").read()
old="pub fn format_plan(plan: &ExecutionPlan) -> String {\n    let mut out = String::new();"
assert old in s
open(p,"w",encoding="utf-8").write(s.replace(old,"pub fn format_plan(plan: &ExecutionPlan) -> String {\n    let mut out: u32 = String::new();",1))
'

expect_caught "未定義名" "E0425" '
import os
p=os.environ["TARGET"]; s=open(p,encoding="utf-8").read()
old="pub fn format_plan(plan: &ExecutionPlan) -> String {"
assert old in s
open(p,"w",encoding="utf-8").write(s.replace(old,old+"\n    let _ = no_such_variable;",1))
'

expect_caught "借用後の move" "E0382" '
import os
p=os.environ["TARGET"]; s=open(p,encoding="utf-8").read()
old="pub fn format_plan(plan: &ExecutionPlan) -> String {\n    let mut out = String::new();"
assert old in s
open(p,"w",encoding="utf-8").write(s.replace(old,old+"\n    let moved = out;\n    let _ = moved;",1))
'

echo
echo "── 元に戻したら PASS するか ──"
cp "$PRISTINE" "$TARGET"
if "$CHECK" >/dev/null 2>&1; then
    echo "  ✅ 復元後 PASS"
else
    echo "  ❌ 復元後も FAIL — ハーネスか作業ツリーが壊れている"
    failures=$((failures + 1))
fi

echo
if [ "$failures" -eq 0 ]; then
    echo "✅ 自己検証 PASS — ハーネスは実際にエラーを検出している"
    exit 0
fi
echo "❌ 自己検証 FAIL ($failures 件) — check.sh の PASS を信用しないこと"
exit 1

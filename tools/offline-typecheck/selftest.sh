#!/usr/bin/env bash
# オフライン型検査ハーネスの自己検証。
#
# 2 つのことを確かめる:
#
# 1. `check.sh` が **何も検出しないのに PASS する** (= 無害に見えて最悪の状態)
#    に陥っていないこと。既知の 4 種類の型エラーを注入し、全て検出されること +
#    元に戻したら PASS することを確認する。
# 2. **MSRV 検査が効いていること** (1.87 の API を注入して検出させる)。
# 3. **shim 自身が正しいこと。** `hex`/`base64` は「本物と同じ符号化」、
#    `chrono` は「実 chrono と同じ RFC3339」と主張している。既知ベクタ
#    (RFC 4648、既知の瞬間、一世紀分の日次往復) で裏を取る。
#    ここが狂うと 358 件が「通っているのに間違っている」状態になる。
#
# 作業ツリーは一切汚さない (一時ディレクトリへコピーして実行する)。
#
#   使い方:  tools/offline-typecheck/selftest.sh
#   終了コード: 0 = ハーネスは健全 / 非 0 = ハーネス自体が壊れている

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cp -r "$ROOT/src" "$ROOT/tools" "$ROOT/Cargo.toml" "$ROOT/clippy.toml" "$TMP/"
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

# アンカーは `Workload` の唯一の variant の doc comment。
# variant 構成を変えたらここも直すこと — 変えたまま放置すると
# 「注入スクリプト自体が失敗」と報告される (黙って PASS しない)。
expect_caught "match 網羅性" "E0004" '
import os
p=os.environ["TARGET"]; s=open(p,encoding="utf-8").read()
old="    /// Single inference call (prompt \u2192 completion)."
assert old in s, "アンカーが見つからない (Workload の構成が変わった?)"
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

# ------------------------------------------------------------------
# MSRV 検査の検出力
# ------------------------------------------------------------------
#
# `lint.sh` は「MSRV 適合」と言うが、**その lint が本当に効いているか**を
# 確かめないと、ただ黙っているだけかもしれない。1.87 で安定化した API を
# 注入して、検出されることを毎回実証する。

echo
echo "── MSRV 検査は本当に効いているか ──"

cp "$PRISTINE" "$TARGET"
cat >>"$TMP/src/core/mod.rs" <<'PROBE'

/// selftest が注入する MSRV 検出プローブ (1.87 安定化 API)。
pub fn _msrv_probe(a: u64, b: u64) -> bool {
    a.is_multiple_of(b)
}
PROBE

# ⚠️ `cmd | grep -q` と書いてはいけない。このスクリプトは `set -o pipefail` なので、
# grep が一致しても **左側 (lint.sh) の非ゼロ終了でパイプライン全体が失敗扱い**に
# なる。lint.sh は警告を見つけたら必ず 1 で終わるので、常に「見逃した」と誤報する。
# 一度出力をファイルに落としてから grep する。
msrv_log="$TMP/msrv-probe.log"
ROPE_LINT_OUT="$TMP/lintout" ROPE_CLIPPY_CONF_DIR="$ROOT" \
    "$TMP/tools/offline-typecheck/lint.sh" >"$msrv_log" 2>&1
if grep -q "stable since" "$msrv_log"; then
    echo "  ✅ 1.87 API を検出した"
else
    echo "  ❌ 1.87 API を見逃した — lint.sh の「MSRV 適合」は信用できない"
    sed 's/^/     /' "$msrv_log" | head -10
    failures=$((failures + 1))
fi

# プローブを取り除く
python3 - "$TMP/src/core/mod.rs" <<'PY2'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
i = s.index("\n/// selftest が注入する MSRV 検出プローブ")
open(p, "w", encoding="utf-8").write(s[:i] + "\n")
PY2

# ------------------------------------------------------------------
# shim 自身のテスト
# ------------------------------------------------------------------
#
# `hex`/`base64` は「本物と同じ符号化」、`chrono` は「実 chrono と同じ
# RFC3339」と主張している。**主張には裏を取る** — 既知ベクタで検証する。
# ここが狂うと、上の 358 件が「通っているのに間違っている」状態になる。

echo
echo "── shim 自身の検証 (既知ベクタ) ──"

SHIMS_DIR="$ROOT/tools/offline-typecheck/shims"
SHIM_OUT="$TMP/shimtest"
mkdir -p "$SHIM_OUT"
ROPE_TYPECHECK_OUT="$SHIM_OUT/.build" "$ROOT/tools/offline-typecheck/check.sh" >/dev/null 2>&1

shim_test() {
    local m="$1"
    shift
    if ! rustc --edition 2021 --test --crate-name "${m}_selftest" \
        "$SHIMS_DIR/$m.rs" "$@" -o "$SHIM_OUT/${m}_t" >"$SHIM_OUT/$m.log" 2>&1; then
        echo "  ❌ $m: テストのビルドに失敗"
        sed 's/^/     /' "$SHIM_OUT/$m.log" | head -10
        failures=$((failures + 1))
        return
    fi
    if "$SHIM_OUT/${m}_t" >>"$SHIM_OUT/$m.log" 2>&1; then
        local n
        n=$(grep -oE "^test result: ok\. [0-9]+" "$SHIM_OUT/$m.log" | grep -oE "[0-9]+$" | head -1)
        echo "  ✅ $m (${n:-?} 件)"
    else
        echo "  ❌ $m: テスト失敗"
        grep -E "^(test |assertion|thread)" "$SHIM_OUT/$m.log" | head -10 | sed 's/^/     /'
        failures=$((failures + 1))
    fi
}

shim_test hex
shim_test base64
shim_test chrono \
    --extern serde="$SHIM_OUT/.build/libserde.rlib" \
    --extern serde_derive_shim="$SHIM_OUT/.build/libserde_derive_shim.so" \
    -L "$SHIM_OUT/.build"

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

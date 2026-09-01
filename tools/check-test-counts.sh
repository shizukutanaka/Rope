#!/usr/bin/env bash
# 文書が書いている「テスト N 件」を、**実測値**と突き合わせる。
#
#   tools/check-test-counts.sh
#
# ## なぜ在るのか
#
# `docs/ASSESSMENT.md` §4 は「テスト件数と挙動は run-tests.sh が守る」と
# 書いていたが、**守っていなかった**。実際に数えたら 6 箇所のうち 5 箇所が
# 間違っており、同じリポジトリの中で 358 / 671 / 236 の 3 つの数字が
# 同時に生きていた (docs/SURPLUS_AND_GAPS.md §1.23)。
#
# `check-doc-anchors.sh` が file:line を守るのと同じ理屈で、これは件数を守る。
# **検証できない主張は書かない。書くなら機械に守らせる。**
#
# ## 書き方
#
# 文書側は数字の直後にマーカーを置く:
#
#     テスト 313 件 <!-- tests:http -->
#
# 鍵は `run-tests.sh` が出す 3 つ:
#   net     … 依存ゼロの `src/net/` 4 モジュールだけを単独ビルドしたもの
#   default … `core/` + `net/` (既定 feature)
#   http    … 上記 + `--features http`。**これが全体の数**
#
# 3 つを足してはいけない — `net` は `default` の、`default` は `http` の
# 部分集合である。足した数 (671 等) は二重計上。
set -u
cd "$(dirname "$0")/.."

COUNTS="tools/offline-typecheck/.build-tests/counts.txt"

if [ ! -f "$COUNTS" ]; then
    echo "  実測値がありません — 先に tools/offline-typecheck/run-tests.sh を実行してください"
    exit 1
fi

# 実測値を読む
declare -A actual=()
while read -r key n; do
    [ -n "${key:-}" ] && actual["$key"]="$n"
done < "$COUNTS"

for k in net default http; do
    if [ -z "${actual[$k]:-}" ]; then
        echo "  実測値に '$k' がありません ($COUNTS が古い可能性)"
        exit 1
    fi
done

checked=0
bad=0

# マーカーを持つ行を全部拾う。数字はマーカーの直前 40 文字以内にあるもの。
while IFS= read -r hit; do
    file="${hit%%:*}"
    rest="${hit#*:}"
    line="${rest%%:*}"
    text="${rest#*:}"
    key="$(printf '%s' "$text" | sed -n 's/.*<!-- tests:\([a-z]*\) -->.*/\1/p')"
    num="$(printf '%s' "$text" | sed 's/<!-- tests:[a-z]* -->.*//' | grep -o '[0-9]\+' | tail -1)"

    if [ -z "${actual[$key]:-}" ]; then
        echo "  ⚠️  $file:$line  未知の鍵 'tests:$key' (net/default/http のいずれか)"
        bad=$((bad + 1))
        continue
    fi
    if [ -z "$num" ]; then
        echo "  ⚠️  $file:$line  マーカーの手前に数字がありません"
        bad=$((bad + 1))
        continue
    fi
    checked=$((checked + 1))
    if [ "$num" != "${actual[$key]}" ]; then
        echo "  ⚠️  $file:$line  '$key' は $num 件と書いてあるが実測は ${actual[$key]} 件"
        bad=$((bad + 1))
    fi
# `--include` は `--` より前に置くこと。後ろに置くとパス扱いになり、
# `*.md` 以外まで走査してしまう (実際に踏んだ)。
done < <(grep -rn --include='*.md' -- '<!-- tests:' . 2>/dev/null)

echo
echo "  実測      : net=${actual[net]} default=${actual[default]} http=${actual[http]}"
echo "  検証できた: $checked 件"
echo "  ずれ      : $bad 件"

[ "$bad" -eq 0 ] || exit 1

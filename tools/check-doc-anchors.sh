#!/usr/bin/env bash
# 文書中の `file:line` アンカーが、まだ正しい場所を指しているかを検証する。
#
#   tools/check-doc-anchors.sh          # 検証のみ
#   tools/check-doc-anchors.sh --fix    # ずれた行番号を書き換える
#
# ## なぜ要るのか
#
# `CLAUDE.md` 規範3 は「発見は file:line アンカーで記録する。将来の実装者が
# 必ず参照する」と定めている。しかし**コードを直すたびに行番号はずれる**。
# 実測 (2026-08-18): 主要アンカー 10 件のうち **9 件がずれていた**。
# 「必ず参照する」ものが 9 割間違っている状態は、記録が無いより悪い —
# 読んだ人を誤った場所へ連れて行くからである。
#
# 規範6 (正直さ) を人間の注意力ではなく**機械で守らせる**のがこのスクリプト。
#
# ## 何を検証するか
#
# アンカーの近傍 (同一行の前後 + 折り返しの次行) にあるバッククォート付き
# Rust 識別子を拾い、その識別子が指定行の ±WINDOW 行に実在するかを見る。
# 見つからなければ、正しい行を探して報告する。
#
# ⚠️ **これは「アンカーが正しい」の証明ではない。** 識別子を伴わないアンカー
# (`src/core/pair.rs:167-207` のように範囲だけのもの) は**検証できない**。
# 検証できた件数と、できなかった件数を必ず両方表示する。

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

MODE="${1:-check}"
WINDOW=12

python3 - "$MODE" "$WINDOW" <<'PY'
import os, re, sys, glob

mode, window = sys.argv[1], int(sys.argv[2])
fix = (mode == "--fix")

# `src/core/ecash.rs:123` (フルパス) と `ecash.rs:123` (短縮形) の両方。
# **短縮形が圧倒的多数** (実測 127 対 13) なので、これを拾えないと意味が無い。
ANCHOR = re.compile(r'`((?:src/[A-Za-z0-9_/]+/)?([A-Za-z0-9_]+\.rs)):(\d+)(?:-(\d+))?`')

# 短縮形 (`ecash.rs`) を実ファイルへ解決する表。同名が複数あれば曖昧として扱う。
BASENAMES = {}
for _p in glob.glob("src/**/*.rs", recursive=True):
    BASENAMES.setdefault(os.path.basename(_p), []).append(_p)
# バッククォート付きの Rust 識別子 (メソッドパスも可)
SYMBOL = re.compile(r'`([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*)`')

# 識別子として拾っても意味の無い語 (型名・一般語)
NOISE = {"Ok", "Err", "Some", "None", "String", "Vec", "true", "false", "self"}

src_cache = {}
def lines_of(path):
    if path not in src_cache:
        try:
            src_cache[path] = open(path, encoding="utf-8").read().split("\n")
        except OSError:
            src_cache[path] = None
    return src_cache[path]

def find_symbol(lines, sym):
    """シンボル定義らしき行を探す。最後の :: 以降を使う。"""
    leaf = sym.split("::")[-1]
    # **定義**として一意に決まるものだけを対象にする。
    # フィールド名 (`total_sats`) やローカル変数は複数箇所に現れて
    # 「正しい行」を決められないので、意図的に扱わない。
    pats = [f"fn {leaf}(", f"fn {leaf}<", f"struct {leaf}", f"enum {leaf}",
            f"trait {leaf}", f"const {leaf}:", f"impl {leaf}"]
    for i, l in enumerate(lines, 1):
        for p in pats:
            if p in l:
                return i
    return None

checked = drift = unverifiable = missing = 0
fixes = []          # (docfile, old_text, new_text)
reports = []

docs = sorted(set(glob.glob("*.md") + glob.glob("docs/*.md") + glob.glob("tools/**/*.md", recursive=True)))
for doc in docs:
    text = open(doc, encoding="utf-8").read()
    out_lines = text.split("\n")
    for idx, line in enumerate(out_lines):
        for m in ANCHOR.finditer(line):
            raw, base, start, end = m.group(1), m.group(2), int(m.group(3)), m.group(4)
            if raw.startswith("src/"):
                path = raw
            else:
                cands = BASENAMES.get(base, [])
                if len(cands) != 1:
                    # 同名ファイルが複数 / 存在しない → 解決できないので数えるだけ
                    unverifiable += 1
                    continue
                path = cands[0]
            src = lines_of(path)
            if src is None:
                missing += 1
                reports.append(f"❌ {doc}:{idx+1}  ファイル無し: {path}")
                continue
            # シンボルはアンカーの前にも後ろにも書かれる。文書は 80 字で
            # 折り返すので**次の行**に載ることもある。両方見る。
            nxt = out_lines[idx+1] if idx + 1 < len(out_lines) else ""
            # **アンカーに近いシンボルほど強い。** 記法の主流は
            # 「`file.rs:123` `symbol`」= 直後だが、「`symbol` (`file.rs:123`)」
            # のように直前に置く行もある。距離を測らずに「最初に解決した方」を
            # 採ると、隣の項目のシンボルを掴んで**誤ったアンカーを書き込む**
            # (実例: issue_pow_challenge に CapabilityChallenge の行を書いた)。
            # **方向を問わず「隣接しているもの」だけを採る。**
            # 記法は 2 通りある:
            #   `file.rs:123` `symbol`      … 直後 (隙間 1 字)
            #   `symbol` (`file.rs:123`)    … 直前 (隙間 2 字)
            # どちらも隙間はごく小さい。「直後を優先」のような方向バイアスを
            # 入れると、`symbol` (`anchor`) 記法で**隣の項目のシンボル**を
            # 掴んで誤ったアンカーを書き込む (実際にやった)。
            # 離れているものは採らず「検証できない」に倒す —
            # **未検証は正直だが、誤った書き換えは嘘になる。**
            ADJACENT = 30
            cands = []
            for sm in SYMBOL.finditer(line):
                if sm.group(1) in NOISE:
                    continue
                if sm.start() < m.end() and sm.end() > m.start():
                    continue
                gap = sm.start() - m.end() if sm.start() >= m.end() else m.start() - sm.end()
                if gap <= ADJACENT:
                    cands.append((gap, sm.group(1)))
            # 行折り返しで次行の先頭に来る場合だけ、次行も見る
            for sm in SYMBOL.finditer(nxt[:ADJACENT]):
                if sm.group(1) not in NOISE:
                    cands.append((ADJACENT + sm.start(), sm.group(1)))
            cands.sort()
            sym = None
            for _, s in cands:
                if find_symbol(src, s) is not None:
                    sym = s
                    break
            if sym is None:
                unverifiable += 1
                continue
            checked += 1
            lo, hi = max(1, start - window), min(len(src), start + window)
            leaf = sym.split("::")[-1]
            near = any(leaf in l for l in src[lo-1:hi])
            if near:
                continue
            actual = find_symbol(src, sym)
            drift += 1
            if actual is None:
                reports.append(f"❌ {doc}:{idx+1}  {path}:{start} → `{sym}` が見つからない")
                continue
            span = (int(end) - start) if end else 0
            # 元の記法 (フルパス / 短縮形) をそのまま保つ
            new_anchor = f"`{raw}:{actual}" + (f"-{actual+span}`" if end else "`")
            reports.append(f"⚠️  {doc}:{idx+1}  {path}:{start} → `{sym}` は実際は {actual} 行目")
            # **位置で記録する。** 同じアンカー文字列が別のシンボルを指して
            # 複数箇所に現れることがあり (実例: first_run.rs:729 が
            # should_show_first_run と step_identity の両方で使われていた)、
            # 全置換すると**正しい方を壊す**。実際にこのツール自身が
            # 振動して発覚した。
            fixes.append((doc, idx, m.start(), m.end(), new_anchor))

if fix and fixes:
    per_doc = {}
    for doc, li, a, b, new in fixes:
        per_doc.setdefault(doc, []).append((li, a, b, new))
    for doc, items in per_doc.items():
        dl = open(doc, encoding="utf-8").read().split("\n")
        # 行内オフセットが崩れないよう、後ろから適用する
        for li, a, b, new in sorted(items, key=lambda x: (-x[0], -x[1])):
            dl[li] = dl[li][:a] + new + dl[li][b:]
        open(doc, "w", encoding="utf-8").write("\n".join(dl))
    print(f"🔧 {len(fixes)} 件のアンカーを修正しました")

for r in reports:
    print("  " + r)

print()
print(f"  検証できた    : {checked} 件")
print(f"  ずれ          : {drift} 件")
print(f"  検証できない  : {unverifiable} 件  (シンボルを伴わないアンカー)")
print(f"  ファイル無し  : {missing} 件")

if missing or (drift and not fix):
    sys.exit(1)
sys.exit(0)
PY

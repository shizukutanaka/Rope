#!/usr/bin/env bash
# `Cargo.lock` で固定された依存が、全て MSRV (Cargo.toml の
# `rust-version`) で使えるかを確かめる。
#
#   tools/offline-typecheck/check-deps-msrv.sh
#
# ## なぜ必要か
#
# CI (`.github/ci.yml.disabled`) は **`Cargo.toml` の `rust-version` と同じ版に
# 固定**されている。依存のどれか 1 つでもそれより上を要求していたら、
# `cargo check` が**最初のジョブで落ちる**。
#
# それはこのリポジトリで**実際に何度も起きた問題**である —
# `Cargo.toml` に `base64ct = ">=1, <1.7"` や `getrandom = ">=0.2, <0.3"` と
# いった上限が並んでいるのは、edition2024 / MSRV 引き上げを避けるため。
# 上限を書いた当時は正しくても、**別の transitive 依存が後から上げてくる**。
#
# ## なぜこの環境でできるのか
#
# `static.crates.io` (crate 本体) は 403 だが、**`index.crates.io`
# (メタデータ) は 200 で通る**。index は各バージョンの `rust_version` を
# 持っているので、**crate を 1 つもダウンロードせずに** MSRV を検証できる。
#
# ⚠️ `rust_version` を**宣言していない** crate は判定できない。
# 宣言が無い = 古い crate であることが多いが、保証ではない。
# 未宣言の数は必ず表示する (黙って「全部 OK」と言わない)。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOCK="$ROOT/Cargo.lock"
MSRV="$(sed -n 's/^rust-version = "\([^"]*\)".*/\1/p' "$ROOT/Cargo.toml" | head -1)"

[ -f "$LOCK" ] || {
    echo "❌ Cargo.lock がありません"
    exit 1
}
[ -n "$MSRV" ] || {
    echo "❌ Cargo.toml に rust-version がありません"
    exit 1
}

echo "ℹ️  MSRV $MSRV に対して Cargo.lock の依存を検査します"
echo "   (index.crates.io のメタデータのみ — crate は 1 つもダウンロードしません)"
echo

python3 - "$LOCK" "$MSRV" <<'PY'
import json, re, subprocess, sys, urllib.parse

lock_path, msrv = sys.argv[1], sys.argv[2]


def ver_tuple(v):
    """'1.75' / '1.75.0' -> (1, 75, 0)。比較用。"""
    parts = re.findall(r"\d+", v or "")
    parts = [int(p) for p in parts[:3]]
    while len(parts) < 3:
        parts.append(0)
    return tuple(parts)


target = ver_tuple(msrv)

# Cargo.lock から (name, version) を拾う
text = open(lock_path, encoding="utf-8").read()
pkgs = []
for block in text.split("[[package]]"):
    n = re.search(r'^name = "([^"]+)"', block, re.M)
    v = re.search(r'^version = "([^"]+)"', block, re.M)
    if n and v:
        pkgs.append((n.group(1), v.group(1)))

# 自分自身は除く
pkgs = [(n, v) for n, v in pkgs if n != "rope"]


def index_path(name):
    """crates.io sparse index のパス規則。"""
    n = name.lower()
    if len(n) == 1:
        return f"1/{n}"
    if len(n) == 2:
        return f"2/{n}"
    if len(n) == 3:
        return f"3/{n[0]}/{n}"
    return f"{n[:2]}/{n[2:4]}/{n}"


violations, unknown, checked, errors = [], [], 0, []

for name, version in sorted(set(pkgs)):
    url = f"https://index.crates.io/{index_path(name)}"
    try:
        raw = subprocess.run(
            ["curl", "-sS", "--max-time", "20", url],
            capture_output=True, text=True, timeout=40,
        ).stdout
    except Exception as e:  # noqa: BLE001
        errors.append((name, version, str(e)))
        continue
    if not raw.strip():
        errors.append((name, version, "index から応答なし"))
        continue

    found = None
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError:
            continue
        if entry.get("vers") == version:
            found = entry
            break

    if found is None:
        errors.append((name, version, "index にこのバージョンが無い"))
        continue

    checked += 1
    rv = found.get("rust_version")
    if not rv:
        unknown.append((name, version))
    elif ver_tuple(rv) > target:
        violations.append((name, version, rv))

# --- どのビルド構成から到達するかを Cargo.lock の依存グラフで切り分ける ---
graph = {}
for block in text.split("[[package]]")[1:]:
    n = re.search(r'^name = "([^"]+)"', block, re.M)
    if not n:
        continue
    deps = []
    m = re.search(r"^dependencies = \[(.*?)\]", block, re.M | re.S)
    if m:
        deps = re.findall(r'"([^" ]+)', m.group(1))
    graph[n.group(1)] = deps


def reach(roots):
    seen, stack = set(), list(roots)
    while stack:
        p = stack.pop()
        if p in seen or p not in graph:
            continue
        seen.add(p)
        stack.extend(graph[p])
    return seen


rope_deps = [d for d in graph.get("rope", []) if d in graph]
# `reqwest` / `tokio` は `http` feature 配下 (Cargo.toml の optional)
default_reach = reach([d for d in rope_deps if d not in ("reqwest", "tokio")])
all_reach = reach(rope_deps)

# ⚠️ ヒューリスティック: Cargo.lock は cfg(target) を記録しないので、
# 「どのプラットフォームで実際にコンパイルされるか」は分からない。
# 名前から明らかに別プラットフォーム専用と分かるものだけ印を付ける。
PLATFORM_PREFIXES = (
    "windows", "wasm-bindgen", "js-sys", "web-sys", "wasi", "wit-bindgen",
    "core-foundation", "security-framework", "objc",
)


def platform_only(name):
    return any(name.startswith(p) for p in PLATFORM_PREFIXES)


print(f"検査したパッケージ: {checked} / {len(set(pkgs))}")
print(f"  MSRV 宣言あり  : {checked - len(unknown)}")
print(f"  MSRV 宣言なし  : {len(unknown)}  (判定不能 — 下記)")
if errors:
    print(f"  index で確認できず: {len(errors)}")
    for n, v, why in errors[:10]:
        print(f"    - {n} {v}: {why}")
print()

if violations:
    default_v, http_v, plat_v = [], [], []
    for n, v, rv in violations:
        if platform_only(n):
            plat_v.append((n, v, rv))
        elif n in default_reach:
            default_v.append((n, v, rv))
        elif n in all_reach:
            http_v.append((n, v, rv))
        else:
            plat_v.append((n, v, rv))

    print(f"⚠️  MSRV {msrv} を超える依存が {len(violations)} 件あります。")
    print("    どの CI ジョブに効くかで分けます:")
    print()

    def show(title, items, note):
        print(f"  [{title}] {len(items)} 件")
        if note:
            print(f"    {note}")
        for n, v, rv in items:
            print(f"      - {n} {v} → rust-version {rv}")
        print()

    show(
        "既定ビルドで到達 (check / test / clippy / fmt が落ちる)",
        default_v,
        "" if default_v else "なし",
    )
    show(
        "http feature でのみ到達 (test-http / clippy --features http が落ちる)",
        http_v,
        "" if http_v else "なし",
    )
    show(
        "別プラットフォーム専用 (ubuntu-latest ではコンパイルされない見込み)",
        plat_v,
        "⚠️ 名前からの推定。Cargo.lock は cfg(target) を持たないので確証ではない。",
    )

    if default_v or http_v:
        print(f"❌ ホスト上でコンパイルされる違反が {len(default_v) + len(http_v)} 件。")
        print(f"   このまま CI ({msrv} 固定) を有効化すると落ちます。取りうる手:")
        print("   (a) CI と Cargo.toml の rust-version を上げる  ← 製品判断")
        print("   (b) Cargo.toml に上限を足して古いバージョンへ固定する")
        print("   (c) CI から --features http のジョブを外す")
        sys.exit(1)

    print("✅ ホスト上でコンパイルされる範囲には違反なし")
    print("   (上記は別プラットフォーム専用と推定されるもののみ)")
else:
    print(f"✅ 固定された依存はすべて MSRV {msrv} 以下です")

if unknown:
    print(f"⚠️  {len(unknown)} 件は rust-version を宣言しておらず判定できません:")
    for n, v in unknown[:10]:
        print(f"    {n} {v}")
    if len(unknown) > 10:
        print(f"    … 他 {len(unknown) - 10} 件")
    print("  宣言が無い = 古い crate であることが多いですが、保証ではありません。")
PY

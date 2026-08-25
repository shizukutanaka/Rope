# オフライン型検査ハーネス

`cargo` が使えない環境で `src/` 全体を **rustc に通す**ための道具。

```sh
tools/offline-typecheck/check.sh      # 型検査: src/ 全体 (約 2.5 秒)
tools/offline-typecheck/run-tests.sh  # テスト 358 件を**実際に実行**
tools/offline-typecheck/lint.sh       # clippy (registry 不要)
tools/offline-typecheck/selftest.sh   # ハーネス自体の健全性検証
```

**`clippy-driver` は toolchain 同梱で registry を必要としない。**
`cargo clippy` が使えなくても、rustc と同じ引数で直接叩けば lint できる。
🔴 **ただし CI は clippy を 1.75 に固定している。**ここの clippy は最新版
(1.94 系) なので、**ここで出た lint が CI では出ない**ことがあり、
**その提案に従うと MSRV 1.75 を壊す**ことがある。
実例: `clippy::manual_is_multiple_of` が勧める `is_multiple_of` は
**Rust 1.87 で安定化**した API で、1.75 ではコンパイルできない。
`lint.sh` は既定でその種の lint を抑止する (`ROPE_LINT_ALL=1` で全表示)。

**`run-tests.sh` は型検査ではなく実行である。**
**`src/` のテスト 358 件すべてがこの環境で実際に走る** (2026-08-18〜)。

- `src/net/` の `inference`/`mdns`/`wire`/`transport` は外部 crate を使わないので
  そのまま走る — 実 UDP マルチキャストでのピア発見、実 TCP 越しのジョブ往復と
  支払い、Transformer の数値検証を含む。
- **`core/` も走る。** `shims/` を「型が合うだけ」から「**実際に動く**」ものへ
  引き上げたため: JSON は `serde.rs` のミニ実装、`hex`/`base64` は**本物と同じ
  符号化**、`chrono` は**実時刻**、`uuid` は**一意**、`rand` は擬似乱数、
  `blake3`/`ed25519` は「入力が違えば出力も違う」性質を持つ非暗号実装。

🔴 **これは「cargo test が通る」ことを意味しない。** スタブ経由の実行であり、
実 crate との挙動差は残る (下記「検出できないもの」)。

---

## なぜ在るのか

この環境は `static.crates.io` が組織のエグレスポリシーで 403 のため、
`cargo build`/`test`/`clippy` が **1 つも動かない**
([`docs/SURPLUS_AND_GAPS.md`](../../docs/SURPLUS_AND_GAPS.md) §0)。
GitHub の codeload / API も 403 で、crate ソースを別経路から取ることもできない。

その結果 `830d8c4` 以降のコミットは全て「コンパイラ未検証」で積み上がっていた。
**変更 → 正しさが分かるまでの時間が事実上無限大**という状態である。

しかし `src/lib.rs` が実際に使う外部 API は
**8 crate・約 30 項目**しかない (`use` 文を数えれば分かる)。
218 個の transitive crate は、その 30 項目のために引かれている。

→ **その 30 項目だけをスタブで置き換えれば、rustc は動く。**
`proc_macro` は rustc 同梱で crates.io を必要としないため、
`#[derive(Serialize)]` や `#[derive(Parser)]` もスタブ化できる。

これがこのハーネスの全てである。**変更 → 判明までが 2.5 秒になる。**

---

## 何を検査しているか

| 対象 | 行数 | 内容 |
|---|---|---|
| `src/lib.rs` | ~11,400 | `core/` 7 モジュール + `net/` (http feature 無し) |
| `src/lib.rs --test` | 同上 | **テスト関数の本体も型検査対象** (実行はしない) |
| `src/main.rs` | ~650 | 4 動詞の CLI 層 |
| `src/main.rs --test` | 同上 | 同上 |

**`src/` の 100% が対象。**

---

## ✅ 検出できるもの

`selftest.sh` が毎回実際に注入して確かめている 4 つ:

| 種類 | rustc コード |
|---|---|
| `match` の網羅性漏れ (variant 追加/削除の取りこぼし) | `E0004` |
| 型不一致・引数の数/型の誤り・戻り値の誤り | `E0308` |
| 未定義の名前・解決できないパス・壊れた import | `E0425` / `E0433` |
| 借用・move の誤り | `E0382` |

加えて (自己検証はしていないが同じ機構で検出される):
トレイト境界の不足、ライフタイムの誤り、privacy 違反、重複定義。

**rustc 自身の警告も出る** — `unreachable_pattern` / `unused_mut` /
`unused_assignments` / `unused_imports` 等。`Cargo.toml` の CI は
`RUSTFLAGS="-D warnings"` なので、**これらは CI では失敗になる**。
実際 2026-08-18 の `Batch`/`Agent` 削除で 3 件の警告が出て、
push 前に潰せた。**「型が通る」だけでなく「CI の警告ゲートも通りそう」まで
分かる** (ただし clippy の lint は別 — 下記)。

**「コンパイラ無しで削除するのは怖い」という制約は、これで解ける。**
variant を消したら網羅性エラーで即座に分かる。

---

## ❌ 検出できないもの (ここを読まずに使わないこと)

**このハーネスが PASS しても「cargo でビルドできる」とは言えない。**
以下は原理的に検出できない:

1. **スタブと実 crate のシグネチャ差** — `shims/` は実 crate のドキュメント
   から手で書いたもので、**実物と突き合わせて検証されていない**。
   スタブ側の型が実物と違えば、偽陽性 (無い筈のエラー) も
   **偽陰性 (本当はあるエラーを見逃す)** も起こりうる。
2. **serde の derive がフィールドに課す境界** — 本物の
   `#[derive(Serialize)]` は全フィールドに `Serialize` を要求するが、
   スタブは空 impl を吐くだけ。**serde 不可能なフィールドを持つ型は
   ここを通って cargo で落ちる。**
3. **`serde_json::json!` の中身** — トークンを読み捨てるため、
   `json!` 内の式は**一切型検査されない**。
4. **clap の引数仕様** — `#[arg(long, default_value = "...")]` は解釈しない。
   CLI の引数定義の誤りは検出できない。
5. **`--features http` の経路** — `reqwest`/`tokio` はスタブ化していない。
6. **MSRV 1.75 適合** — ここの rustc は 1.94。1.94 で通っても
   1.75 で通るとは限らない (これは CI でしか確かめられない)。
7. **clippy のバージョン差** — `lint.sh` で clippy は**走る**ようになったが、
   ここは 1.94 系、CI は **1.75 固定**。新しい lint が余計に出たり、
   逆に 1.75 だけの挙動を見落としたりする。**提案が 1.75 に存在する API か
   必ず確認すること。**
8. **スタブと実 crate の挙動差** — テストは走るが、走っているのは
   **スタブを通した挙動**である。最も大きい既知の差:
   - **`chrono::DateTime` の JSON 表現**。実 serde は RFC3339 文字列、
     ここはエポックからの**ナノ秒 (数値)**。したがって round-trip テストが
     通っても、**実 serde_json が書いたファイルと互換だという意味ではない。**
   - **`serde` の属性**は 6 形しか解釈しない
     (`rename_all` snake/lowercase, `rename`, `default`, `tag`)。
     未対応の属性は**黙って無視される**ので、新しい属性を使うと CI で落ちうる。
   - **`json!` の中身は型検査すらされない。**
9. 🔴 **暗号的性質はゼロ** — `blake3` は BLAKE3 ではなく、`ed25519` は
   Ed25519 ではなく、`rand` は CSPRNG ではない。いずれも「入力が違えば出力も
   違う」だけの非暗号実装で、**ロジックのテストを通すためだけに存在する**。
   衝突耐性・原像計算困難性・鍵の安全性は**一切保証しない**。
   セキュリティの検証にこのハーネスを使ってはいけない。

### 一言でいうと

> **型検査とテストは走る。しかし `cargo` の代用ではない。**
> 走っているのは**スタブを通した挙動**であり、実 crate・clippy・MSRV 1.75・
> `--features http`・暗号的性質はどれも未検証。
> 「ハーネスが緑」を「ビルドできる」「動く」「安全」と言い換えないこと
> ([`CLAUDE.md`](../../CLAUDE.md) 規範6「正直さの文化」)。

CI が有効化されたら、このハーネスは**捨てるのではなく残す** —
CI は数分、これは 2.5 秒で、編集中のループはこちらの方が速い。
ただし**出荷判定は必ず CI 側で行う**こと。

---

## 構成

```
tools/offline-typecheck/
  check.sh                    型検査の実行 (src/ 全体)
  run-tests.sh                依存ゼロ 4 モジュールのテストを**実行**
  selftest.sh                 ハーネス自体の健全性検証
  shims/
    serde_derive_shim.rs      proc-macro: Serialize / Deserialize
    clap_derive_shim.rs       proc-macro: Parser / Subcommand / Args / ValueEnum
    serde.rs serde_json.rs chrono.rs uuid.rs anyhow.rs
    blake3.rs ed25519_dalek.rs rand.rs base64.rs hex.rs
    tracing.rs dirs.rs clap.rs
  .build/ .build-tests/       中間生成物 (git 管理外)
```

`tools/` は `src/`・`examples/`・`tests/`・`benches/` のいずれでもないため、
**cargo は一切ビルドしない**。実際の依存関係には影響しない。

## スタブを直すとき

`check.sh` が「実 crate なら通る筈のコード」で落ちたら、それはスタブの
不足である。エラーが名指しするメソッド/型を該当スタブに足す。
**スタブに足す時は必ず実 crate のドキュメントに合わせること** —
適当なシグネチャを足すと、上記の偽陰性 (1) をこちらから作り込むことになる。

新しい外部 crate を `src/` に足した場合は `shims/` に新しいスタブと、
`check.sh` のビルド順・`--extern` 行を追加する。

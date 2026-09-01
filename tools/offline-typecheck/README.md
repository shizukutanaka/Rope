# オフライン型検査ハーネス

`cargo` が使えない環境で `src/` 全体を **rustc に通す**ための道具。

`selftest.sh` は**ハーネス自身とスタブの正しさ**を検証する — 型エラーを注入して
検出されることに加え、`hex`/`base64` を **RFC 4648 の試験ベクタ**で、
`chrono` を**既知の瞬間と一世紀分の日次往復**で確認する。
「本物と同じ」と主張する以上、裏を取らなければ意味がない。

```sh
tools/offline-typecheck/check.sh           # 型検査: src/ 全体 (約 2.5 秒)
tools/offline-typecheck/run-tests.sh      # テスト 313 件 <!-- tests:http --> を**実際に実行**
tools/offline-typecheck/lint.sh           # clippy + MSRV (registry 不要)
tools/offline-typecheck/check-deps-msrv.sh # 依存の MSRV (index のみ使用)
tools/offline-typecheck/selftest.sh       # ハーネス自体の健全性検証
tools/check-doc-anchors.sh                # 文書の file:line アンカー検証
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
**`src/` のテスト 313 件 <!-- tests:http --> すべてがこの環境で実際に走る**
(2026-08-18〜)。内訳は 305 件 <!-- tests:default --> が既定 feature、
残りが `--features http`。依存ゼロの `src/net/` だけを単独ビルドした
62 件 <!-- tests:net --> は前者の部分集合である — **3 つを足してはいけない。**

これらの数字は `tools/check-test-counts.sh` が実測値と突き合わせる
(`check-doc-anchors.sh` が file:line を守るのと同じ理屈)。

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
| `src/lib.rs` (+`--cfg feature="http"`) | +約 900 | Cashu mint の実 HTTP クライアント |

**`src/` の 100% が対象** — `#[cfg(feature = "http")]` 配下も含む。

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
5. ✅ **`--features http` の経路も検査・実行できるようになった** (2026-08-18)。
   `reqwest`/`tokio` をスタブ化した (`--cfg 'feature="http"'` で入れる)。
   🔴 ただし **`reqwest` の `send()` は必ず失敗する** — ネットワークに触らない
   ことを型ではなく挙動で示している。**実 mint との通信は検証していない**
   (それは CI でも実行しない)。
6. **MSRV 1.75 適合** — ここの rustc は 1.94。1.94 で通っても
   1.75 で通るとは限らない (これは CI でしか確かめられない)。
7. **clippy のバージョン差** — `lint.sh` で clippy は**走る**ようになったが、
   ここは 1.94 系、CI は **1.75 固定**。新しい lint が余計に出たり、
   逆に 1.75 だけの挙動を見落としたりする。**提案が 1.75 に存在する API か
   必ず確認すること。**
8. **スタブと実 crate の挙動差** — テストは走るが、走っているのは
   **スタブを通した挙動**である。既知の差:
   - **`serde` の属性**は 6 形しか解釈しない
     (`rename_all` snake/lowercase, `rename`, `default`, `tag`)。
     未対応の属性は**黙って無視される**ので、新しい属性を使うと CI で落ちうる。
   - **`json!` の中身は型検査すらされない。**
   - JSON の**キー順・空白**は実 `serde_json` と一致する保証が無い
     (往復はできるが、バイト単位の一致は CI でしか確かめられない)。
   - ✅ **`chrono::DateTime` は RFC3339 に揃えた** (2026-08-18)。以前は
     ナノ秒の数値で、実ファイルと互換でなかった。暦の変換は Howard Hinnant の
     アルゴリズムで、`selftest.sh` が既知の瞬間と**一世紀分の日次往復**で
     裏を取っている。
9. 🔴 **暗号的性質はゼロ** — `blake3` は BLAKE3 ではなく、`ed25519` は
   Ed25519 ではなく、`rand` は CSPRNG ではない。いずれも「入力が違えば出力も
   違う」だけの非暗号実装で、**ロジックのテストを通すためだけに存在する**。
   衝突耐性・原像計算困難性・鍵の安全性は**一切保証しない**。
   セキュリティの検証にこのハーネスを使ってはいけない。

10. **依存グラフの MSRV 検査は「下限」でしかない**
    (`check-deps-msrv.sh`)。`index.crates.io` は 200 なので `Cargo.lock` の
    217 パッケージの `rust_version` を crate 本体無しで読めるが:
    - **55 件は `rust_version` を宣言していない** — 判定不能。宣言が無い =
      古い crate であることが多いが、**保証ではない**
    - `Cargo.lock` は `cfg(target)` を持たないので、
      「別プラットフォーム専用だから ubuntu では無関係」の判定は
      **crate 名からの推定**であって確証ではない
    - 宣言された `rust_version` を見るだけで、**実際に 1.75 でビルドしたわけ
      ではない**。構文・借用検査の版差は見えない

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
    reqwest.rs tokio.rs        `--features http` 用 (send() は必ず失敗する)
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

# Cashu BDHKE 実装レディネス — ネットワークアクセス復旧後にすぐ実行するための runbook

> 作成: 2026-07-10。`docs/P2P_IMPLEMENTATION_READINESS.md` と対になる文書。
> 対象読者: `cargo build` が復旧した後、最初に実 BDHKE (Cashu の盲目署名) に
> 着手するセッション。ここに書いてあることは**今のセッションでは実行できない**
> (ビルド不能、`docs/SURPLUS_AND_GAPS.md` §0 参照)。プロトコル仕様は
> Cashu 公式 NUT-00 ([cashubtc/nuts](https://github.com/cashubtc/nuts/blob/main/00.md))
> を直接 WebFetch で取得し検証済み — 数式は本物、実装コードは未検証の設計提案。

---

## 0. 現状: プレースホルダの正確な位置と挙動

- `src/core/ecash.rs:453-474` `build_proof` — `C` (unblinded signature) を
  `blake3::hash(format!("C|{}|{}|{}", keyset_id, amount, secret))` から
  33バイト圧縮 secp256k1 point 風のバイト列 (`0x02` prefix + 32バイト) を
  組み立てているだけ。楕円曲線演算は一切行っていない。
- `src/net/cashu_mint.rs:529-556` `build_blinded_outputs` — 同様に `B'`
  (blinded point) を `HashToCurve` 風の hex 文字列で偽装。
- `src/net/cashu_mint.rs:361-368` — NUT-07 (`POST /v1/checkstate`, 二重使用
  検証) は `Ys` フィールド (`hash_to_curve(secret)`) を正しく送れないため
  **未実装のまま** (secp256k1 依存を避けるための意図的な選択、コード
  コメントに明記)。
- 両ファイルとも wire format (JSON構造、フィールド名) は NUT 仕様に準拠 —
  **配線・シリアライズ層は完成しており、中身の楕円曲線演算だけが空洞**。

---

## 1. BDHKE プロトコル仕様 (NUT-00, 2026-07-10 に公式リポジトリで検証済み)

Alice = wallet (Rope クライアント)、Bob = mint、Carol = 検証者 (Bob 自身)。

```
1. hash_to_curve:
   Y = PublicKey('02' || SHA256(msg_hash || counter))
   msg_hash = SHA256(DOMAIN_SEPARATOR || x)
   DOMAIN_SEPARATOR = b"Secp256k1_HashToCurve_Cashu_"
   counter は 0 から、有効な curve point が見つかるまでインクリメント

2. Blinding (Alice が計算):
   B_ = Y + rG     (r = ランダムな blinding factor, スカラー)

3. Mint Signing (Bob が計算、B_ を受け取って):
   C_ = kB_        (k = その amount/keyset に対応する mint の秘密鍵)

4. Unblinding (Alice が計算、C_ を受け取って):
   C = C_ - rK     (K = kG, mint の公開鍵)
   証明: C_ - rK = kB_ - rK = k(Y+rG) - rkG = kY + krG - krG = kY = C

5. Verification (Bob/Carol が検証):
   k * hash_to_curve(x) == C  が成立するか確認
```

**現行コードとの対応**:
- `ecash.rs::build_proof` の `secret` = 上記の `x`。今は `Y = hash_to_curve(x)`
  を計算していない (blake3 nullifier とは別物、混同注意 — nullifier は
  replay 検知用の別ハッシュで、こちらは本物・変更不要)。
- `cashu_mint.rs::build_blinded_outputs` が `B_` を計算する場所。
  `r` (blinding factor) をここで生成し、**secret に保持しておく必要がある**
  (unblinding 時に再度使うため — 現状のコードは `r` という概念自体が存在しない)。
- mint 側の `C_` 計算・返却は `net/cashu_mint.rs` の HTTP レスポンス
  パース箇所 (`http` feature 配下)。**Rope はここでは mint 役ではなく
  wallet 役** なので `C_` を計算する必要はなく、mint からのレスポンスを
  受け取って unblinding するだけでよい。

---

## 2. Rust crate の選定

| 選択肢 | 特徴 | Rope との整合 |
|---|---|---|
| **`secp256k1`** ([crates.io](https://crates.io/crates/secp256k1)) | Pieter Wuille の `libsecp256k1` (C実装) への Rust バインディング。Bitcoin Core / rust-bitcoin エコシステムで最も実績豊富。 | **C FFI を含む** — Rope の既存暗号依存 (`ed25519-dalek`, `blake3`) は全て pure Rust。FFI 境界の監査コストが増える。 |
| **`k256`** ([crates.io](https://crates.io/crates/k256), RustCrypto) | Pure Rust の secp256k1 実装。ECDSA/ECDH/BIP340 Schnorr 対応、secret 依存演算は定数時間実装が目標。`elliptic-curve` crate のトレイトを利用。 | **Rope の既存依存 (dalek-cryptography系, RustCrypto系) と一貫** — pure Rust 方針を維持できる。 |
| paritytech/libsecp256k1 | pure Rust だが **メンテナンス終了** (k256 への移行が公式に推奨されている)。 | 非推奨、選択肢から除外。 |

**推奨: `k256`**。理由は (1) pure Rust で `unsafe_code = "deny"` の既存方針と
自然に整合、(2) 既存の `ed25519-dalek`/`curve25519-dalek` と同じ RustCrypto
エコシステムでメンテナンス体制の一貫性がある、(3) Cashu 公式実装 (CDK) 側の
依存は今回未確認のため独自に精査要 — もし CDK が `secp256k1` を使っているなら
将来的な相互運用検証で有利な可能性はあるが、Rope は mint 実装ではなく
wallet 側のみなので依存の一致は必須ではない。

`hash_to_curve` の実装は crate 側に無いカスタムロジック
(`'02' || SHA256(...)` を candidate point として解釈し、有効な curve point
になるまで counter を回す) なので **自前実装が必要** — `k256::AffinePoint`
または同等の型に対する「バイト列から有効な圧縮点かどうか判定する」
API を確認すること。

---

## 3. 実装順序

### Step 1: `hash_to_curve` の実装 + 単体テスト (依存追加のみ、他コード変更なし)

- 新規関数、置き場所は `src/core/ecash.rs` (既存の secret 生成ロジックの隣)
  または新規 `src/core/bdhke.rs` (推奨 — BDHKE 計算をひとまとめにし、
  `ecash.rs`/`cashu_mint.rs` 両方から使えるようにする)。
- **テストベクタが必須**: NUT-00 仕様には公式テストベクタが付属している
  可能性が高い (`cashubtc/nuts` リポジトリの `00.md` 内、または
  `tests/` ディレクトリ) — 実装時に必ず一次ソースで確認し、
  自前実装の `hash_to_curve` が公式テストベクタと一致することを
  最初に検証する。ここが狂うと後続の署名検証が全て失敗する。

### Step 2: Blinding — `build_blinded_outputs` (`cashu_mint.rs:529`) の実装

- `r` (blinding factor) をランダム生成し、**`secret` と一緒に保持**する
  必要がある (現状の `Proof` 構造体には `r` を保持するフィールドが無い —
  追加が必要。`secret` フィールドの隣に `blinding_factor: String` (hex) 等)。
- `B_ = Y + rG` を計算し、wire format (`cashu_mint.rs` の既存 JSON 構造)
  に載せて mint へ送る部分は変更不要 (フィールド自体は既にある)。

### Step 3: Unblinding — `build_proof`/mint レスポンス処理 の実装

- mint からの `C_` レスポンスを受け取り、Step 2 で保持した `r` と mint の
  公開鍵 `K` (mint の keyset 情報、`Mint` 構造体に `pubkey` フィールド
  既存 — 要確認: 現在の `pubkey: String` が keyset ごとの `K` を正しく
  表現できているか) を使って `C = C_ - rK` を計算。
- ここで得た `C` を `Proof.c` に格納 — 現状 `build_proof` が
  blake3 プレースホルダを入れている箇所を丸ごと置き換える。

### Step 4: NUT-07 (`checkstate`) の実装

- `cashu_mint.rs:361-368` が「secp256k1 非依存のため未実装」としていた
  ブロッカーが Step 1-3 で解消されるため、`Ys` フィールド
  (`hash_to_curve(secret)`) を計算して送信できるようになる。
- 実装自体は Step 1 の `hash_to_curve` を呼ぶだけ (新規の暗号ロジック不要)。

### Step 5: DLEQ (NUT-12) — **優先度を引き上げ (2026-08-08)** / P2PK (任意のまま)

> **⚠️ 2026-08-08 優先度訂正**
> ([`RESEARCH_UPDATE_2026-08.md`](RESEARCH_UPDATE_2026-08.md) §4c):
> 本 Step は当初 DLEQ と P2PK をまとめて「任意、優先度低」としていたが、
> **NUT-12 (DLEQ) は Rope の構造上そうではない**。一次仕様
> (`cashubtc/nuts` `12.md`) を確認した結果:
>
> - DLEQ proof (`e`, `s`, ユーザー間転送時は `r`) により、**受信者は mint に
>   問い合わせず**に `R1 = s*G - e*A` / `R2 = s*B' - e*C'` /
>   `e == hash(R1,R2,A,C')` を検証でき、mint が発行を否認できなくなる。
> - **A7 (中断耐性) の前提**: `tick_stream` (`ecash.rs:982`) の 1 秒課金で
>   毎 tick mint に往復すると 60 秒ジョブで最大 60 往復。DLEQ があれば
>   受領時オフライン検証で往復を省ける (`CATEGORY_RESEARCH.md` §E2 が
>   既に指摘していたが、本 readiness の優先度に未反映だった)。
> - **A1 未達との相互作用**: Rope は NAT 越えを持たず実質 LAN 止まり (W2)。
>   **LAN 内では mint への到達性自体が保証されない**ため、DLEQ なしの ecash は
>   「mint に繋がらないと検証できない ecash」になる。A1 が制約されている
>   現状こそ DLEQ の価値が高い。
>
> **P2PK (NUT-11/28) は任意のままでよい** — escrow の鍵束縛強化は
> A5 が動いた後の話。DLEQ とは優先度が異なる。

- `docs/RESEARCH_IMPROVEMENTS.md` #4 および
  `docs/RESEARCH_UPDATE_2026-07.md` §3 で指摘した通り、escrow の鍵束縛
  強化に必要。Step 1-4 の BDHKE 基盤 (secp256k1 演算) が整って初めて
  着手可能になる後続タスク。

---

## 4. 影響範囲マップ

| ファイル | 変更内容 | リスク |
|---|---|---|
| `Cargo.toml` | `k256` を `[dependencies]` に追加 (推奨、§2参照) | 低〜中 (edition2024 汚染の再発リスクあり、`cargo add` 直後に即ビルド確認必須) |
| `src/core/bdhke.rs` (新規、推奨) | `hash_to_curve`/blinding/unblinding の純粋関数群。**I/O なし、`core/` の既存方針 (pure state machine) と整合** | 中 (新規暗号コード、テストベクタでの検証が必須) |
| `src/core/ecash.rs` | `Proof` 構造体に `blinding_factor` フィールド追加。`build_proof` のプレースホルダ部分を `bdhke::unblind` 呼び出しに置換 | 中 (既存の `Proof` シリアライズ形式が変わる — 永続化済み `ecash.json` との後方互換性を要検討、v0.2.11 で確立した「実データに存在し得ない値は安全に削除可」パターンの逆: **新規フィールド追加は `#[serde(default)]` 等で既存ファイルの読込を壊さないこと**) |
| `src/net/cashu_mint.rs` | `build_blinded_outputs` を実計算に置換。NUT-07 の `Ys` 実装 | 中 |
| `src/core/ecash.rs::SpentNullifiers` | **変更不要** — nullifier は blake3(secret) のままで正しい (BDHKEの`C`とは独立した仕組み) | — |

---

## 5. 注意点

- **テストベクタ厳守**: BDHKE はセキュリティクリティカルな暗号プロトコル。
  自前実装を「動いているように見える」だけで信用しない — NUT-00 公式の
  テストベクタ (存在確認は Step 1 の一部) と一致することを最優先で検証する。
  一致しない状態のコードは絶対にマージしない。
- **`Proof.c` のシリアライズ後方互換性**: 現行の 33バイト圧縮 point 形式
  (`c: String`, hex 66文字) は wire format としては既に NUT 準拠なので
  **`Proof` 構造体自体のフィールド定義は変更不要** — 中身の計算方法が
  変わるだけ。`blinding_factor` のような **一時的にしか要らない値**
  (unblinding が終われば不要) を `Proof` に永続化すべきか、それとも
  `mint_tokens`/`spend_proofs` のようなメソッド内のローカル変数に
  留めるべきかは設計判断が必要 — 後者の方が状態肥大化を避けられ推奨。
- **CDK との相互運用は未検証**: このドキュメントの数式は NUT-00 の
  一次仕様に基づくが、実際の mint 実装 (CDK 等) との実地テストは
  ネットワークアクセスが必要 (`http` feature 配下、実 mint への接続) —
  ビルド復旧後もこの検証には実在する Cashu mint (testnet 等) への
  アクセスが別途必要になる点に注意。

---

## 6. 完了の定義 (Definition of Done)

- [ ] `hash_to_curve` が NUT-00 公式テストベクタと一致する
- [ ] blinding → mint署名 (モックmintで可) → unblinding → 検証、の
      一連が secp256k1 演算として正しく動作する (`k*hash_to_curve(x) == C`)
- [ ] 既存の `ecash.rs`/`cashu_mint.rs` テスト (計 39+23件) が全てPASS、
      または placeholder 前提だったアサーションを実値に更新した上でPASS
- [ ] `ecash.json` の既存フォーマットとの読込後方互換性を確認
      (`config::load_or_recover` の破損復旧テストパターンと同様の
      「旧フォーマットのファイルを読ませて壊れずロードできる」検証)
- [ ] NUT-07 checkstate が実装され `net/cashu_mint.rs` のコメントから
      「未実装のまま」の記述を削除
- [ ] `SECURITY.md` の「まだ暗号学的に機能していないもの」表からBDHKE行を削除
- [ ] `docs/SURPLUS_AND_GAPS.md` §1.1 の該当箇所を「[DONE]」に更新
- [ ] **返金経路の proof 復元** (`docs/SURPLUS_AND_GAPS.md` §1.8): `close_stream`
      (`ecash.rs:1030`) / `refund_escrow` (`ecash.rs:837`, `簡略化` コメント) /
      `resolve_dispute` PayerWins・Split (`ecash.rs:883,890`) は現状
      `wallet.total_sats += 返金額` のみで proof バケットを復元しない。
      実 mint 結線後は返金を **実 mint swap による proof 再発行** に置換し、
      `total_sats == Σ proofs` の不変条件を返金後も維持すること
      (放置すると「表示されるが使えない残高」になる)。§1.8 を「[DONE]」に更新
- [ ] 実 mint (testnet) との相互運用を1回以上確認 (§5 参照)

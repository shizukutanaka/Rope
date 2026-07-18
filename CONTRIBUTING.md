# Contributing to Rope

Rope は「他人のアイドル GPU を、安全に、1 ドル未満で 60 秒借りる」ための
Rust 製 CLI です。貢献を歓迎しますが、このプロジェクトには他と少し違う
いくつかの明確な原則があります。最初に読んでください。

---

## まず知っておくべき現状 (正直な開示)

Rope は **状態機械としては完成しているが、エンドツーエンドでは動かない
スケルトン** です。実 P2P 通信・実暗号鍵交換 (Noise)・実 BDHKE 署名・
実 TEE attestation 検証・実 GPU 推論は **まだ配線されていません**
(QR ペアリングの blake3 keyed-MAC だけは本物)。

これは隠された欠陥ではなく、意図的に文書化された現状です。詳細:

- [`SECURITY.md`](SECURITY.md) — 何が本物の暗号で、何がまだプレースホルダか
- [`docs/ASSESSMENT.md`](docs/ASSESSMENT.md) — 長所 / 短所 / 改善点の評価
- [`docs/SURPLUS_AND_GAPS.md`](docs/SURPLUS_AND_GAPS.md) — 過不足の機械可読な一覧
- [`docs/P2P_IMPLEMENTATION_READINESS.md`](docs/P2P_IMPLEMENTATION_READINESS.md) /
  [`docs/CASHU_BDHKE_IMPLEMENTATION_READINESS.md`](docs/CASHU_BDHKE_IMPLEMENTATION_READINESS.md)
  — 次に実装すべき2大ギャップの実行手順書

「実際に動く」機能を追加する PR は、対応する readiness ドキュメントを
出発点にしてください。

---

## 設計原則 (PR はこれに沿うこと)

README の設計原則をコードにも適用します:

1. **Focus** — 4 動詞 (`rope` / `rope pair` / `rope run` / `rope earn`) のみ。
   新しいトップレベル動詞やサブコマンドの追加は、まず Issue で合意を取ること。
2. **Subtraction** — 「後で使うかも」で機能スロットを残さない。
   宣言だけで到達しないコードは追加しない。既存の未到達コードについては
   [`docs/REACHABILITY_AUDIT.md`](docs/REACHABILITY_AUDIT.md) を参照。
3. **正直なフォールバック** — 「できるフリ」をしない。未実装の機能は
   `capability_boundary!` マクロ (`src/main.rs`) で「今動く部分 / 次の版」を
   明示するか、docs で正直に開示する。CLI 出力・README・コミットメッセージが
   実態と食い違う変更はマージしません。

---

## 品質ゲート (マージ前に必ずローカルで通すこと)

CI 定義は [`.github/ci.yml.disabled`](.github/ci.yml.disabled) にありますが、
リポジトリのシークレットにアクセスするため現在は未有効化です
(有効化はメンテナ判断)。**それまでは各コントリビュータがローカルで
以下の全ゲートを通した上で PR を出してください**:

```bash
cargo build
cargo test --all-targets
cargo test --all-targets --features http
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features http -- -D warnings
cargo fmt --all -- --check
```

- **警告 = エラー**: `Cargo.toml` の `[lints]` で `unused_imports = "deny"`、
  `unsafe_code = "deny"` を crate 全体に強制しています。`clippy -D warnings`
  も必須です。
- **`unsafe` 禁止**: 新しい `unsafe` ブロックは原則マージしません。FFI 依存を
  伴う機能 (例: `secp256k1` C バインディング) を提案する場合は、pure Rust
  代替 (例: `k256`) を先に検討し、Issue で理由を説明してください。
- **MSRV 1.75**: `rust-version = "1.75"`。新規依存が推移的に `edition2024` を
  要求すると 1.75 でビルドが壊れます。`Cargo.toml` の上限固定パターン
  (`base64ct`/`getrandom`/`cpufeatures` のコメント参照) に倣ってください。

> **注記**: このプロジェクトの一部の履歴 (v0.2.14 前後) は、ネットワーク制約で
> `cargo` が動かない環境でレビューされ「コンパイラ未検証」と CHANGELOG に
> 明記されています。ビルド可能な環境で作業できる貢献者は、それらの範囲を
> 再検証する PR も歓迎します。

---

## テスト方針

- ロジックは `core/` の pure state machine に置き、単体テストを同ファイルの
  `#[cfg(test)] mod tests` に書きます。I/O (HTTP/socket) は `net/` に分離。
- バグ修正 PR には **その回帰を捕捉するテスト** を必ず添えてください。
- 安全性クリティカルな挙動 (例: `ConfidentialCompute` intent を非 TEE ピアへ
  ルーティングしない) は、feasible/infeasible の両方向をテストします
  (`src/core/intent.rs` の既存テスト群が手本)。

---

## コミット / PR

- コミットは論理単位で分割し、メッセージに「何を・なぜ」を書きます。
- 実態を変えない doc/コメントの純追記は歓迎ですが、コードの挙動が
  変わらないことを明記してください。
- 大きな機能 (実 P2P、実 BDHKE、検証エンジン、推論エンジン統合) は、
  着手前に Issue で設計を合意してから PR にしてください。対応する
  readiness ドキュメントがある場合はそれを更新する形が理想です。

---

## ライセンス

貢献物は [MIT ライセンス](LICENSE) の下で提供されたものとみなされます。

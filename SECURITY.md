# セキュリティポリシー

## 現状の暗号学的成熟度 (率直な開示)

Rope は「他人の GPU を安全に借りる」ことを謳うが、**現時点でセキュリティ上の実質的な
保証を提供する準備ができていない主要コンポーネントがある**。導入・評価する前に
以下を必ず把握すること。詳細な技術的根拠は [`docs/ASSESSMENT.md`](docs/ASSESSMENT.md)
および [`docs/RESEARCH_IMPROVEMENTS.md`](docs/RESEARCH_IMPROVEMENTS.md) を参照。

### 実際に暗号学的に機能しているもの

| コンポーネント | 実装 | 場所 |
|---|---|---|
| QR ペアリング HMAC | blake3 keyed-hash (定数時間比較) + nonce リプレイ防止 + 有効期限 | `src/core/pair.rs` |
| Capability トークン署名 | ed25519-dalek による実 ECDSA 相当署名・検証 | `src/core/session.rs` |
| 二重使用検出 (nullifier) | blake3 ハッシュによる実リプレイ検出 | `src/core/ecash.rs` |
| 永続化の破損検出 | JSON パース失敗時の自動バックアップ退避 + 初期状態復帰 | `src/core/config.rs` |

### まだ暗号学的に機能していないもの (プレースホルダ)

| コンポーネント | 実態 | 場所 |
|---|---|---|
| Cashu BDHKE 署名 (bearer token の値の裏付け) | secp256k1 楕円曲線演算を行わず、blake3 ダイジェストを署名の形にはめ込んだ決定論的プレースホルダ。**実際の金銭的価値を保証しない** | `src/core/ecash.rs`, `src/net/cashu_mint.rs` |
| TEE attestation 署名 | `unverified-digest:` prefix 付きの blake3 ダイジェスト。実 NVIDIA NRAS / RIM / OCSP 検証チェーンなし。**GPU が本当に TEE で動いている証明にはならない** | `src/core/confidential.rs` |
| Noise XX/IK ハンドシェイク | 状態機械のみ。実鍵交換 (x25519) / 実 AEAD 暗号化 (aes-gcm) は未実装 | `src/core/pair.rs` |
| P2P 通信そのもの | 実ソケット無し。mDNS/Bluetooth/DHT 発見・Noise 握手はすべて型のみのモデル | `src/core/pair.rs` |
| proof-of-execution (計算が実際に正しく行われた証明) | 未実装。実推論エンジン自体が未統合 | (未実装) |

**結論**: 上記の理由により、Rope は現時点で実際の金銭・機微データ・信頼境界を
懸けた本番運用に使うべきではない。README の 60 秒デモは実際の P2P/推論を
伴わないローカルシミュレーションである (詳細: README 冒頭の注記)。

## 信頼モデル

- ピアの信頼は TOFU (Trust On First Use) — `pair::TrustLevel::Unknown → Familiar
  → Trusted` の遷移は「以前成功したジョブがあるか」のみに基づき、初回接続する
  未知のピアに対する Sybil 耐性は無い。
- proof-of-capability (PoW ベースの `CapabilityChallenge`) は `pair.rs` に
  実装・テスト済みだが、現行の 4 動詞のどこからも呼ばれていない (v0.3 で
  結線予定の予約 API)。
- `Privacy::ConfidentialCompute` を要求する intent は、attestation 検証済みの
  TEE ピアにのみルーティングされ、非 TEE/未検証ピアへは resolver が拒否する
  (`core::intent` のガード)。ただしこの attestation 自体が上記の通り
  プレースホルダである点に注意。

## 依存関係とビルド

- `#![deny(unsafe_code)]` をクレート全体に強制。
- `cargo clippy -D warnings` / `cargo fmt --check` をローカルで実行 (CI 自動化は
  `.github/ci.yml.disabled` として定義済みだが、secrets アクセスを伴うため
  ユーザーの明示的判断待ちで未有効化 — 詳細は README の CI 節)。
- 依存クレートは `Cargo.lock` でピン留め。バージョン範囲は `edition2024` 要求を
  避けるため意図的に上限を設定している箇所がある (`Cargo.toml` のコメント参照)。

## 脆弱性の報告

Rope はまだ実運用前の段階のプロジェクトであり、専任のセキュリティチームは
無い。脆弱性を発見した場合は GitHub の Issue (機微でない場合) または
リポジトリの連絡先経由で報告してほしい。上記「プレースホルダ」コンポーネントに
関する既知の限界は本ファイルに既に記載済みのため、それ自体の再報告は不要 —
実装済みコンポーネント (QR HMAC、Capability 署名、nullifier 検出、永続化) に
対する具体的な攻撃が主な関心対象。

# セキュリティポリシー

## 現状の暗号学的成熟度 (率直な開示)

Rope は「他人の GPU を安全に借りる」ことを謳うが、**現時点でセキュリティ上の実質的な
保証を提供する準備ができていない主要コンポーネントがある**。導入・評価する前に
以下を必ず把握すること。詳細な技術的根拠は [`docs/ASSESSMENT.md`](docs/ASSESSMENT.md)
および [`docs/RESEARCH_IMPROVEMENTS.md`](docs/RESEARCH_IMPROVEMENTS.md) を参照。

---

## 🎯 v1 のセキュリティモデル (2026-08 スコープ決定)

[`docs/V1_SCOPE.md`](docs/V1_SCOPE.md) で v1 の要件を確定させた結果、
**v1 が提供しないものが明確になった**。以下は「未完成」ではなく
**v1 の設計上の境界**である (v2 へ延期)。

### v1 は機密計算 (Confidential Compute) を提供しない

- **`Privacy::ConfidentialCompute` と TEE attestation は v1 のスコープ外。**
  消費者 GPU に CC が存在しない (下記 W4 と同じ理由) ため、
  「他人の GPU で秘匿する」は v1 では成立しない。
- → **v1 では、貸し手はプロンプトと出力を見ることができる。**
  機微なデータを v1 の Rope で他人の GPU に送ってはならない。
- これは実装の遅れではなく、**成立しない約束を降ろした**もの。
  v2 で戻す際の根拠は [`docs/RESEARCH_UPDATE_2026-08.md`](docs/RESEARCH_UPDATE_2026-08.md) §4 にある。

### v1 の貸し手保護はプロセスレベルのみ — GPU レベルは保護しない

- 借り手のワークロードは**プロセス隔離**下で実行される (namespace / seccomp /
  Landlock 系)。しかし **GPU レベルの攻撃面は保護されない**:
  - GPU デバイスノード経由の権限昇格 (例: CVE-2026-22164)
  - 同一 GPU を共有するプロセス間の side channel (実行パターン・
    メモリアクセス時間・キャッシュ挙動からの推定)
- GPU 呼び出しに介在できるのは gVisor + nvproxy だが、**導入が必要でゼロコンフィグを
  壊す**ため v1 では採らない ([`docs/RESEARCH_UPDATE_2026-08.md`](docs/RESEARCH_UPDATE_2026-08.md) §4j)。
- → **貸し手は「GPU レベルでは無防備」であることを理解した上で貸すこと。**

### v1 は借り手指定の URI をフェッチせず、借り手指定の重みをロードしない

- `Workload::Train` / `Retrieve` は **v1 から削除**された。
  借り手が渡すのは**プロンプトとモデル名のみ**で、モデル名は
  **貸し手側の allowlist で解決される**。
- 結果として **SSRF 面と pickle デシリアライズ RCE 面が構造的に存在しない**
  ([`docs/SURPLUS_AND_GAPS.md`](docs/SURPLUS_AND_GAPS.md) §1.11 が挙げた 2 つの脅威は、
  防御ではなく**機能の削除**によって消えている)。
- モデル形式は **SafeTensors / GGUF に限定**する (pickle/`torch.load` は使わない)。

### v1 の推論エンジンが守る境界 (2026-08-18 実装)

`src/net/inference.rs` は**外部 crate を一切使わない** — pickle も動的ライブラリも
読まない。攻撃面は「ファイルを 1 つ読んで算術をする」だけ。

- **チェックポイントのヘッダを信用しない。** 全次元に上限を課し、**宣言された
  重みの個数とファイル長が完全一致することを確認してから**割り当てる。
  壊れた/細工されたファイルで巨大割り当て → OOM という経路を塞ぐ
  (`refuses_oversized_header_without_allocating` テスト)。
- **モデル名をパスに連結する前に無害化する** (`safe_model_stem`)。
  `../../etc/passwd` 等のパストラバーサルを拒否する
  (`safe_model_stem_rejects_path_traversal` テスト)。**モデルの置き場所は
  貸し手が決める** (`ROPE_MODEL_DIR` または `~/.rope/models`)。
- **トークナイザが語彙外の id を生成しない。** 語彙数がモデルと食い違う
  トークナイザは**ロード時に**拒否する (実行中に落ちるより早い)。
- **計算量に上限。** プロンプト長・生成トークン数・モデルの文脈長のいずれかで
  必ず停止する (`generation_stops_at_context_limit_not_by_panicking` テスト)。

⚠️ **これはプロセス隔離の代わりにはならない。** 依存ゼロで攻撃面が小さいことと、
サンドボックスに入れることは別の話である。v1 は前者だけを提供する。

### v1 は実行結果を検証しない

- **A4 (proof-of-execution) は v1 のスコープ外。** 借り手は「本当にそのモデルが
  走ったか」を暗号学的に確認できない。escrow の deadman 返金が唯一の保護。
- LAN 上の相手であることが実質的な緩和になっている前提の設計。

---

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

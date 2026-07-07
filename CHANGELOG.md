# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

**⚠️ この節の変更はコンパイラ未検証。** 今回のセッションのコンテナはネットワーク
ポリシーにより `static.crates.io`/`crates.io` への接続が 403 (policy denial) で
拒否され、かつ新規作成コンテナのため依存クレートキャッシュもゼロ — 既存の
依存関係すら再取得できず `cargo build` 自体が失敗する状態だった (`cargo build
--offline` は `anstream` 取得失敗で即エラー、`$HTTPS_PROXY/__agentproxy/status`
で `connect_rejected`/`policy denial` を確認)。従って以下は入念な手動レビューのみで
`cargo build`/`test`/`clippy`/`fmt` による検証を経ていない。ビルド可能な環境での
再検証が必須。

「市販レベルの品質」を目指す監査の一環として、新規依存を要さず・低リスクな
改善から着手した (詳細な現状評価は `docs/ASSESSMENT.md` 参照)。

### Changed
- `pair::PairManager::accept_pairing_token` — 外部/未信頼入力 (QR コード文字列) が
  起点となる経路上にあった `.expect("record_discovery が追加したピアが見つからない")`
  を `.context(...)?` に変更。`record_discovery` の実装が将来変わっても panic せず
  `Err` を返すようになる (現状は両者とも同じ理由で失敗しないため機能的な差は無い、
  防御的ハードニング)
- `docs/ARCHITECTURE.md` のモジュール別行数/テスト数テーブルが v0.2.5 時点のまま
  古かった (11,730行/232テスト) のを、v0.2.13 時点の実測値
  (11,948行/238テスト・244テスト`--features http`) に同期
- `README.md` の「60秒デモ」直後に、これが実 P2P/実推論ではなく
  `sample_haiku_response()` による固定応答のシミュレーションであることを明示する
  注記を追加。`docs/ASSESSMENT.md` には既に記載があったが、README のみを読む
  読者には伝わっていなかった

### Deferred — 明示的なユーザー確認が必要、または再検証可能な環境が必要
- **CI 有効化** (`.github/ci.yml.disabled` → `.github/workflows/ci.yml`):
  auto mode の許可分類器が「secrets アクセスを伴う自動パイプラインの起動に
  ユーザーの明示的同意が無い」として正しくブロックした。`.disabled` ファイル自体は
  改善済み (トリガーを `main` 限定から全ブランチ push/PR に拡張、`--features http`
  のテスト/clippy ジョブを追加) — 移動のみユーザー確認待ち
- **`#![allow(dead_code)]` の棚卸し** (`src/lib.rs`/`src/main.rs`):
  コンパイラのフィードバック無しに dead_code 判定を行うのは高リスクと判断し、
  今回は着手を見送り。ビルド可能な環境での次回実施を推奨
- `load_confidential()`/`load_pair()` の「破損 vs 未設定」区別: 調査の結果、
  既存の `config::load_or_recover` が既に両者を区別し、破損時は `eprintln!` で
  警告しファイルをバックアップした上で初期状態に倒す実装済みの挙動だった
  (`test_load_or_recover_corrupt_backs_up_and_defaults` でテスト済み)。
  修正不要と判断

### Added
- **`SECURITY.md`** — 従来存在しなかったセキュリティポリシー文書を新設。
  「本物として機能している暗号 (QR HMAC・Capability 署名・nullifier 検出・
  永続化の破損検出)」と「まだプレースホルダの暗号 (BDHKE・TEE attestation 署名・
  Noise 握手・P2P 通信そのもの)」を明確に分離して一覧化し、TOFU 信頼モデルの
  限界、脆弱性報告の窓口を記載。純粋なドキュメント追加でありコンパイル不要 —
  今回の環境制約下でも安全に実施可能だった項目

## [0.2.13] - 2026-07-01

v0.2.11/v0.2.12 で記録した Tier 3 (未構築 enum variant) の判断を実施。
enum variant の削除は struct field 削除と異なり後方互換性のリスク種別が違う
(未知フィールドは serde が無視するが、未知の enum 値は deserialize 失敗しうる) —
今回は全対象について「構築経路が一切存在しない」ことを確認済みのため、
実データがその値を持ちうるケースは無く安全と判断した。

### Removed

- `ecash::StreamState::{Opening, Paused, Closing}` — `open_stream` は常に直接
  `Active` を生成し、`close_stream` は直接 `Closed` に遷移する。pause/resume に
  相当する操作 (`pause_stream`/`resume_stream` 等) はそもそも存在しない。
  実際に使われる `Active`/`Closed`/`Stalled` の3値のみに削減
- `pair::DiscoveryMethod::Contact` — 比較・代入ともにゼロ。`AcceptMode::
  ContactsOnly` (「連絡先のみ」受付) は `trust_store` メンバーシップで判定して
  おり、この variant とは無関係だった

### Kept, documented — 「未結線の判定分岐」であり削除対象ではない

- `pair::TrustLevel::OwnDevice` — `is_capability_proven_or_trusted` で意味のある
  判定に使われているが、これを代入する仕組み (複数デバイス間のアイデンティティ
  紐付け) が無いため常にデッドブランチ。判定ロジック自体は将来の own-device
  リンク機能の自然な受け皿として妥当なため保持、doc comment でステータスを明記
- `confidential::SessionStatus::{Active, Suspended, Terminated}` —
  `create_secure_session` は `Establishing` のみ代入し、以降の遷移が無いため
  `update_stats().active_sessions` は常に 0。`TeeStatus::Running` (v0.2.11 で
  修正) と異なり、これは「実 TEE セッション確立」という v0.3 実 I/O 結線を
  待つ性質の未実装であり、同日修正可能な結線漏れではない — doc comment で
  区別を明記

### Changed
- テスト計 238 (default) / 244 (`--features http`) — 変化なし
  (削除対象 variant を参照するテストは元々存在せず)

## [0.2.12] - 2026-07-01

v0.2.11 で記録した Tier 2 (write-only 統計値) の追跡調査。今回は「削除」と
「表示配線」の両方が該当する具体例を発見し、それぞれの判断基準に従って処理した。

### Removed

#### `ecash::EcashStats.current_balance_sats` — `wallet.total_sats` の冗長ミラー
- 7 箇所の書込みサイトで `self.wallet.total_sats` をミラーしていたが、
  `format_ecash` を含めどこからも読まれない。しかも `format_ecash` は
  常に `wallet.total_sats` を直接表示しており、このミラーは同期ズレのリスクを
  負うだけで一切の価値を生んでいなかった。削除して7箇所の同期コードを除去

### Changed — write-only だった lifetime カウンタを表示に配線

`current_balance_sats` とは異なり、以下は個別の意味を持つ実データ
(単純なミラーではない) であるにもかかわらず表示されていなかったため、
削除ではなく `format_pair`/`format_ecash` への配線を選択:

- `pair::PairStats.total_handshakes_attempted` — `total_handshakes_succeeded`
  + `total_handshakes_failed` とは別に「開始されたが未解決」を捕捉できる値
- `pair::PairStats.total_peers_ever_paired` — 現在ペア数 (`currently_paired`)
  とは別の lifetime カウント
- `ecash::EcashStats.total_streams_opened` — escrow 同様の lifetime 開設数
  (streams は現在 `active` 数のみ表示していた)

回帰テスト2件追加 (`format_pair`/`format_ecash` が実際にこれらの値を含むこと)。

### Deferred (今回は対応せず、記録のみ)
- `pair::PairStats` の QR/TOFU 系フィールド (`tofu_accepts`,
  `pubkey_mismatch_rejections`, `qr_tokens_issued/consumed/expired`,
  `qr_hmac_rejections`, `qr_nonce_replays_blocked`) — テストでは検証済みだが
  `format_pair` 未表示。7個をまとめて表示に追加すると diagnostics 向けの
  情報がユーザー向けダッシュボードを圧迫する懸念があり、今回は見送り
- Tier 3 (未構築 enum variant: `StreamState::Opening/Paused/Closing`,
  `DiscoveryMethod::Contact`, `TrustLevel::OwnDevice`,
  `SessionStatus::Suspended/Terminated`) — 次回ロードマップ照合の上で判断

### Changed
- テスト計 238 (default) / 244 (`--features http`)

## [0.2.11] - 2026-07-01

全モジュール網羅の死蔵面監査 (Explore agent による Tier 1〜4 分類) を実施し、
機能バグ1件の修正と、ロードマップ裏付けのない死蔵フィールド5件の削除を実施。

### Fixed

#### `TeeStatus::Running` が一度も代入されず、期限切れ再検証が機能していなかった
- `TeeStatus::Running` は `refresh_expired_attestations`/`active_tee_count`/
  `update_stats` の3箇所で比較されるが、`perform_attestation` はこれまで
  `attestation_status` のみ更新し `status` フィールド自体は作成時の
  `Initializing` のまま放置していた。結果:
  - `active_tee_count()` は常に 0 を返していた (稼働中インスタンスがあっても)
  - `refresh_expired_attestations()` の `if status != Running { continue }`
    ガードが全インスタンスをスキップし続け、**期限切れ attestation の
    自動検出が一度も機能していなかった**
- 修正: `perform_attestation` 成功時に `status = TeeStatus::Running`、
  失敗時に `status = TeeStatus::Error` を代入。回帰テスト3件追加

### Removed — ロードマップ裏付けのない死蔵フィールド

- `intent::Intent.tags: Vec<String>` — 読取り経路ゼロ
- `intent::Intent.duration: Duration` + `Duration` 構造体一式 — 読取り経路ゼロ
  (`max_seconds`/`deadline` とも)
- `session::JobSpec` 構造体 — どこからも構築されない（テスト含めゼロ）
- `confidential::ConfidentialStats.failed_attestations` — 一度もインクリメントされず
- `confidential::ConfidentialStats.encrypted_data_gb` — 書込み・読取りともゼロ
- `first_run::FirstRun.is_repeat_user` + `FirstRunConfig.remember_first_run` —
  常にfalse固定/制御フラグ未読

いずれも `ecash::LightningLink` (v0.2.8) / `confidential::SecurityPolicy` (v0.2.10)
と同一パターン (書込み・読取り経路ゼロ、ロードマップ記載なし)。
`intent::RegionConstraint` は同種の未読フィールドだが `docs/RESEARCH_IMPROVEMENTS.md`
#13 に明示的統合計画があるため保持し、doc comment でステータスを明記。

後方互換性を実機検証: intent.json/confidential.json/first_run.json それぞれに
削除済みキーを注入し、いずれも「破損」リカバリを発火させず正常ロードすることを確認。

### Changed
- テスト計 236 (default) / 242 (`--features http`) (TeeStatus 回帰テスト+3、
  削除対象フィールドを参照するテストは元々存在せず差引ゼロ)

## [0.2.10] - 2026-07-01

前回 (v0.2.9) `SecurityPolicy` について「削除するか intent.rs に統合するか次回判断」
と明記した宿題を実施。

### Removed

#### `confidential::SecurityPolicy` サブシステムを削除
- `docs/RESEARCH_IMPROVEMENTS.md` にロードマップ記載が一切無く (`CapabilityChallenge`
  との対比で確認済み)、`intent::IntentManager` の TEE feasibility ゲートと
  同種の判定ロジックが重複していた。ロードマップの裏付けが無い以上「保持すべき
  予約 API」の根拠が無く、削除を選択 (統合という選択肢は「Intent にポリシー ID
  を持たせる」という新たなスキーマ拡張を要し、それを正当化する具体的な要求が
  無いため見送り)
- 削除対象: `SecurityPolicy` 構造体、`ConfidentialManager.policies` フィールド、
  `add_policy`/`validate_against_policy` メソッド、関連テスト3件
- 後方互換性を実機検証: 旧バージョンで保存された `confidential.json` に残る
  `"policies": [...]` キーは serde が黙って無視するため問題なくロードできる

### Changed
- テスト計 233 (default) / 239 (`--features http`) (-3, 削除対象のテストごと除去)

## [0.2.9] - 2026-07-01

前回 (v0.2.8) 「要判断」として持ち越した2件 (proof-of-capability, SecurityPolicy)
の削除/結線判断を実施。加えて `rope pair` 自体が QR を含む一切の実ピア動作を
実行しないことを確認した。

### Investigated (コード変更なしの判断)

#### `pair::CapabilityChallenge` / proof-of-capability — **保持と判定**
- `docs/RESEARCH_IMPROVEMENTS.md` #10 (Sybil耐性/評判、優先度「高」) に明示的な
  ロードマップ記載があり、実装・テストも完了済み。`rope pair` が実ピア発見を
  一切実行しない (QR も含め、mDNS/Bluetooth/QR いずれも v0.3 で実結線予定)
  ため現状無到達なだけであり、「削除すべき過剰機能」ではなく「正当に留保された
  v0.3 API」と判定。ステータスを doc comment に明記し、将来の誤削除を防止

#### `confidential::SecurityPolicy` — **判断を保留、設計上の重複を明記**
- proof-of-capability と異なり、ロードマップに一切記載が無い。さらに
  `intent::IntentManager` の TEE feasibility ゲート (`freshest_verified_instance`,
  v0.2.6) がこの機構を使わず、独自に「Verified かつ鮮度内」をハードコード判定
  しており、同種の判定ロジックが2箇所に分散している設計上の重複を発見。
  削除するか intent 側をポリシー経由に統合するか、次回判断が必要な旨を
  doc comment に明記 (この版では判断を下さず、状況を正直に記録するに留めた)

#### `rope pair` が QR ペアリングも含め一切の実ピア動作をしないことを確認
- `Pair` 動詞の doc comment は「AirDrop 式ピア発見 (mDNS / Bluetooth / QR)」と
  謳うが、`main.rs::run_pair` は `record_discovery`/`begin_handshake`/
  `accept_pairing_token` のいずれも呼んでいない。QR は実ネットワーク I/O 不要
  なため CLI に追加できないか検討したが、QR トークンに載せる `endpoint` 自体が
  実在しない (リスニングソケット未実装) ため、追加すれば「本物のピアに
  つながるふり」を新たに作るだけになると判断し、実装を見送った。
  TEE/ペアルーティング修正 (v0.2.6/v0.2.7) で確立した「本物でなければ正直に
  infeasible にする」原則と整合する判断

## [0.2.8] - 2026-07-01

ソクラテス式問答法を継続。前回 (v0.2.7) は「機能が足りない」側の発見だったが、
今回は「機能が過剰」側 — 宣言されているが誰にも参照されない構造体・分岐を探索した。

### Removed

#### `ecash::LightningLink` — 完全に不活性な構造体
- `EcashManager.lightning_links: Vec<LightningLink>` は 10 フィールドの Lightning
  ノード情報 (pubkey, alias, 交換レート, 入出金履歴) を持つ構造体だったが、
  **書き込み経路も読み込み経路も一切存在しなかった** (宣言・`Vec::new()` 初期化のみ、
  push も read も無し)。`proof-of-capability` (pair.rs, v0.2.7 で既出) とは異なり、
  テストすら無く、検証すらされていなかった純粋な死蔵データ。削除して確認:
  旧バージョンで保存された `ecash.json` に残る `"lightning_links": [...]` キーは
  serde が黙って無視するため、既存ユーザーの状態ファイルは問題なくロードできる
  (実機で検証済み)

### Changed
- テスト計 236 (default) / 242 (`--features http`) — 変化なし (LightningLink を
  参照するテストは元々存在しなかった)

### Known Limitations (今回のソクラテス式問答で新たに判明)
- `confidential::SecurityPolicy` / `add_policy` / `validate_against_policy`
  (v0.2.2 で鮮度チェックを追加した箇所) も、`pair.rs` の proof-of-capability と
  **全く同じパターン**で main.rs のどの動詞からも呼ばれていない。テストは充実して
  いるが、ポリシーを実際に登録・適用するユーザー操作が存在しない。
  1件だけなら偶然だが、2件目が見つかったことで「よく検証されているが
  完全に到達不能なサブシステム」がこのコードベースの局所的パターンではなく
  構造的な傾向であることが示唆される — 次回は削除/結線の判断をまとめて行うべき

## [0.2.7] - 2026-07-01

ソクラテス式問答法で機能の過不足を検証。「TEE ルーティング修正 (v0.2.6) は同じ
根本原因を持つ兄弟分岐すべてに一貫適用されたか？」を問うたところ、修正が不十分
だったことが判明した。

### Fixed

#### `select_provider` の非TEE分岐も実ピア状態を無視していた (v0.2.6 の修正漏れ)
- v0.2.6 で `Privacy::ConfidentialCompute` (TEE) 分岐のみ `ConfidentialManager` の
  実状態を参照するよう修正したが、同一関数内の他の `FederatedPeer` 分岐
  (`EnergyPreference::MinimizeWatts`, `RenewableOnly`/`PreferRenewable`,
  `OnDevicePreferred`/`FederatedOnly` の一般ケース) は `"low-energy-peer"`,
  `"renewable-peer"`, `"peer-1"` という固定文字列を返し続けていた —
  `rope pair` が実際に発見・ペアした `PairManager.paired` を一切参照しない、
  TEE分岐と全く同じ欠陥パターン
- `IntentManager::resolve()` が `Option<&PairManager>` も受け取るように変更。
  `pick_federated_peer()` ヘルパーを新設し、実ペアリング済みピアがあればその ID へ
  ルーティング、無ければ trust_score=0.0 の番兵値を返す (TEE分岐と統一された規約)
- `check_feasibility` の TEE 専用チェックを、`ProviderChoice::FederatedPeer` の
  `trust_score <= 0.0` を検出する一般ルールへ統合。TEE/非TEE 両方の
  「委任先ピアが見つからない」ケースを1箇所で確実に infeasible にする
  (メッセージは文脈に応じて TEE 向け/一般向けを出し分け)
- 回帰テスト2件追加 (ペアリング済みピア無しで infeasible / 実ピアへ正しくルーティング)

### Changed
- テスト計 236 (default) / 242 (`--features http`)

### Known Limitations (今回のソクラテス式問答で判明、未対応)
- `pair.rs` の proof-of-capability / reputation サブシステム
  (`CapabilityChallenge`, `issue_pow_challenge`, `verify_capability_proof`,
  `reputation_score` 等、約300行) はテストで検証済みだが、`rope pair` からの
  呼び出しがゼロ — 完全に到達不能な状態。プロジェクト自身の「約束されてない機能の
  ためにスロットを残さない」哲学と緊張関係にある。削除するか `rope pair` に
  結線するか、次回判断が必要
- `intent::RegionConstraint` は宣言・デフォルト設定のみで、resolver のどこからも
  参照されない dead field のまま (プロバイダの地域メタデータ基盤が無いため)
- `net::cashu_mint` に `melt()` 相当の関数は名前空間にすら存在しない
  (「未実装」ではなく「スタブすら無い」— より正確な記述)

## [0.2.6] - 2026-07-01

### Fixed

#### `intent::select_provider` が実 TEE 状態を一切参照しなかった (最重要・複数回のレビューで指摘)
- 4 レビューラウンド (v0.2.2〜v0.2.5) で繰り返し指摘されていた設計ギャップを解消。
  `Privacy::ConfidentialCompute` (機密計算) の provider 選択が、`ConfidentialManager`
  の実 attestation 状態を一切見ずに固定のプレースホルダー peer
  (`"tee-peer"`, trust_score=1.0) を常に返していた。ユーザーが自分の Intent に
  `verification: Attested` を設定しさえすれば (実ピアの検証状態と無関係に)
  `check_feasibility` を通過してしまい、検証済み TEE がゼロ個でも「feasible」な
  実行計画が組めていた
- `ConfidentialManager::freshest_verified_instance()` を新設 (Verified かつ
  `attestation_interval_seconds` 以内の鮮度を持つインスタンスのみ返す —
  `create_secure_session`/`validate_against_policy` と同一基準を共有)
- `IntentManager::resolve()` が `Option<&ConfidentialManager>` を受け取るように変更。
  `select_provider` は実インスタンスが見つかった場合のみそのピアへルーティングし、
  無ければ trust_score=0.0 の番兵値を返す。`check_feasibility` 側でも独立に
  「検証済み TEE 無し」を検出し infeasible にする (ルーティング層とフィージビリティ層で
  二重に安全側へ倒す設計)
- `rope run` に `--verification (none/attested/zk/full)` フラグを新設。
  旧実装ではこの経路自体が CLI から到達不可能だった (常に verification=None の
  既存ゲートで止まっていた) ため、新フラグを追加して実際に検証可能にした
- 回帰テスト 4 件追加 (実 Verified TEE で feasible+正しい peer へルーティング /
  TEE 無しで infeasible / attestation 期限切れで infeasible / 旧テストの是正)

### Changed
- テスト計 234 (default) / 240 (`--features http`)
- clippy `--all-targets -- -D warnings` (両 feature) を完全クリーンに維持

### Known Limitations (今回判明した環境制約)
- **この実行環境では crates.io から新規依存パッケージを追加できない**:
  エグレスポリシーが `static.crates.io` (実クレートダウンロード先) への接続を
  403 で拒否する (`index.crates.io` のメタデータ取得は許可されている非対称な設定)。
  これにより Cashu の本物の BDHKE (secp256k1) 実装や libp2p/snow による P2P 実結線は、
  この環境では技術的に実施不能と判明した。将来これらに着手する場合は、
  crates.io への完全なアクセスを持つ環境が必要

## [0.2.5] - 2026-07-01

### Fixed

#### `examples/library_usage.rs` was 100% inert placeholder text
- 全3フロー (pair/ecash/confidential) が丸ごとコメントアウトされた擬似コードで、
  `cargo run --example library_usage` は "詳細は src/core/*.rs を参照" と
  表示するだけの空実行だった。README は「core/ ライブラリ使用例」と謳っていたが、
  実際にコピペで動くコードは一切無かった
- **根本原因**: `Cargo.toml` に `[lib]` ターゲットが無く (`src/lib.rs` 不在)、
  `core`/`net` は `main.rs` の private module としてのみ存在していた。
  examples/ はパッケージの lib クレートしか import できないため、
  lib ターゲットが無い限りこの例は**原理的に実コードを書けなかった**
  (README が謳う「テスト・組み込み・外部ツール統合に最適」は、実際には
  lib ターゲットが存在せず外部から embed 不可能だった)
- 修正: `src/lib.rs` を新設し `pub mod core; pub mod net;` を宣言。
  `main.rs` は `mod core; mod net;` (再宣言) ではなく `use rope::core;`
  (import) に変更し、module tree の二重コンパイル・テスト二重実行を回避
  (テスト総数は 232/238 のまま不変で確認済み)。`core::short()` を
  `pub(crate)` → `pub` に昇格 (クレート境界を跨ぐため)。
  `library_usage.rs` を実際に `cargo run --example` で動く実行可能コードへ
  全面書き換え (assert 付き、3フローとも実際に mgr を操作)

#### `cargo install rope` は実際には動かなかった
- crates.io に **既に無関係な別プロジェクト** ("rope" という文字列データ構造、
  yanked 済み、`github.com/epsilonz/rope.rs`) がこの名前を使用しており、
  再利用不可能。README の最初の「インストール」手順が実際には機能しない
  (もしくは無関係なパッケージを指す) 状態だった。ソースからの
  `cargo install --path .` 手順に置換 (`README.md`, `examples/quickstart.sh`)

### Changed
- clippy が `SessionManager::new()` に `new_without_default` を新たに検出
  (lib ターゲット新設により本当に public API になったため)。
  `impl Default for SessionManager` を追加して解消

## [0.2.4] - 2026-07-01

ドキュメントの実態不一致を対象にした監査。コード変更は無し。

### Fixed

#### CI が一度も実行されていなかった (発見のみ、有効化は保留)
- `.github/ci.yml.disabled` は import 時 (v0.1.0) からこの名前・場所にあり、
  `.github/workflows/` 直下に置かれていないため **GitHub Actions が一度も認識・
  実行していなかった**。README/CHANGELOG が一貫して謳ってきた「CI green」は
  実際にはローカルでの `cargo test`/`clippy`/`fmt` 実行結果であり、CI による
  自動ゲートではなかった。有効化 (`.github/workflows/ci.yml` への移動) は
  リポジトリシークレットにアクセスする自動パイプラインを起動する操作のため、
  本セッションでは承認が得られず保留 — 実施するかはユーザー判断。
  表記を実態に合わせて訂正 (README, `docs/ASSESSMENT.md`)

#### 壊れたドキュメントリンク
- README が参照する `docs/internal/APPLE_METHOD.md` と `docs/internal/` は
  存在しない (import 時から欠落)。`docs/ARCHITECTURE.md` のディレクトリ構造図も
  存在しない `docs/internal/`, `docs/archive/`, `examples/job.yaml` を記載していた。
  実在するファイル (`docs/ASSESSMENT.md`, `docs/RESEARCH_IMPROVEMENTS.md` 等) への
  参照に置き換え、存在しないパスの記載を削除

### Changed
- `docs/ASSESSMENT.md` を v0.2.2/v0.2.3 の修正内容で更新 (改善点テーブル、
  永続化の実態記述を「チェックポイント止まり」へ精緻化 — 完全な `atomic_write`/
  `load_or_recover` インフラは健全だが、動詞内の個別状態遷移ごとの保存はまだ無い)

## [0.2.3] - 2026-07-01

前回 (0.2.2) がカバーしなかった領域 (net/cashu_mint.rs の未結線 HTTP 翻訳層、
session.rs/config.rs の再監査、ドキュメント整合性) を対象にした追加レビュー。

### Fixed

#### net::cashu_mint — NUT-07 wire format 不一致 (v0.3 結線時に即失敗するバグ)
- **checkstate リクエスト/レスポンスのフィールド名が仕様と不一致**: 旧実装は
  `{"secrets": [...]}` を送信し、レスポンスの proof 識別子を `secret` として
  パースしていたが、現行 NUT-07 は `Ys` (リクエスト) / `Y` (レスポンス) —
  各要素は `hash_to_curve(secret)` の結果であり生 secret ではない。
  準拠 mint に接続すると 400 か state 不一致で二重使用検出が機能しなくなる
  欠陥だった。wire field 名を仕様に合わせて修正 (`CheckStateRequest.ys`,
  `ProofState.y`、共に `#[serde(rename)]` で正しい JSON key を維持)
- **mint 応答の C (blind signature) が無検証で Proof に埋め込まれていた**:
  `translate_signatures_to_proofs` は額面・keyset は検証済み (問⑲) だったが、
  `sig.c_` の形式 (66 hex 文字、02/03 prefix の圧縮 secp256k1 point) は
  未検証のままコピーしていた。空文字列や壊れた C を返す mint (バグ/悪意) が
  下流に「妥当な署名」と誤認される Proof を生成できた。形式検証を追加

### Changed
- 回帰テスト **8 件追加** (NUT-07 wire field, C 形式検証, その他) —
  テスト計 232 (default) / 238 (http feature)
- `docs/ARCHITECTURE.md` の全モジュール行数・テスト数表を実測値へ再生成
  (2026-04 時点の数値から大幅に乖離していた)
- README の「exit(1): 0 コマンド」表記を実態に合わせて訂正。v0.2.2 で
  graceful 化したのはロック競合・孤児セッション掃除失敗のみであり、
  致命的 I/O 障害 (disk full 等) は引き続き正直に exit(1) する
  (黙って成功したふりをする方が危険なため、これは意図した挙動)

### Known Limitations (v0.3 スコープ — 今回も未対応、より正確に記載)
- `intent::select_provider` の TEE ルーティングは実際の `ConfidentialManager` 状態を
  参照せず、常にプレースホルダ peer を返す (コード内に "Seam: v0.3" として既存の記載あり)
- `pair.rs` の discovery 層はピアの `advertised_pubkey` を自己申告のまま dedup キーに
  使っており、実 Noise 暗号ハンドシェイクが結線されるまで pubkey squatting に対する
  防御がない (実 P2P I/O 自体が v0.3 スコープ)
- `net::cashu_mint` は `/v1/melt/bolt11` (NUT-05 実行) が未実装なだけでなく、
  **unblinding が本物の BDHHKE ではない** (`translate_signatures_to_proofs` は
  mint の blind signature `C'` をそのまま `Proof.c` に格納しており、実際の
  ec point 減算による unblind 処理を行っていない — 依存追加ゼロ方針のため
  secp256k1 演算を実装していない)。今回 wire format とレスポンス検証は修正したが、
  これは「プロトコルとして繋がる」ことの前提整備であり、「暗号学的に正しい
  Cashu token を生成する」ことは依然として v0.3 (secp256k1 依存追加) 待ち。
  モジュール自体もどこからも呼び出されていない (未結線)

## [0.2.2] - 2026-07-01

4 並列レビューエージェントによる全モジュール精査 (ecash/mint, intent/confidential,
pair/session, config/first_run/main) から新たに見つかった欠陥を修正。

### Fixed

#### 資金消滅バグ (ecash.rs)
- **receive_proofs の二重加算** (問㉑): 受信した proof の nullifier を記録していなかった
  ため、同一 proof を1バッチに2つ含める、または別呼び出しで再提示するだけで残高が
  水増しされた。バッチ内重複検知 + nullifier 記録を追加
- **lock_funds のオーバーシュート消滅** (問㉒): escrow/stream 開設時、2冪額面 proof が
  target を超えて消費されると、超過分が `total_sats` から差し引かれずに proof ごと
  破棄され価値が消滅していた (例: [1,4] proof から 3 sats ロックで 2 sats 消滅)。
  超過分を「お釣り」proof として bucket に戻し、価値を保存するよう修正

#### 契約違反: graceful exit(0) (main.rs)
- **LockGuard 取得失敗が exit(1) を引き起こす**: `rope pair`/`rope run`/`rope earn` の
  ロック取得失敗 (別プロセスが実行中、read-only fs 等) が `?` で伝播し、
  「crash 絶対に出さない」という capability_boundary の方針に反して exit(1) していた。
  友好的メッセージ表示後 graceful に exit(0) するよう修正
- **prune_stale の I/O エラーが exit(1) を引き起こす**: セッションディレクトリ読取失敗
  (権限変更、NFS 障害等) が非本質的操作にもかかわらず crash を引き起こしていた。
  non-fatal 化 (警告表示のみで続行)

#### セキュリティ強化
- **QR ペアリング nonce のリプレイ未検出** (pair.rs): `PairingTokenPayload.nonce` は
  生成されるが一度も照合されておらず、有効期限内であれば同じ QR を何度でも再提示できた。
  期限に連動して自然に縮小する bounded nonce store を追加し、リプレイを拒否
- **TEE attestation ポリシー検証の鮮度チェック抜け** (confidential.rs):
  `validate_against_policy` は `create_secure_session` と異なり `last_attestation` の
  鮮度を確認しておらず、`refresh_expired_attestations()` が呼ばれるまで期限切れの
  attestation でもポリシーを通過できた。鮮度チェックを共通化し両経路で一貫させた

#### 防御的堅牢化
- ID 表示のバイトスライス切り詰め (`&s[..n.min(len)]`) を UTF-8 境界安全な
  `core::short()` へ統一 (main.rs, confidential.rs, first_run.rs)。
  現状は内部生成 UUID のみで実害は無いが、既存の同型バグ (問⑧⑨等) と同じ
  パターンのため防御的に統一

### Changed
- 回帰テスト **7 件追加** (receive_proofs 二重加算×2、lock_funds 価値保存、
  QR nonce リプレイ、attestation ポリシー鮮度) — テスト計 230 (default)
- `mint_tokens`/`lock_funds` の proof 構築ロジックを `build_proof`/`derive_keyset_id`
  へ共通化 (重複コード削減)
- clippy `--all-targets -- -D warnings` を完全クリーンに維持
- `cargo fmt --all -- --check` パス

### Known Limitations (v0.3 スコープ — 今回は未対応)
- `intent::select_provider` の TEE ルーティングは実際の `ConfidentialManager` 状態を
  参照せず、常にプレースホルダ peer を返す (コード内に "Seam: v0.3" として既存の記載あり)
- `pair.rs` の discovery 層はピアの `advertised_pubkey` を自己申告のまま dedup キーに
  使っており、実 Noise 暗号ハンドシェイクが結線されるまで pubkey squatting に対する
  防御がない (実 P2P I/O 自体が v0.3 スコープ)
- `net::cashu_mint` の `/v1/melt/bolt11` (NUT-05 実行) は未実装、かつモジュール全体が
  どこからも呼び出されていない (実 mint HTTP 接続は v0.3 スコープ)

## [0.2.1] - 2026-06-30

### Fixed

#### Round 5: ecash.rs の 3 欠陥
- **SpentNullifiers sliding window double-spend 攻撃** (問⑪): FIFO eviction を廃止し
  容量到達時に `overflow=true` で拒否。100k を埋めて evicted nullifier を再提示する攻撃を
  防止。nullifier 履歴は完全に保持
- **proof_satisfies の条件パース誤り** (問⑫): `rfind("==")` → `find("==")`。
  `"label==value1==value2"` の条件が正しく `"value1==value2"` 全体を expected として処理
- **tick_stream elapsed のローカルクロック悪用** (問⑬): `max_tick_interval_seconds` を
  追加 (デフォルト 60 秒)。elapsed を capped して 5 分放置での 3,000 sat 一括消費を防止

#### Round 6: session.rs の「完成して見える」安全プリミティブ硬化
- **panic_stop の緊急停止信頼性** (問⑭): docker 失敗で早期 return → cleanup も呼ばない
  バグを修正。best-effort 化して必ず `clear_session_files()` に到達。フラグも立てたまま残す
- **Capability token の未検証** (問⑮): ed25519 署名検証を実装。`sign()` / `verify_signature()` /
  `is_expired()` / `authorizes()` / `validate()` の 5-gate で完成。署名素材は JSON 配列正規化

#### Round 7a: confidential.rs + intent.rs の「リモートを無検証で信頼」の欠陥
- **attestation 新鮮度チェック遅延** (問⑯⑰): `create_secure_session()` で now - last_attestation ≤
  attestation_interval_seconds を直接検証。refresh_expired_attestations() に依存しない
- **replay 検出の固定 60 秒ウィンドウ** (問⑰): `attestation_interval_seconds` に連動。
  slow-drip 攻撃 (13 秒ごとに小額実行) を防止、warning にも表示
- **RenewableOnly/PreferRenewable がルーティングに未反映** (問⑱): `select_provider()` に
  renewable branch 追加。OnDevicePreferred+Small は LocalDevice、renewable 要件は FederatedPeer に

#### Round 7b: cashu_mint.rs + config.rs の実 I/O 経路の検証欠落
- **mint 応答の額面・keyset 未検証** (問⑲): `translate_signatures_to_proofs()` に
  `expected_amounts: &[u64]` を追加。位置ごとに `sig.amount == expected[i]` と
  `sig.id == keyset_id` を検証。mint が 8→64 sat すり替え や別 keyset 署名を拒否
- **stale lock 検出の liveness バグ** (問⑳): `try_reclaim_stale()` を毎試行で呼び出す。
  PID を再読込して二重奪取レースを防ぎ、live lock は保護。dead holder は即座に奪取

### Security
- **QR ペアリング MAC を blake3 keyed-hash に置換**: 非暗号学的な FNV を MAC
  として使っていたため payload 偽造を防げなかった。`blake3::keyed_hash`
  (正式な keyed MAC) へ置換、依存追加なし (blake3 は既存依存)
  (`src/core/pair.rs`)

### Changed
- 回帰テスト **19 件追加** (SpentNullifiers overflow / rfind separator / tick capping /
  panic_stop cleanup / Capability 署名/失効/限度 / attestation freshness / mint amount / lock liveness)
  — テスト計 225 (default) / 231 (http feature)
- clippy `--all-targets -- -D warnings` を完全クリーンに維持
- `cargo fmt --all -- --check` パス
- README の数値更新 (main.rs 574 行、core 7 モジュール、テスト数)

## [0.2.0] - 2026-04-18

### Added
- **4 動詞 CLI**: `rope` (first-run) / `rope pair` / `rope run` / `rope earn`
- **7 core モジュール**: config, pair, confidential, ecash, intent, session, first_run
- **1 net モジュール**: cashu_mint (Cashu NUT-00/01/04/05/06/07 HTTP クライアント)
- **174 単体テスト** (cargo test 実通過、CI green)
- プロセス間ロック (G5): 並行 run の更新喪失を防止、stale lock 自動回収
- atomic write: 全 8 save が tmp→rename、書込み中断でも破損しない
- 秘密鍵を作成時点で 0o600 (umask 無関係、TOCTOU 窓なし)
- attestation digest を unverified-digest: と明示 (placeholder を本物の署名と誤認しない)
- Cashu NUT-00 準拠 proof 型 (BLAKE3 nullifier, 33-byte compressed point C)
- GPU TEE attestation (5-step validation: TEE×GPU compat, memory encryption, security level, replay detection, evidence hash)
- AirDrop 式ピア発見 state machine (mDNS / Bluetooth / QR / DHT / Direct)
- Noise XX/IK 握手 state machine (TOFU 信頼ストア, MITM 検知)
- ecash escrow + streaming (deadman 自動返金, 1 秒単位課金)
- 60 秒 wow moment orchestrator (9 段階 state machine)
- Apple-style capability boundary (全動詞 graceful exit(0))
- `[lints.rust]` unsafe_code = deny, unused_imports = deny
- Cashu mint ↔ ecash 翻訳層 (translate_signatures_to_proofs, build_blinded_outputs)

### Removed
- 115 モジュール (122 → 7, -94%)
- main.rs 379 CLI コマンド (→ 4 動詞)
- 11,200 行 main.rs (→ 389 行, -97%)
- wizard / troubleshoot / report CLI コマンド
- 41 孤立モジュール (呼出ゼロ dead code)

### Architecture
- core/ = pure state machine (I/O なし)
- net/ = HTTP / wire format (I/O 層)
- Apple Foundation/AppKit 分離パターン準拠

## [0.1.0] - 2026-03-01

### Added
- Initial GPU lending prototype (122 modules, 11,594-line main.rs)

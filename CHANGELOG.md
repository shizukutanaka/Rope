# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

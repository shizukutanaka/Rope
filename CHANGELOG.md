# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

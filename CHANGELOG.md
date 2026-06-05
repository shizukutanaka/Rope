# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] - 2026-06-05

### Fixed
- **streaming 課金のオーバーフロー耐性**: `tick_stream` / `close_stream` の
  `elapsed * rate` と残額計算を `saturating_mul` / `saturating_sub` 化。
  クロックスキューや長時間放置でも panic せず remaining で頭打ち
  (`src/core/ecash.rs`、同モジュール内の他箇所と一貫)
- **intent history のパニック修正**: `complete()` の `drain(..100)` は
  `max_history < 100` で範囲外 panic していた。超過分のみ drain するよう修正
  (`src/core/intent.rs`、`submit` 側の既存実装と一貫)
- **整数オーバーフロー防御**: `target_ms * 4` と `num_items * tokens` を
  `saturating_mul` 化 (`src/core/intent.rs`)
- **UTF-8 境界 panic 修正**: 表示用 ID 切り詰めの `&s[..n]` バイトスライスを
  char ベースの共通ヘルパー `core::short` に統一 (intent / confidential /
  session)。マルチバイト ID でも panic しない
- **クロスプラットフォーム stale lock 検出**: `LockGuard` の生存判定は Linux
  専用の `/proc/<pid>` のみで、非 Linux では全ロックを stale と誤判定して
  相互排他を壊していた。非 Linux 向けに mtime ベースの保守的フォールバックを
  追加 (`src/core/config.rs`)

### Security
- **QR ペアリング MAC を blake3 keyed-hash に置換**: 非暗号学的な FNV を MAC
  として使っていたため payload 偽造を防げなかった。`blake3::keyed_hash`
  (正式な keyed MAC) へ置換、依存追加なし (blake3 は既存依存)
  (`src/core/pair.rs`)
- **attestation evidence digest の強化**: FNV を blake3 に置換し、report ごとの
  nonce を含めて再 attestation でも digest が変わるようにした (リプレイ識別性)。
  実 TEE 署名検証は引き続き v0.3 で結線、`unverified-digest:` プレフィックス維持
  (`src/core/confidential.rs`)

### Changed
- clippy `--all-targets -- -D warnings` を完全クリーンに (manual_div_ceil /
  map_or / doc list indentation の 4 警告を解消)
- 回帰テスト 4 件追加 (streaming saturating, history drain panic, multibyte
  display, attestation nonce 識別性) — 計 178 テスト
- README / ステータスの数値を実測へ同期 (main.rs 行数、テスト数)

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

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

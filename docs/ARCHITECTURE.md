# Rope アーキテクチャ

*7 モジュール + 1 net レイヤ — 旧 122 モジュールから -94%*

---

## 全体図

```
┌─────────────────────────────────────────────────────┐
│  CLI (main.rs, 530 行)                              │
│                                                     │
│  rope          → first_run (wow moment)             │
│  rope pair     → pair + session (mDNS / QR / Noise) │
│  rope run MODEL → intent (resolve → plan → execute) │
│  rope earn     → pair + session + ecash (streaming)  │
│                                                     │
├─────────────────────────────────────────────────────┤
│  core/ (pure state machine, I/O なし)               │
│                                                     │
│  ┌───────────┐  ┌───────────┐  ┌───────────────┐   │
│  │ first_run │──│   pair    │──│ confidential  │   │
│  │ (9 stage) │  │ (mDNS/QR)│  │ (TEE attest)  │   │
│  └─────┬─────┘  └─────┬─────┘  └───────────────┘   │
│        │              │                              │
│  ┌─────▼─────┐  ┌─────▼─────┐  ┌───────────────┐   │
│  │  intent   │  │  session  │  │    ecash      │   │
│  │ (resolve) │  │ (lifecycle)│  │(Cashu+escrow) │   │
│  └───────────┘  └───────────┘  └───────────────┘   │
│                                                     │
│  ┌───────────────────────────────────────────────┐  │
│  │              config (鍵, 設定, ディレクトリ)     │  │
│  └───────────────────────────────────────────────┘  │
│                                                     │
├─────────────────────────────────────────────────────┤
│  net/ (I/O 層, HTTP / wire)                         │
│                                                     │
│  ┌───────────────────────────────────────────────┐  │
│  │ cashu_mint (NUT-01/04/05/06/07 + 翻訳層)     │  │
│  └───────────────────────────────────────────────┘  │
│  ┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┐  │
│  │ v0.3 予定: discovery.rs / noise.rs / gpu.rs  │  │
│  └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┘  │
│                                                     │
└─────────────────────────────────────────────────────┘
```

---

## モジュール一覧 (7 core + 1 net)

### core/ — pure state machine

| モジュール | 行数 | テスト | 責務 |
|-----------|-----:|------:|------|
| **config** | 396 | 8 | 鍵生成 (Ed25519), 設定, ディレクトリ管理 |
| **pair** | 1,211 | 28 | ピア発見 (mDNS/BT/QR/DHT), Noise XX/IK 握手, 信頼ストア |
| **session** | 607 | 14 | セッション状態機械, verify code, panic stop |
| **confidential** | 1,197 | 24 | TEE attestation (H100/H200/Blackwell), 暗号セッション, ポリシー |
| **intent** | 1,257 | 22 | Intent → ExecutionPlan 解決, 予算/遅延/プライバシー制約 |
| **ecash** | 1,375 | 30 | Cashu NUT-00 proof, escrow (6状態), streaming (1秒単位) |
| **first_run** | 1,069 | 19 | 60秒 wow moment (9 stage state machine) |

### net/ — I/O 層

| モジュール | 行数 | テスト | 責務 |
|-----------|-----:|------:|------|
| **cashu_mint** | 690 | 18 | Cashu HTTP client (NUT-01/04/05/06/07), 翻訳層 |
| **main.rs** | 530 | 13 | CLI 引数解析, 4 動詞ルーティング, 統合テスト |

---

## 設計原則

### Apple 流分離

```
core/ は Foundation — 型, 状態遷移, 検証ロジック
net/  は AppKit     — HTTP, ソケット, wire 形式
main.rs は アプリ   — CLI 引数解析, 合成, 出力
```

core は `reqwest` も `tokio` も import しない。pure sync Rust。
テスト時に I/O モック不要。

### Carmack / Martin / Pike

- **Carmack**: HashMap/Vec 中心、trait 最小化、enum state machine
- **Martin**: 単一責任 (1 モジュール = 1 責務)、Open/Closed
- **Pike**: anyhow::Result 統一、明示的 async なし (core 全域 sync)

---

## 状態機械マップ

### first_run (9 stage)

```
Welcome → IdentityReady → Discovering → Paired
  → AttestVerified → DemoIntentCreated → DemoCompleted
  → Finale → Done
       ↘ (任意地点から) Aborted
```

### pair / handshake (5 state)

```
Initializing → EphemeralSent → ResponderIdentified → Established
  ↘ Failed / TimedOut (終端, advance 拒否)
```

### ecash / escrow (6 state)

```
Deposited → InProgress → Released (→ アーカイブ)
  ↘ Refunded (deadman 自動)
  ↘ Disputed → Resolved (mint 調停)
```

### session (5 state)

```
Idle → Waiting → Connected → Active → Terminated
  ↘ (Idle以外から) Terminated
```

---

## データフロー (60 秒デモ)

```
1. rope                    → first_run::new()
2. 鍵確認                  → config::ensure_initialized()
3. ピア探索                 → pair::record_discovery()
4. 握手                    → pair::begin_handshake() → advance × 3
5. TEE 検証                → confidential::perform_attestation()
6. Intent 構築             → intent::submit() + resolve()
7. (demo) haiku 表示       → first_run::sample_haiku_response()
8. 完了画面                → first_run::format_welcome_letter()
```

---

## ディレクトリ構造

```
rope/
├── Cargo.toml
├── LICENSE             # MIT
├── README.md
├── .gitignore
├── src/
│   ├── main.rs         # CLI (530 行, 4 動詞)
│   ├── core/
│   │   ├── mod.rs
│   │   ├── config.rs
│   │   ├── pair.rs
│   │   ├── session.rs
│   │   ├── confidential.rs
│   │   ├── intent.rs
│   │   ├── ecash.rs
│   │   └── first_run.rs
│   └── net/
│       ├── mod.rs
│       └── cashu_mint.rs
├── docs/
│   ├── ARCHITECTURE.md  # この文書
│   ├── internal/        # 設計判断記録
│   └── archive/         # 旧版
└── examples/
    └── job.yaml
```

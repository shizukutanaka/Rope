# Rope アーキテクチャ

*7 モジュール + 1 net レイヤ — 旧 122 モジュールから -94%*

---

## 全体図

```
┌─────────────────────────────────────────────────────┐
│  CLI (main.rs, 604 行)                              │
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
| **config** | 715 | 15 | 鍵生成 (Ed25519), 設定, ディレクトリ管理, プロセス間ロック |
| **pair** | 2,480 | 47 | ピア発見 (mDNS/BT/QR/DHT), Noise XX/IK 握手, 信頼ストア, QR nonce リプレイ防止 |
| **session** | 862 | 22 | セッション状態機械, verify code, panic stop, capability token |
| **confidential** | 1,477 | 28 | TEE attestation (H100/H200/Blackwell), 暗号セッション, ポリシー鮮度検証 |
| **intent** | 1,613 | 33 | Intent → ExecutionPlan 解決, 予算/遅延/プライバシー制約 |
| **ecash** | 1,873 | 38 | Cashu NUT-00 proof, escrow (6状態), streaming (1秒単位) |
| **first_run** | 1,114 | 19 | 60秒 wow moment (9 stage state machine) |

### net/ — I/O 層

| モジュール | 行数 | テスト | 責務 |
|-----------|-----:|------:|------|
| **cashu_mint** | 907 | 17 (http feature 有効時 23) | Cashu HTTP client (NUT-01/04/05/06/07), 翻訳層 (**未結線**: 呼び出し元ゼロ) |
| **main.rs** | 604 | 13 | CLI 引数解析, 4 動詞ルーティング, 統合テスト |

合計: 11,702 行, 232 テスト (default) / 238 テスト (`--features http`) (2026-07-01 時点、v0.2.3)

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
│   ├── main.rs         # CLI (604 行, 4 動詞)
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

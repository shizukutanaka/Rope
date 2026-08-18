# Rope

**他人のアイドル GPU を、安全に、1 ドル未満で 60 秒借りる方法。**

EXO は同じ所有者のデバイス向け。Rope は他人のデバイス向け。

> ⚠️ 上の一文は **v2 のビジョン**。現時点で正直に言うと:
> - ✅ **推論は実際に走る** (2026-08-18〜)。`rope run` は依存ゼロの Transformer を
>   **CPU で**実行する — 固定文字列を返すプレースホルダではない。
> - ❌ **まだ「他人の」ではない。** ピア発見 (mDNS) と Noise、ecash 決済が
>   未結線のため、計算は**自分のマシンで**走る。
> - ❌ **まだ「GPU」でもない。** 現在の実行は CPU f32。GPU バックエンドは
>   `InferenceEngine` トレイトを実装すれば足せる形になっている。
> - ❌ **v1 は「安全に」(TEE 秘匿) を提供しない** — 消費者 GPU に機密計算が
>   無いため、**貸し手はプロンプトを見られる**。
>
> v1 が実際に提供するものは [§ v1 スコープ](#-v1-スコープ-2026-08-決定) と
> [`SECURITY.md`](SECURITY.md) を参照。

---

## 60 秒デモ

```
$ rope
👋 Rope へようこそ。60秒で他人のGPUでAIを動かします。
🔑 鍵の準備ができました。
📡 近くのピアを探しています…
🤝 接続しました。
🛡️  GPU は安全 (プロンプトは相手に見えません)
💭 ジョブを組み立て中…
✨ 動きました。

A quiet click, a distant fan,
Silicon warms for a stranger,
My thanks ride on light.
```

ログイン無し。設定無し。質問無し。

> **この画面は何をしているか**: 上記は初回起動 (`rope`, 引数無し) の実際の出力だが、
> 現状これは **ローカルで完結するシミュレーション**。実際に近隣ピアを探索・接続・
> 推論を実行しているのではなく、鍵生成などの本物のロジックの間に、
> 4 種類の中から選ばれる固定の haiku を表示している (`sample_haiku_response`)。
> `pair`/`run`/`earn` を含め、実 P2P 通信・実暗号鍵交換・実 GPU 推論はまだ配線されて
> いない — 状態機械としては完成しているが、実 I/O が無い。詳細と理由:
> [`docs/ASSESSMENT.md`](docs/ASSESSMENT.md)。

---

## インストール

`crates.io` 上で `rope` という名前は既に別プロジェクト (無関係な文字列データ構造の
"rope"、yanked 済み) に使われているため `cargo install rope` は使えない
(名前は再利用不可)。ソースからビルドする:

```bash
git clone https://github.com/shizukutanaka/rope.git
cd rope
cargo install --path .
```

初回実行:

```bash
rope
```

60 秒後、他人の GPU で haiku が画面に出る。

---

## 🎯 v1 スコープ (2026-08 決定)

**Musk のアルゴリズム** (①要件を疑え → ②削除せよ → ③単純化…) を適用し、
**出荷できる最小の製品**まで要件を削った。決定と根拠:
[`docs/V1_SCOPE.md`](docs/V1_SCOPE.md)。

> **v1 = LAN 上の他人の GPU で、プロンプト推論を、
> トークン不要の ecash で払って、設定ゼロで走らせる。**

| v1 に**入れる** | v1 から**外す** (v2 へ延期) |
|---|---|
| LAN 発見 (mDNS)・TOFU 信頼 | **TEE 秘匿 (A6)** — 消費者 GPU に CC が無く (W4)、構造的緊張 4 件中 3 件の源 |
| **プロンプト推論** (ローカルモデル、CPU) — ✅ **実装済 (2026-08-18)**。`src/net/inference.rs` が依存ゼロの Transformer を実行する | `Train` / `Retrieve` — 借り手指定 URI と重みロードは **SSRF と pickle RCE の面**。削除で攻撃面ごと消える |
| **ecash 決済 (トークン不要)** + DLEQ オフライン検証 | 実行検証 (A4) — 検証すべき実行がまだ無い |
| deadman 返金・ゼロコンフィグ | NAT 越え / Sybil 耐性 — LAN では不要 |
| 貸し手のプロセス隔離 (GPU レベルは**保護しないと開示**) | |

**なぜ削るのか**: TEE を外すと `A6⊥A8` `A6⊥A9` が消え、
`Train`/`Retrieve` を外すと `A9⊥A8` も消える —
**構造的緊張 4 件のうち 3 件が、機能を足さずに削除だけで解消する**。
**v1 が証明すべき固有価値は「トークン不要の少額決済で計算が買える」** —
調査した競合は全て独自トークン決済で、ecash の GPU マーケットは存在しない
([`docs/RESEARCH_UPDATE_2026-08.md`](docs/RESEARCH_UPDATE_2026-08.md) §4k)。

---

## 4 動詞

```bash
rope                  # 引数無し、初回 60 秒 wow moment
rope pair             # AirDrop 式ピア発見 (mDNS / BT / QR)
rope run <model>      # ローカル優先、足りなきゃピア
rope earn             # 自分の GPU を貸し出す
```

---

## なぜ Rope か (8 競合との比較)

> ⚠️ **表の読み方 (2026-08 訂正)**: 競合列は**出荷済みの機能**を指す。
> Rope 列は **✅ = 実際に動く** / **🔶 = 型と状態機械はあるが未結線 (現時点では
> 提供されていない)** を区別する。以前は全て ✅ だったが、これは
> 実装済み競合と設計だけの自プロダクトを同じ記号で並べるもので、
> 実態と食い違っていた。公理ごとの厳密な現状は
> [`docs/FIRST_PRINCIPLES_AUDIT.md`](docs/FIRST_PRINCIPLES_AUDIT.md) §2 を参照。

| | Rope | EXO | Petals | Akash | io.net | Vast.ai | RunPod | Ollama |
|---|------|-----|--------|-------|--------|---------|--------|--------|
| 他人 GPU | 🔶 | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| TEE プライバシー | 🔶 | N/A | ❌ | △ | ❌ | ❌ | △ | N/A |
| 独自トークン不要 | ✅ | N/A | ✅ | ❌AKT | ❌IO | ✅ | ✅ | N/A |
| ジョブ中断耐性 | 🔶escrow | N/A | ❌ | △ | △ | ❌ | △ | N/A |
| 1秒単位課金 | 🔶 | N/A | N/A | ❌ | ❌ | ❌ | ❌ | N/A |
| ゼロコンフィグ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |

**Rope の wedge (目標)** = 他人 GPU + GPU TEE + ecash 少額決済の交点。
**現時点で実際に提供できているのは「独自トークン不要」と「ゼロコンフィグ」の 2 つ**
— 他は設計済みだが未結線 (`FIRST_PRINCIPLES_AUDIT.md` §5)。

**この分野は動いている (2026-08 調査)**: 上表の 8 競合以外にも
Render/Dispersed.com・Aethir・Fluence・Nosana・iExec・Argentum AI 等が存在し、
特に **Cocoon (Confidential Compute Open Network, TON 上)** は
**GPU 提供者に「プライベート推論」の対価を払う**という **Rope とほぼ同じ wedge**
を狙う新規参入。ただし **調査した限りどの競合も独自トークンを使う**
(TON / NOS / AKT / IO) — **ecash や Lightning で決済する GPU マーケットは
見つからなかった**ため、「独自トークン不要」は現時点でも**実質的な差別化**として
成立している。詳細: [`docs/RESEARCH_UPDATE_2026-08.md`](docs/RESEARCH_UPDATE_2026-08.md) §4k。

> **TEE プライバシーの前提**: 「プロンプトは相手に見えません」が成立するのは
> **attestation 検証済みの TEE 対応 GPU (NVIDIA Hopper/Blackwell 以降)** に限ります。
> CC 非対応の消費者 GPU に機微ジョブを送ると平文が露出するため、
> `Privacy::ConfidentialCompute` の intent は **Attested 検証を必須**とし、
> 検証なし/非 TEE ピアへのルーティングは resolver が拒否します
> (`core::intent` の feasibility ガード)。詳細: [`docs/RESEARCH_IMPROVEMENTS.md`](docs/RESEARCH_IMPROVEMENTS.md) #2。

---

## アーキテクチャ — 7 モジュール (旧 122 から -94%)

### 4 動詞の核 (5)
- `first_run` — 60 秒 wow moment オーケストレータ
- `pair` — mDNS + Noise XX/IK + QR フォールバック
- `intent` — workload を 4 動詞へ翻訳する単一プリミティブ
- `confidential` — H100/H200/Blackwell GPU TEE 配線 (LANDMINES 2 対応)
- `ecash` — Cashu bearer token + escrow + streaming

### 共有基盤 (2)
- `config` — 設定パス解決
- `session` — セッション ID と暗号鍵管理

これだけ。ROPE_2028 は「30 モジュール」を目標にしたが、**実際の出荷可能形は 7**。
集中の極限。

---

## ステータス

```
モジュール:  7 core + 1 net  (旧 122、-93%)
main.rs:     643 行 (旧 11,594、-94%)
総 Rust:     12,040 行 (旧 ~75,600、-84%)
テスト:      238 (cargo test 実通過、ローカル確認。CI は未配線 — 後述)
版:          0.2.14
exit(1):     ロック競合・孤児セッション掃除失敗では出さない (v0.2.2 で graceful 化)。
             致命的 I/O 障害 (disk full 等) では正直に exit(1) + 原因表示。
```

---

## 設計原則

1. **Empathy** — Markkula 1977 — 質問しない、推測する
2. **Focus** — 4 動詞のみ
3. **Subtraction** — 90 モジュール削除済
4. **Work Backwards** — 60 秒の haiku から逆算
5. **Wow Moment** — 「他人 GPU で AI が動いた」を初回体験

---

## ドキュメント

- [`CLAUDE.md`](CLAUDE.md) — **AI エージェント (Opus/Sonnet) 向けの入口**。長所短所改善案の要約と作業規範
- [`CHANGELOG.md`](CHANGELOG.md) — 変更履歴 (SemVer)
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — 貢献ガイド (設計原則・品質ゲート・テスト方針)
- [`SECURITY.md`](SECURITY.md) — 現状の暗号学的成熟度 (何が本物で何がプレースホルダか), 信頼モデル, 脆弱性報告
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — 7 モジュール構造, 状態機械マップ, データフロー
- [`docs/ASSESSMENT.md`](docs/ASSESSMENT.md) — 長所 / 短所 / 改善点の評価
- [`docs/RESEARCH_IMPROVEMENTS.md`](docs/RESEARCH_IMPROVEMENTS.md) — 同種ソフト・arXiv 調査に基づく優先度バックログ (2026-06-05 時点)
- [`docs/RESEARCH_UPDATE_2026-07.md`](docs/RESEARCH_UPDATE_2026-07.md) — 上記の1ヶ月差分アップデート (新規論文3件・Iroh 1.0 等の新規検討事項)
- [`docs/REACHABILITY_AUDIT.md`](docs/REACHABILITY_AUDIT.md) — `#![allow(dead_code)]` 配下 161 関数の到達可能性監査 (次回削除候補 29件)
- [`docs/SURPLUS_AND_GAPS.md`](docs/SURPLUS_AND_GAPS.md) — 過剰(過去に削除/保持判断したもの)と不足(未実装・要判断)を機械可読形式で一覧化。後続の AI エージェント向け
- **[`docs/A3_INFERENCE_IMPLEMENTATION_READINESS.md`](docs/A3_INFERENCE_IMPLEMENTATION_READINESS.md) — 📘 推論を実際に走らせる手順書 (v1 の第1項目)**
- **[`docs/V1_SCOPE.md`](docs/V1_SCOPE.md) — 🎯 v1 で何を作り何を作らないか (Musk のアルゴリズムによる要件削減の決定)**
- [`docs/FIRST_PRINCIPLES_AUDIT.md`](docs/FIRST_PRINCIPLES_AUDIT.md) — 製品定義の公理から機能の要否を演繹した監査 (上記の帰納的一覧と対をなす)
- [`docs/RESEARCH_UPDATE_2026-08.md`](docs/RESEARCH_UPDATE_2026-08.md) — 最新論文・技術動向 (Hollow-LLM 攻撃、推論 crate 選定、Iroh 1.0、NVIDIA CC 本番化)
- [`docs/P2P_IMPLEMENTATION_READINESS.md`](docs/P2P_IMPLEMENTATION_READINESS.md) — 最大のギャップ (実P2P I/O) を、ネットワーク復旧後すぐ着手できる runbook として事前設計
- [`docs/CASHU_BDHKE_IMPLEMENTATION_READINESS.md`](docs/CASHU_BDHKE_IMPLEMENTATION_READINESS.md) — Cashu 盲目署名 (BDHKE) の実装 runbook。NUT-00 公式仕様の数式を検証済み
- [`examples/quickstart.sh`](examples/quickstart.sh) — 4 動詞クイックスタート
- [`examples/library_usage.rs`](examples/library_usage.rs) — core/ ライブラリ使用例
- [`.github/ci.yml.disabled`](.github/ci.yml.disabled) — CI 定義 (check / test / clippy / fmt)。
  **注意**: `.github/workflows/` 直下に無いため GitHub Actions からは未認識・未実行
  (import 時からこの状態 — 有効化は要判断のため保留中)

---

## ライセンス

MIT

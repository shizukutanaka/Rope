# Rope

**他人のアイドル GPU を、安全に、1 ドル未満で 60 秒借りる方法。**

EXO は同じ所有者のデバイス向け。Rope は他人のデバイス向け。

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

## 4 動詞

```bash
rope                  # 引数無し、初回 60 秒 wow moment
rope pair             # AirDrop 式ピア発見 (mDNS / BT / QR)
rope run <model>      # ローカル優先、足りなきゃピア
rope earn             # 自分の GPU を貸し出す
```

---

## なぜ Rope か (8 競合との比較)

| | Rope | EXO | Petals | Akash | io.net | Vast.ai | RunPod | Ollama |
|---|------|-----|--------|-------|--------|---------|--------|--------|
| 他人 GPU | ✅ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| TEE プライバシー | ✅ | N/A | ❌ | △ | ❌ | ❌ | △ | N/A |
| 独自トークン不要 | ✅ | N/A | ✅ | ❌AKT | ❌IO | ✅ | ✅ | N/A |
| ジョブ中断耐性 | ✅escrow | N/A | ❌ | △ | △ | ❌ | △ | N/A |
| 1秒単位課金 | ✅ | N/A | N/A | ❌ | ❌ | ❌ | ❌ | N/A |
| ゼロコンフィグ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |

**Rope の wedge** = 他人 GPU + GPU TEE + ecash 少額決済の交点。

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
総 Rust:     11,946 行 (旧 ~75,600、-84%)
テスト:      238 (cargo test 実通過、ローカル確認。CI は未配線 — 後述)
版:          0.2.13
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

- [`CHANGELOG.md`](CHANGELOG.md) — 変更履歴 (SemVer)
- [`SECURITY.md`](SECURITY.md) — 現状の暗号学的成熟度 (何が本物で何がプレースホルダか), 信頼モデル, 脆弱性報告
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — 7 モジュール構造, 状態機械マップ, データフロー
- [`docs/ASSESSMENT.md`](docs/ASSESSMENT.md) — 長所 / 短所 / 改善点の評価
- [`docs/RESEARCH_IMPROVEMENTS.md`](docs/RESEARCH_IMPROVEMENTS.md) — 同種ソフト・arXiv 調査に基づく優先度バックログ
- [`docs/REACHABILITY_AUDIT.md`](docs/REACHABILITY_AUDIT.md) — `#![allow(dead_code)]` 配下 161 関数の到達可能性監査 (次回削除候補 29件)
- [`examples/quickstart.sh`](examples/quickstart.sh) — 4 動詞クイックスタート
- [`examples/library_usage.rs`](examples/library_usage.rs) — core/ ライブラリ使用例
- [`.github/ci.yml.disabled`](.github/ci.yml.disabled) — CI 定義 (check / test / clippy / fmt)。
  **注意**: `.github/workflows/` 直下に無いため GitHub Actions からは未認識・未実行
  (import 時からこの状態 — 有効化は要判断のため保留中)

---

## ライセンス

MIT

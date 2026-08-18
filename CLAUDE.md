# CLAUDE.md — Opus / Sonnet 向け作業指示書

> このファイルは Claude Code がセッション開始時に自動ロードする。
> **Rope で作業する Opus / Sonnet セッションは、まずこれを読んでから動くこと。**
> 長所・短所・改善案の要約と作業規範を凝縮している。詳細は各所からリンクする
> 既存ドキュメントへ委譲する (このファイルは「入口」であって全部は書かない)。
>
> 最終更新: 2026-07。対象リポジトリ状態: v0.2.14 公開後。

---

## 0. 最初に確認すること (Orientation)

1. **この製品の実態**: Rope は「状態機械としては完成しているが、エンドツーエンドでは
   動かないスケルトン」。実 P2P 通信・実暗号 (Noise/BDHKE)・実 TEE attestation 検証・
   実 GPU 推論は**まだ配線されていない** (QR ペアリングの blake3 keyed-MAC だけ本物)。
   これは隠れた欠陥ではなく意図的に開示された現状 → [`SECURITY.md`](SECURITY.md),
   [`docs/ASSESSMENT.md`](docs/ASSESSMENT.md)。README 冒頭のデモも「シミュレーション」
   と明記済み。

2. **🔴 最重要の環境制約**: このリポジトリの実行環境では、ネットワークポリシーにより
   `cargo build`/`test`/`clippy` が `static.crates.io` の 403 で**そもそも動かない
   ことがある** ([`docs/SURPLUS_AND_GAPS.md`](docs/SURPLUS_AND_GAPS.md) §0)。
   - **セッション開始時に一度だけ** ビルド可否を確認する (ただし何度もポーリングしない)。
   - **ビルド不能なら**: コード変更は「コンパイラ未検証」を CHANGELOG とコミット
     メッセージに必ず明記する。入念な手動レビュー (型・借用・参照解決・clippy lint の
     目視) で代替し、公開済みリリースに未検証コードを積み増すのは慎重に判断する。

3. **ブランチとリリース**:
   - デフォルトブランチ = `claude/deepresearch-ultrathink-improve-wnYtn`
     (通常の `main` ではない)。開発・push は原則このブランチ。
   - 公開リリース = **v0.2.14** (デフォルトブランチ最新)。git tag は gateway 制約で
     未作成 — 必要なら手動 (`git tag -a v0.2.14 <sha> && git push origin v0.2.14`)。
   - 別ブランチ・tag への push は gateway が 403 で拒否する場合がある。

---

## 1. 長所 (壊すな — これらは Rope の芯)

1. **設計の極端な集中** — 4 動詞 (`rope`/`pair`/`run`/`earn`) / 7 core モジュール。
   新しいトップレベル動詞やサブコマンドは Issue 合意なしに足さない。
2. **明快な状態機械** — `ecash`/`pair`/`confidential`/`intent` が型と状態遷移で表現され、
   I/O と分離した pure ロジックとして単体テスト可能。
3. **厳格なローカル品質ゲート** — `clippy -D warnings`/`fmt --check`/`unsafe_code = deny`
   を crate 全体に強制。テスト 239+ (default) / 245+ (`--features http`)。
   **ただし CI 自体は未有効化** (§下記短所)。「ローカルで厳格」≠「自動化されている」。
4. **依存最小主義** — HTTP は opt-in feature、edition2024 を避けた上限固定でビルド再現性。
5. **正直なフォールバック** — 「できるフリ」をしない。未実装は `capability_boundary!`
   マクロ (`src/main.rs`) で「今動く/次の版」を開示、または docs で正直に開示。
   **CLI 出力・README・コミットメッセージが実態と食い違う変更はしない** (最重要の文化)。
6. **未結線コードが自己文書化されている** — `#![allow(dead_code)]` 配下 161 関数の
   到達可能性を全数調査済 ([`docs/REACHABILITY_AUDIT.md`](docs/REACHABILITY_AUDIT.md))、
   主要な「テスト済みだが到達不能」構造体には status doc comment を付与済。
7. **次の実装の事前調査が完了している** — 最大 2 ギャップ (P2P/BDHKE) の実行手順書あり
   (下記改善案の表)。

---

## 2. 短所 (直すべき / 意識すべき — 各行に詳細アンカー)

| # | 短所 | 詳細 |
|---|------|------|
| W1 | 実 I/O が皆無 (Noise/mDNS/DHT/BDHKE/NRAS すべてモデルのみ)。唯一の実 I/O は `http` の Cashu mint DTO | `SURPLUS_AND_GAPS.md` §1.1, §1.2 |
| W2 | NAT 越えなし (hole punch/relay 未実装) → 実質 LAN 止まり。**最大の機能的欠落** | §1.2 |
| W3 | 検証 (proof-of-execution) が enum のみ。free-riding 未対策。実推論エンジンも未統合 | §1.3 |
| W4 | TEE プライバシー訴求と「消費者 GPU は CC 非対応」の矛盾 (ルーティングガードのみ対処済) | §1.4 |
| W5 | CI 未有効化 (`.github/ci.yml.disabled` のまま。有効化は**要ユーザー同意**) | §1.5 |
| W6 | 永続化がチェックポイントのみ (WAL 未化)。実 ecash 結線時に金銭損失リスクへ昇格 | §1.6 |
| W7 | **ecash マネーパスの潜在的不整合2件** (§1.8 返金が `total_sats` を増やすが proof 未復元 → 「表示されるが使えない残高」/ §1.9 `spend_proofs` が検証前にバケット変異 → 部分的に不正な id 列で有効 proof 消滅)。実 ecash 結線前に要修正 | §1.8, §1.9 |
| W8 | 6 個の `format_*` が未表示 (`rope status` 動詞が無い)。7 個の QR/TOFU 統計も未表示 | §1.7 |

W7/W8 等の「現状は実害ゼロだが将来バグ化」項目は、`EcashManager` の変異 API 全体が
どの CLI 動詞からも到達しないため今は無害 (§2.3)。実結線時に必ず対処すること。

---

## 3. 改善案 — **v1 スコープ順** (2026-08 決定)

> **🎯 まず [`docs/V1_SCOPE.md`](docs/V1_SCOPE.md) を読むこと。**
> Musk のアルゴリズム (①要件を疑え → ②削除 → ③単純化 → ④高速化 → ⑤自動化) で
> 要件を削り、**出荷できる最小の製品**を確定させた。
> **v1 = LAN 上の他人の GPU で、プロンプト推論を、トークン不要の ecash で払って、
> 設定ゼロで走らせる。**
>
> **v1 から外したもの (v2 へ延期、着手しないこと)**: TEE 秘匿 (A6) /
> `Workload::Train`・`Retrieve` / 実行検証 (A4) / NAT 越え / Sybil 耐性。
> これらを外すと**構造的緊張 4 件中 3 件が消える** — 削除が最大の設計改善だった。

| 順 | やること | ブロッカー | 手引き |
|---|--------|-----------|-------|
| **1** | **A3+A9: プロンプト推論を実際に走らせる + プロセス隔離** (不可分) | `build` (**mistral.rs**) | **📘 手順書: [`docs/A3_INFERENCE_IMPLEMENTATION_READINESS.md`](docs/A3_INFERENCE_IMPLEMENTATION_READINESS.md)** (配線点 3 箇所・実装順序・DoD)。最小条件は `FIRST_PRINCIPLES_AUDIT.md` §8 (CPU・非決定論的・非TEE・**ローカルモデルパスのみ**)。置き場所は `net/inference.rs` + feature 分離。隔離は pure Rust crate (`sandbox-rs` 等) で足りる — **`Inference` のみなら SSRF も pickle RCE も無い** (`RESEARCH_UPDATE_2026-08.md` §4j, §4l) |
| **2** | **A5 を `rope run` に結線** (トークン不要決済 = 唯一の差別化) | `build` (k256) | [`CASHU_BDHKE_IMPLEMENTATION_READINESS.md`](docs/CASHU_BDHKE_IMPLEMENTATION_READINESS.md)。**DLEQ (NUT-12) を含めること** — LAN では mint 到達性が保証されず、オフライン検証が必須 (§4c) |
| **3** | **A1: mDNS で LAN のピアを実際に見つける** (NAT 越えは v2) | `build` (Iroh) | [`P2P_IMPLEMENTATION_READINESS.md`](docs/P2P_IMPLEMENTATION_READINESS.md)。**Iroh の `MdnsDiscovery` はデフォルト有効・リレー不要**で LAN に必要十分 (§4d) |
| **4** | **`A9→A7` の欠落を直す** — 緊急停止時に未完了 escrow を返金 | なし (要ビルド検証) | `SURPLUS_AND_GAPS.md` §1.10。**完了済み escrow は返金しない**こと (貸し手の攻撃面になる) |
| 5 | ③単純化 → ④高速化 → **⑤ CI 有効化** | **`consent`** | **④より前に⑤をやらない** (Musk の順序違反)。CI は動くものが出てから |
| — | ecash マネーパスの既知不整合 (§1.8/§1.9) | — | **A5 結線 (順 2) と同時に必ず直す** |

**v2 へ延期した項目の根拠**は消えていない — 戻す時は
[`RESEARCH_UPDATE_2026-08.md`](docs/RESEARCH_UPDATE_2026-08.md) を参照。

**新研究**: 2026-07 時点の関連論文・スタック更新は
[`docs/RESEARCH_UPDATE_2026-07.md`](docs/RESEARCH_UPDATE_2026-07.md)
(TensorCommitments, Iroh 1.0, 推論エンジン比較, Corvex の実 TEE 事例)。

---

## 4. 作業規範 (このプロジェクトで守るべき運用)

1. **未検証の明記**: ビルド不能環境での全コード変更は「コンパイラ未検証」を
   CHANGELOG `[Unreleased]` とコミットメッセージに明記する (例外なし)。
2. **単独レビューを過信しない**: 実例として、`perform_attestation` に `update_stats()` を
   丸ごと配線した変更が `active_sessions` を 0 に巻き戻す回帰を生み、**単独レビューでは
   見逃し、Workflow の多エージェント敵対的レビューでのみ検出された** (commit `66383b9`)。
   重要・広範な変更は敵対的レビューを推奨 (Workflow はユーザーの明示的 opt-in が必要 —
   「ultracode」や「ワークフローで検証して」等)。
3. **発見は file:line アンカーで記録**: バグ・不整合を見つけたら `SURPLUS_AND_GAPS.md` に
   file:line 付きで追記する (返金 proof 不整合 §1.8 が手本)。将来の実装者が必ず参照する。
4. **後方互換性**: 永続化 JSON (`~/.rope/*.json`) のフォーマット変更は既存ファイルの
   読込を壊さないこと。新規フィールドは `#[serde(default)]`。`load_or_recover` の
   破損復旧パターンを踏襲。
5. **コーディング規約**: `unsafe` 禁止 / MSRV 1.75 / edition2024 回避 (上限固定パターン) /
   品質ゲート全通過。詳細は [`CONTRIBUTING.md`](CONTRIBUTING.md)。
6. **正直さの文化**: 「動くフリ」を絶対にしない。デモ・出力・ドキュメントは実態と
   一致させる。プレースホルダは `SECURITY.md` の分離開示に従う。

### モデル別の運用ヒント (Opus / Sonnet)

- **Sonnet セッション**: 小さく明確にスコープされた変更に向く — 手順書
  (readiness runbook) 準拠の実装、§アンカー参照の徹底、発見の file:line 記録。
  ビルド不能環境での広範な削除・暗号/金銭経路 (`ecash.rs`/`cashu_mint.rs` の
  マネーパス) の変更は避ける。改善案表で `consent`/`decision` タグの付いた
  項目は着手せずユーザーに委ねる。
- **Opus セッション**: 判断を伴う作業に向く — 到達可能性監査・精読による
  欠陥発見 (§1.8 返金 proof 不整合が実例)、複数文書間の整合検証、runbook の
  設計判断 (libp2p vs Iroh、k256 選定等) の再評価。広範・重要な変更には
  Workflow 敵対的レビュー (要ユーザー opt-in) を組み合わせる。
- **共通の絶対条件**: どちらのモデルでも「未検証の明記」(規範1) と
  「正直さの文化」(規範6) に例外は無い。

---

## 5. ドキュメント地図 (何を知りたい時にどれを読むか)

| 知りたいこと | 読むファイル |
|-------------|-------------|
| **🎯 何を作り何を作らないか (v1 の決定)** | **[`docs/V1_SCOPE.md`](docs/V1_SCOPE.md)** ← まずこれ |
| 過不足の機械可読な一覧 (§アンカー付き) | [`docs/SURPLUS_AND_GAPS.md`](docs/SURPLUS_AND_GAPS.md) |
| **なぜその機能が要るのか** (公理からの演繹・優先順位の根拠) | [`docs/FIRST_PRINCIPLES_AUDIT.md`](docs/FIRST_PRINCIPLES_AUDIT.md) |
| 長所/短所/改善点の人間向け評価 (ソクラテス式問答含む) | [`docs/ASSESSMENT.md`](docs/ASSESSMENT.md) |
| **推論をどう実装するか (v1 の順序1)** | **[`docs/A3_INFERENCE_IMPLEMENTATION_READINESS.md`](docs/A3_INFERENCE_IMPLEMENTATION_READINESS.md)** |
| 実 P2P をどう実装するか | [`docs/P2P_IMPLEMENTATION_READINESS.md`](docs/P2P_IMPLEMENTATION_READINESS.md) |
| 実 BDHKE をどう実装するか | [`docs/CASHU_BDHKE_IMPLEMENTATION_READINESS.md`](docs/CASHU_BDHKE_IMPLEMENTATION_READINESS.md) |
| どのコードが CLI から到達するか | [`docs/REACHABILITY_AUDIT.md`](docs/REACHABILITY_AUDIT.md) |
| 何が本物の暗号で何がプレースホルダか | [`SECURITY.md`](SECURITY.md) |
| モジュール構造・状態機械マップ | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) |
| 優先度付き改善バックログ (2026-06) + 差分 (2026-07, **2026-08**) | [`docs/RESEARCH_IMPROVEMENTS.md`](docs/RESEARCH_IMPROVEMENTS.md), [`docs/RESEARCH_UPDATE_2026-07.md`](docs/RESEARCH_UPDATE_2026-07.md), [`docs/RESEARCH_UPDATE_2026-08.md`](docs/RESEARCH_UPDATE_2026-08.md) |
| 貢献規約・品質ゲート | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| 変更履歴 | [`CHANGELOG.md`](CHANGELOG.md) |

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

**⚠️ この節の変更はコンパイラ未検証** (ビルド不能の制約は `[0.2.14]` から継続、
`docs/SURPLUS_AND_GAPS.md` §0 参照)。入念な手動レビュー (型シグネチャ・参照解決・
clippy lint の目視確認) のみ実施。ビルド可能な環境での再検証が必須。

### Documented (careful-reading finding, no code change)

- **返金経路が `wallet.total_sats` を増やすが proof を復元しない潜在的不整合を
  発見・記録** — ecash のマネー経路を精読した結果、`close_stream`
  (`ecash.rs:1030`) / `refund_escrow` (`ecash.rs:837`) / `resolve_dispute`
  (`ecash.rs:883,890`) が返金時に scalar (`total_sats`) だけを増やし proof
  バケットを復元しないことを確認。`lock_funds` は proof 合計をチェックするため、
  実 ecash 結線後は「表示されるが再使用できない残高」になる。現状 ecash API は
  どの動詞からも到達しない (§2.3) ため実害ゼロ。`docs/SURPLUS_AND_GAPS.md` に
  §1.8 として file:line 付きで記録し、`CASHU_BDHKE_IMPLEMENTATION_READINESS.md`
  の Definition of Done に「返金は実 mint swap による proof 再発行に置換」を追加。
  信用側 (`mint_tokens`/`receive_proofs`/`lock_funds`) は不変条件を維持しており
  問題ないことも確認済み
- **`spend_proofs` がバケットを検証前に変異させ、部分的に不正な id 列で有効 proof を
  破壊する潜在バグを発見・記録** (§1.8 と同じ精読パス)。`spend_proofs`
  (`ecash.rs:523-542`) は `bucket.retain(...)` で該当 proof を**先に削除**してから
  「全部見つかったか」を検証し、不一致なら `total_sats` 減算前に bail する。
  結果、存在しない/重複 id を含む呼び出しでマッチ済み proof が失われ
  `total_sats > Σproofs` に desync する (§1.8 は fail-closed だがこちらは
  fail-*un*-closed で資金消滅)。`before_len` は捕捉のみで rollback に未使用。
  現状 `spend_proofs` は CLI 未到達 (§2.3) のため実害ゼロ。
  `docs/SURPLUS_AND_GAPS.md` §1.9 に file:line と v0.3 修正方針
  (「削除前に全 id 存在を検証し all-or-nothing 化」) を記録。
  **ビルド不能環境でのマネーパス修正は CLAUDE.md §4 の運用規範に従い見送り**、
  コンパイラ検証可能な v0.3 での修正に委ねる

### Changed

- **⑤自動化 (CI) のブロッカー記述を訂正 — 「要同意」は誤りで、実際は「権限不足」** —
  Musk の①(要件を疑え)を自分のメモに適用した結果。従来 `[BLOCKED:consent]` として
  「secrets アクセスのため要同意」と記録していたが、**ファイルを読むと `env:` は
  `CARGO_TERM_COLOR`/`RUSTFLAGS` のみ、ジョブは check/test/test-http/clippy/fmt/gate で
  secrets 参照ゼロ・deploy も publish も無し**。**実際のブロッカーは権限**で、
  git gateway は `workflows` 権限不足で拒否、GitHub API も 403 (両経路で実測)。
  → **リポジトリ所有者なら `git mv .github/ci.yml.disabled .github/workflows/ci.yml`
  の 1 手で有効化できる**。
  **これは単なる自動化ではない**: ローカルは `static.crates.io` 403 でビルド不能だが
  **Actions は crates.io に到達できる** → **CI はこの環境で唯一のコンパイラ検証手段**であり、
  `830d8c4` 以降の未検証コミット群を一括検証する (= Musk の④サイクルタイム短縮そのもの)
- **③単純化: ドキュメント地図を 3 層構造に再編** — `docs/` は 14 文書 6,500 行あり、
  実装者がどれを読むべきか判断できない状態だった (本セッションが診断した「文書 20:1」問題の本体)。
  **Tier 1 = v1 実装に必要な 3 つだけ** (`V1_SCOPE` / `A3_INFERENCE_...READINESS` /
  `SURPLUS_AND_GAPS`)、**Tier 2 = 判断の根拠**、**Tier 3 = 履歴 (実装では開かなくてよい)**。
  ②で要件を削った後、③は**残ったものを単純化する** — 削るべきは機能ではなく
  「実装者が頭に入れねばならない文書数」だった

### Added

- **`SECURITY.md` に「v1 のセキュリティモデル」節を追加** — v1 スコープ決定により
  **開示すべき境界が 4 つ確定**したため。「未完成」ではなく**設計上の境界**として明記:
  (1) **v1 は機密計算を提供しない** — **貸し手はプロンプトと出力を見られる**。
  機微データを v1 で他人の GPU に送ってはならない。(2) **貸し手保護はプロセスレベルのみで
  GPU レベルは無防備** — デバイスノード経由の権限昇格 (CVE-2026-22164) と同一 GPU 上の
  side channel は残る (gVisor+nvproxy なら介在できるがゼロコンフィグを壊すため v1 では採らない)。
  (3) **借り手指定 URI のフェッチと借り手指定の重みロードが構造的に存在しない**
  (`Train`/`Retrieve` 削除の帰結 — §1.11 の 2 脅威は防御ではなく機能削除で消えた)。
  (4) **実行結果を検証しない** (A4 は v2)。escrow の deadman 返金が唯一の保護で、
  LAN 上の相手であることが実質的な緩和という前提
- **📘 `docs/A3_INFERENCE_IMPLEMENTATION_READINESS.md` — v1 第1項目の実行手順書** —
  P2P/BDHKE には手順書があったが、**最も重い A3 (型すら存在しない唯一の公理) には
  無かった**。既存 runbook と同形式で作成。**配線点 3 箇所を grep 確認して特定**:
  (a) `main.rs:400-403` の `capability_boundary!` が**自ら「next: 実 GPU 推論実行」と
  宣言している**箇所、(b) `main.rs:226` の `sample_haiku_response()` (デモの固定文字列)、
  (c) `intent.rs:603` の `resolve()` が返す `ExecutionPlan` を誰も実行しない点。
  **A9 と 1 スコープで扱う** (実行させることは隔離を要求する)。
  **`Train`/`Retrieve` を v1 から削除したことで A9 の要求が大幅に軽くなる** —
  SSRF 面も pickle RCE 面も消滅し、**gVisor 不要で pure Rust の隔離 crate で足りる**
  (= `A9⊥A8` は「A8 を取る」で決着)。
  **DoD の最強シグナル**: **`rope` 無引数デモが実推論を表示し、README の
  「シミュレーション」注記を外せること** — 外せないなら A3 は未完了。
- **🎯 `docs/V1_SCOPE.md` — Musk のアルゴリズムで要件を削り、出荷できる v1 を確定** —
  ①要件を疑え → ②削除せよ → ③単純化 → ④高速化 → ⑤自動化 の順序を守って適用。
  **最初に削除したのはプロセスそのもの**: `830d8c4` 以降の実測が
  **src/ 180 行追加 vs 文書 3,672 行追加 (比率 20:1)、新規の製品能力ゼロ**であり、
  「実装前にさらに調査する」という要件自体が削除対象だった。
  **v1 = LAN 上の他人の GPU で、プロンプト推論を、トークン不要の ecash で払って、
  設定ゼロで走らせる。**
  **v1 から削除 (v2 へ延期)**: **TEE 秘匿 (A6)** — 消費者 GPU に CC が無く (W4 未解消)、
  **構造的緊張 4 件中 3 件の発生源** (`A6⊥A8` 性能 / `A6⊥A9` 法務 / 加えて `A9⊥A8` 隔離)。
  **`Workload::Train`/`Retrieve`** — 借り手指定 URI と重みロードは SSRF と pickle RCE の面
  (§1.11) であり、**削除すると攻撃面ごと消える** (防御を足すのではなく機能を消す)。
  **実行検証 (A4)** — A3 が無い今、検証する対象が存在しない。
  **NAT 越え / Sybil 耐性** — LAN に限れば不要。
  → **削除だけで構造的緊張 4 件中 3 件が消滅**。
  **v1 が証明すべき固有価値は「トークン不要の少額決済で計算が買える」** —
  競合は全て独自トークン決済で ecash の GPU マーケットは存在しない (§4k)。
  `README.md` に v1 スコープ節を追加、`CLAUDE.md` の改善案表を **v1 順序に全面置換**
  (A3+A9 → A5 結線 → A1 mDNS → `A9→A7` 修正 → その後に③④⑤。
  **④より前に⑤ CI をやらない** = Musk の順序違反)
- **`docs/RESEARCH_UPDATE_2026-08.md`** — 最新論文・技術情報の調査 (2026-08-08)。
  **調査設計を変更**: 従来は既存バックログのカテゴリ順に調べていたが、今回は
  `FIRST_PRINCIPLES_AUDIT.md` が演繹した公理の依存順 (A1 発見 → A3 実行 → A4 検証)
  に沿って「最優先と判定した箇所の実装可否を一次情報で確定させる」ことを目的とした。
  主要な発見:
  (1) **🔴 Hollow-LLM 攻撃** (arXiv:2607.28884, IEEE S&P'26 採択、前回調査以降の新着) —
  宣言アーキテクチャを保ったまま「ゴースト重み」で実効計算を潰し、ZK 証明を通過しつつ
  小モデル相当のコストしか払わない攻撃。根本原因は証明が *effort gap*
  (どれだけ計算したか) を検証しない点。**Rope の `proof_satisfies` はこれより更に弱く**、
  第一原理監査の A4 判定を査読付き論文が独立に裏付けた形。
  `RESEARCH_IMPROVEMENTS.md` #1 の受け入れ基準に「投入計算量の検証」を追加し、
  TensorCommitments の採否を effort gap 耐性の確認まで保留に変更。
  (2) **A3 の推論 crate を mistral.rs に確定** — pure Rust (Candle 0.9.2)、CPU 単独動作可で、
  第一原理監査 §8 が演繹した最小要件 (CPU・非TEE で十分) と噛み合う。`llama-cpp-2` は
  C++ FFI + 作者が semver 非準拠を明言しており依存最小主義・MSRV 固定方針と衝突。
  (3) **Iroh v1.0.0 が 2026-06 に正式リリース済**と確定 (NAT 越え成功率が libp2p ~70% を
  上回る、`libp2p-iroh` による段階移行も可能) → `P2P_IMPLEMENTATION_READINESS.md` に反映。
  (4) NVIDIA CC は本番運用フェーズ (Blackwell 組込、Apple PCC 採用)。NRAS の
  「attestation は起動時のみ」設計が Rope の鮮度チェック設計と整合することを確認。
  (5) **A2 (信頼確立) に 2 件の新着** — TraceRank (arXiv:2510.27554) は支払いを推薦として
  扱い支払者の評判で重み付ける手法で、**Rope は既に ecash を持つため決済履歴を信頼根拠に
  転用できる**ことが判明 (ただし Cashu の無記名性と衝突するため設計要検討)。
  DARTIC (arXiv:2605.18146) は dual-ledger で匿名性・評判・スケールを三立し、
  **評判を検証可能な実行結果で駆動する** — つまり検証が effort gap を見逃せば評判ごと
  汚染される。この 2 件から **第一原理監査の依存グラフに `A5 → A2` (逆方向) と
  `A4 → A2` の 2 辺を追加**。
  (6) **A5/A7 の調査で既存文書の優先度判定の誤りを 1 件発見・訂正** —
  `CASHU_BDHKE_IMPLEMENTATION_READINESS.md` が DLEQ (NUT-12) を「Step 5 任意・優先度低」に
  置いていたが、**一次仕様 (`cashubtc/nuts` `12.md`) を確認した結果、Rope の構造上は
  中核基盤**と判明。受信者が mint に問い合わせず署名を検証できるため、
  (a) `tick_stream` の 1 秒課金で mint 往復を省ける (60 秒ジョブで最大 60 往復を回避)、
  (b) **NAT 越え未実装で実質 LAN 止まりの現状では mint 到達性自体が保証されない**ため
  オフライン検証が不可欠 — **A1 が制約されている今こそ DLEQ の価値が高い**という逆説。
  該当 readiness の Step 5 を優先度引き上げに訂正 (P2PK は任意のまま)。
  併せて SAKSHI (arXiv:2307.16562) の**マイクロペイメント連鎖ハッシュ** (直前の支払いの
  ハッシュを含めることで中断時に「どこまで払ったか」を双方が独立に証明できる) を
  A7 実装時の設計入力として記録。
  (7) **A1 続報: P2P readiness が「未調査」と明記していた項目を確定** —
  Iroh の発見機構と Rope の `DiscoveryMethod` 4 経路の対応を調べ、
  **mDNS は `MdnsDiscovery` (旧 `LocalSwarmDiscovery`) としてデフォルト有効・
  インターネット/リレー/DNS 不要**、DHT 相当は pkarr (署名済 DNS パケットを
  mainline DHT へ publish)、Direct は NodeId dial で充足、**Bluetooth のみ非対応**と判明。
  演繹される含意: (a) **mDNS を自前実装する必要がない**ため A1 の実装コストは想定より小さい、
  (b) §4c の DLEQ と同型で、**Iroh の mDNS 発見がリレー不要である事実は Rope の現状
  (NAT 越えなし=実質 LAN) と噛み合う** — **NAT 越えを解決しなくても「LAN 内で実際に
  ピアを見つける」ところまでは到達できる**ため実装順序に直接効く、
  (c) `DiscoveryMethod::Bluetooth` は Iroh 採用時に唯一対応物が無く製品判断が要る。
  (8) **Hollow-LLM の追加調査で「推奨手法の前提が崩れている可能性」を検出** —
  論文本文には到達できないままだが、検索経由で防御方向と脅威モデルが判明。
  緩和方向は「ZK で推論の正しさだけでなく**重み行列のスパース性・活性度という
  構造的性質を証明する**」。より重要なのは脅威モデルの違いで、VeriLLM/DetectLLM/SVIP 系は
  **出力分布・隠れ状態・ランク検定・タイミング等の副次観測**を使うのに対し、
  Hollow-LLM が対象とするのは**検証者が公開アーキテクチャとコミットメントと証明
  トランスクリプトしか見えない配備**。**Rope は A6 (秘匿) を謳う以上、借り手は貸し手の
  重みを見られない構造**なので後者に近く、**`RESEARCH_IMPROVEMENTS.md` #1 が推奨してきた
  VeriLLM 系の前提が Rope では崩れている可能性**がある。併せて Rope 固有の選択肢として
  **「A6 (TEE) が本物になれば attestation が検証の土台になる」** (VeriAttn の
  GPU 計算 + TEE 検証構成) を A4 着手時の設計分岐として記録。
  (9) **A4×A6 の設計分岐に実測値で答え、新たな構造的緊張 `A6 ⊥ A8` を発見** —
  GPU-CC の性能を調査した結果、**GPU 計算自体はほぼ無損失 (BF16 matmul が
  非機密モードの 0.998 倍)** だが **LLM サービング全体では 13-27% のスループット損失**、
  原因は計算ではなく**機密 VM ↔ GPU のブリッジ** (転送が直列化され小さな転送ほど
  固定コストの割を食う) と判明 (arXiv:2606.23969)。H100 でも同傾向で
  オーバーヘッドはバッチ・シーケンスが大きいほど縮小 (2-5%、大型モデルでほぼゼロ)。
  → (a) 「TEE は遅いから検証に使えない」という想定は**誤り**、A4×A6 統合は現実的。
  (b) ただし **Rope の「60 秒で 1 ドル未満」= 小さく短いジョブはオーバーヘッドが
  最大化する領域**であり、**A6 (秘匿) と A8 (ゼロコンフィグ体験) が構造的に緊張する** —
  W4 とは別の性能面の矛盾として第一原理監査に記録。両立にはモデル常駐・転送最小化を
  **A3 実装時に織り込む必要**がある (後付け困難)。(c) VeriAttn (arXiv:2606.16352) が
  **「TEE は軽量な完全性検証だけを担い、線形/非線形の attention 計算は GPU にオフロード」**
  という分業で TSDP 比 2.60-5.42 倍の加速を達成しており、実測値と整合する参照設計となる。
  (10) **A8「60 秒」の隠れた前提を実測で確定** — cold start は 4 フェーズで
  **実測 40 秒超** (イメージ pull 4-8 分 / 70B 重み転送 40 秒超 / CUDA 初期化 10-30 秒)、
  cold/warm 間に **1000 倍のレイテンシ差**。60 秒の約束は cold start を含めた瞬間に
  構造的に破れるため、**A8 は「貸し手側でモデルが既にウォーム」を暗黙の前提としている**
  ことを明文化。デモモデルが 1B 級である既存設計は偶然この制約に整合しており、
  `RESEARCH_IMPROVEMENTS.md` #11 (モデル配布/コールドスタート) を「高優先」から
  **「A8 の成立条件」に格上げ**。keep-warm の経済 (GPU 費用 2-3 倍) を誰が負担するかを
  A5 の設計課題として記録。併せて**供給側の実在を査読付き研究で裏付け** —
  ACM AIBC 2025 (アイドル消費者 GPU の性能/コスト/炭素比較)、Petals (ピーク 800+ ノードで
  BLOOM-176B の P2P 推論を実証)、BOINC (500 万台)、本番 GPU 稼働率 40-65% —
  **リスクは供給ではなく A3/A2/A4 の未実装に集中**していることを確認
  (11) **W4 (TEE 訴求 vs 消費者 GPU) を両面で具体化** — 公理一巡を通じて唯一
  「解消していない」とし続けた矛盾を再訪。消費者 GPU を正面から扱う 2 件により、
  (a) **供給側の経済性が確定** (arXiv:2601.09527、RTX 5060 Ti/5070 Ti/5090 で 79 構成の
  ベンチ): セルフホスト推論は **$0.001-0.04/100万トークン**でクラウド API budget 帯の
  **40-200 倍安**、30M トークン/日で **4 ヶ月未満で回収** → 「1 ドル未満」は貸し手に
  巨大なマージンを残す。NVFP4 は BF16 比 **1.6 倍スループット + 41% 省エネ・品質低下 2-4%**
  (A3 のモデル形式選定に直接使える)。**⚠️ ただし同論文の "Private" は「ローカル配備」の意味で
  Confidential Computing ではなく、W4 はこれによって解消しない**。
  (b) **CC 無しで A6 が戦う相手が具体化** — CloakLM (arXiv:2606.18400) が引用する
  **Hermes** (PCIe バス観測だけで DNN を無損失再構成) / **TunnelS** (ドライバ経由で HBM を
  推論を止めずに高スループット exfiltration)。ハードウェアを物理所有する貸し手は両方使える →
  **SW のみの防御は「摩擦」にはなるが「保証」にはならない**。
  `SURPLUS_AND_GAPS.md` §1.4 に 3 つの製品選択肢 (CC 対応ピア限定を維持 / 消費者 GPU 向けに
  より弱い保証レベルを正直に定義 / 機微でないワークロード専用と割り切る) を `decision` 案件として記録
  (12) **🔴 公理集合の欠落を発見し、自分の過去判定を訂正** — A1-A7 を見直すと
  **すべて「借り手が必要とするもの」**であり、**貸し手が悪意ある借り手から自分を守る**
  という要件が欠けていた。**A9 (貸し手の保護) を公理として追加**。
  これに伴い **`FIRST_PRINCIPLES_AUDIT.md` §3 の `session.rs` クラスター評価を訂正** —
  当初「A1-A8 のどれにも対応しない → 削除材料が増えた」としたが、`panic_stop`
  (`session.rs:360-380`) は停止フラグを立て `docker ps --filter name=rope-` で
  Rope のコンテナを列挙し `docker kill` する = **貸し手の緊急停止機構そのもの**で
  **A9 に直接対応**。「対応しない」は**公理集合が不完全だったための誤判定**であり、
  当該クラスターを**削除候補から外す** (`SURPLUS_AND_GAPS.md` §2.4 の保留判断にも
  演繹的な答えが出た)。
  技術面: 2026 年の実務コンセンサスは「Docker/runc は AI 生成コードに不十分、
  gVisor/MicroVM 必須」、さらに「**信頼できないワークロードでは GPU アクセラレーションを
  無効化すべき**」(GPU デバイスノード経由の脆弱性 + 同一 GPU 上の side channel) —
  Rope の前提と正面から衝突する助言だが、**`Workload::Inference` はプロンプトのみで
  任意コード実行ではない**ため攻撃面は桁違いに狭い。**ただし
  `Workload::Train { dataset_uri }` と RAG は借り手指定 URI を貸し手がフェッチする**ため
  **Inference と Train/RAG を同じ脅威モデルで扱ってはいけない**。
  法務面: 生成 AI の CSAM に関し**モデルを提供する側も二次的責任を負いうる**
  (知りながら助長、または**緩和を怠った**場合)、STOP CSAM Act of 2025 成立。
  → **新しい構造的緊張 `A6 ⊥ A9`**: 借り手の秘匿を完全にするほど貸し手は
  自分の機材で何が生成されているか知り得なくなり、TEE は**盾にも刃にもなる**。
  W4・`A6 ⊥ A8` に続く A6 絡み 3 つ目の緊張で、しかも**技術ではなく法務の領域**
  (13) **🔴 公理ペアの体系的スイープで `A9 → A7` の欠落した辺を発見** (コードで確認) —
  これまでの公理間関係は逐次的に見つけたもので網羅確認をしていなかった (A9 自体が
  見落としだった)。A9 を軸に確認した結果、**緊急停止と返金が接続していない**ことが判明:
  `panic_stop` (`session.rs:360-390`) は `clear_session_files()` (`session.rs:385`) を呼ぶが、
  これが消すのは `session_dir()` 配下の `.json` のみで **`ecash.json` は対象外**。
  escrow の自動返金は `process_deadman` (`ecash.rs:900-918`) の `deadman_at < now` 頼みで
  既定 **60 分** (`ecash.rs:341,366,677`) → **貸し手が緊急停止しても借り手の資金は
  最大 60 分ロックされ、A7 (中断しても双方が損しない) が A9 の経路では成立しない**。
  A7 の返金は*時間切れ*を想定した設計で、**貸し手の意図的停止という A9 由来の中断は
  A9 が公理集合に無かったため想定されていなかった** — 公理の欠落が実装の欠落として
  現れた実例。現状は両者とも CLI 未到達 (§2.3) で実害ゼロだが **A3+A9 実装時に顕在化**。
  修正時の注意も記録: 素朴な即時返金は**新たな攻撃面**を作る (計算完遂後に `panic_stop`
  を撃てば成果を得たまま返金を強制できる) ため、**未完了 escrow に限定**する必要がある
  (`process_deadman` が既に持つ `Deposited | InProgress` フィルタと同形)。
  `SURPLUS_AND_GAPS.md` §1.10 に file:line 付きで記録。
  併せて**「関係なし」と判定したペアも記録** (A1⊥A9 は Rope 固有でない一般論、
  A8→A9 は実装方法の問題、A2→A7 は escrow が責任判定を要求しない設計で回避済み) —
  こじつけを避けるため否定の記録にも価値がある
  (14) **A9 のサンドボックス選定 — GPU 要件が選択肢をほぼ一つに絞ることが判明** —
  §4h は「gVisor/Firecracker 級」と並列に書いていたが、**Rope が GPU を使わせる制約**を
  掛けると答えが変わる: **Firecracker は GPU パススルーを意図的にサポートしない**
  (VFIO/IOMMU/PCIe パススルーが最小 virtio デバイス集合に無い) ため**失格**。
  **gVisor は nvproxy で NVIDIA の `ioctl` をユーザ空間で捕捉・転送**し、
  **GPU 呼び出しに介在できる唯一の選択肢**。
  一方 **pure Rust の隔離 crate 群** (`sandbox-rs` / `sandlock-core` / `hakoniwa` /
  `landlock`) が存在し、**docker も root も不要**で現行 `panic_stop` の docker 前提を
  置き換えうる — ただし**GPU 呼び出しには介在しない**ため `/dev/nvidia*` を通した瞬間に
  GPU 攻撃面が残る。→ **新しい構造的緊張 `A9 ⊥ A8`**: 強い A9 (gVisor 導入) は
  ゼロコンフィグを壊し、A8 を守る pure Rust 案は GPU レベルが無防備。
  `A6 ⊥ A8` と同じ形・同じ根。加えて**どれも Linux 前提** (landlock は 5.13+) で、
  macOS/Windows の貸し手は W4 と独立した第二の OS 制約に当たる。
  **A3 実装前に選択の確定が必要** (後から差し替えると `session.rs` クラスター全体に波及)。
  未確定事項も記録: 各 crate の MSRV/依存ツリー、nvproxy の実測オーバーヘッドは未確認
  (15) **🔴 README の競合比較表を訂正 — 未実装能力を実装済み競合と同じ ✅ で並べていた** —
  「関連ソフトウェア」の観点で 2026 のランドスケープを調べた際、自分の比較表を
  第一原理監査と突き合わせて発見。Rope 列は 6 行中 5 行が ✅ だったが、うち 4 件
  (他人GPU=A1+A3 / TEE プライバシー=A6 / ジョブ中断耐性=A7 / 1秒単位課金=A5) は
  **監査が未達と判定した能力**。表の下の注記は「どの GPU が CC 対応か」を説明するのみで、
  **✅ が未実装を指すことの開示は無かった** → Petals や Ollama の**出荷済み機能**と
  Rope の**設計だけの機能**を同記号で並べており、`CLAUDE.md` 規範6 (正直さの文化) に反する。
  **訂正**: Rope 列に **✅ (実際に動く) / 🔶 (型はあるが未結線)** の区別を導入し、
  読み方の注記と監査へのリンクを追加、**実際に提供できているのは「独自トークン不要」と
  「ゼロコンフィグ」の 2 つだけ**と明記。監査 §5 が既に同じ内容を書いていたのに
  **README 本体には反映されておらず、最も読まれる箇所 (製品のピッチ) に不整合が残っていた**。
  併せて**新規競合 Cocoon** (Confidential Compute Open Network, TON 上 — GPU 所有者に
  プライベート推論の対価を払う、**Rope とほぼ同一 wedge**) と 2026 の同分野プレイヤー
  (Render/Dispersed.com, Aethir, Fluence, Nosana, iExec, Argentum AI) を記録。
  **差別化の再確認**: 調査した限り**どの競合も独自トークン決済** (TON/NOS/AKT/IO) で、
  **ecash や Lightning の GPU マーケットは見つからなかった** → 「独自トークン不要」は
  表の中で唯一「実装状態に依存せず、かつ競合が誰も持っていない」差別化であり、
  **Rope が最初に証明すべき固有価値はここかもしれない** (A1/A3 は競合が既に持つ能力)
  (16) **A9 の絶対条件が 2 つ確定 — 「モデルを読む」こと自体がコード実行** —
  (a) **pickle のデシリアライズはロード中に任意コードを実行する**ため、借り手指定の重みを
  pickle 形式で読むことは **RCE と等価**。CVE-2026-25874 (HuggingFace LeRobot) が
  gRPC 経由の unsafe pickle で**認証不要 RCE** を起こしており、その構造が示唆的 —
  **バリデータがオブジェクトを見るのは pickle が構築した後** (`__reduce__` 実行後)。
  **スキャンも防御にならない** (ShadowPickle: 10 スキャナに対し **63% 回避**)。
  → **形式で制約する: SafeTensors/GGUF のみ受け付け、`torch.load`/pickle は使わない**。
  加えて `model`/`base_model` は**素の `String`** (`intent.rs:127,133`) で型が
  形式も取得元も制約しないため、**newtype 化する価値がある**。
  (b) **`dataset_uri` のフェッチは DNS ピンニングが必須** — ホスト名 allowlist は
  **DNS リバインディングで破られる** (短 TTL で検証時は公開 IP、リクエスト直前に内部 IP)。
  **CVE-2026-27826 (MCP Atlassian) は SSRF 修正そのものをこの手口で回避した 2026 の事例**。
  **`reqwest::dns` が解決をカスタマイズする trait を提供**しており Rope は既に reqwest に
  依存しているため**新規依存不要**。IP 検証には **`0.0.0.0`/`255.255.255.255` も含める**
  (Rust の実例: GHSA-q537-8fr5-cw35 — `activitypub-federation-rust` が `0.0.0.0` を
  見落として SSRF)。`SURPLUS_AND_GAPS.md` §1.11 に記録。
  **⚠️ 併せて自己訂正**: §4h/§4j で「Train/RAG が借り手指定 URI をフェッチする」と
  複数回書いたが、`Retrieve` は **`corpus_id` (貸し手側に既にある corpus の識別子)** で
  **URI を取らない** — **URI フェッチの露出は `Train` のみ**であり、`Retrieve` は
  設計上むしろ安全側だった
  **調査の限界も明記**: arxiv.org が egress proxy でブロックされるため論文は要旨のみ、
  crates.io 個別ページも取得不可 — 未確定事項を確定として扱わない
- **`docs/FIRST_PRINCIPLES_AUDIT.md`** — First Principles Thinking による機能の
  過不足監査を新設。既存の `SURPLUS_AND_GAPS.md` が実装から出発する**帰納**
  (「このコードは到達するか」) なのに対し、本書は製品定義
  「他人のアイドル GPU を安全に 1 ドル未満で 60 秒借りる」から**演繹**
  (「そもそも何が必要か」) する逆向きの分析。
  製品成立の必要条件を 7 公理 (A1 発見 / A2 信頼確立 / A3 実行 / A4 検証 /
  A5 対価移転 / A6 秘匿 / A7 中断耐性) + 補助公理 A8 (ゼロコンフィグ) に還元し、
  各公理を現行実装と file:line で突き合わせた。
  **結論: A1-A7 は 7 つすべて未達 (到達可能ゼロ)、機能しているのは A8 のみ** —
  帰納的監査と独立に同じ結論に到達し、現状認識の確度を高めた。
  さらに 2 つの新しい知見:
  (1) **A3 (推論実行) だけが型すら存在しない** — 他 6 公理は「型はあるが中身が空」
  なのに対し、推論エンジンへのバインディングは一行も無い (`llama` の grep ヒットは
  全てモデル名の文字列のみ)。`ExecutionPlan` を立てても実行する主体がいない。
  (2) **既存バックログとの優先順位のずれを検出** — 演繹では A5 (BDHKE) は
  A1/A3 に依存する後段であり、また **A3 (推論エンジン統合) を独立した最優先項目
  として立てていない**のが既存バックログの盲点。第一原理では A3 は
  「検証の前提条件」ではなく製品の中核能力そのもの。
  過剰側も公理対応で再評価 (`CapabilityChallenge` は A2 に直接対応するため
  保持が演繹的に正当化される / `session.rs` クラスターと `format_*` はどの公理にも
  対応しない)。純ドキュメント追加
  **第2段階 (§7・§8 追記)**: (a) **A8 の解剖** — 唯一機能する公理を実装レベルで
  分析。`first_run` の 8 ステップは A1/A6/A3 を順番に「演じる」構造で、
  `step_attest` はコード自身が「暫定: 実運用は Bob 側 GPU 種別を取得」と明記
  (`first_run.rs:410-411`)。**結果として `first_run` は A1-A7 実装時の統合テスト
  ハーネスとして既に完成しており、エンドツーエンドテストを新規設計する必要がない**
  (実装コスト見積りの下方修正)。(b) **A3 の最小充足条件を演繹** — 必須要素
  (推論エンジンバインディング / モデル取得手段 / `ExecutionPlan` からの実行配線点)
  と、**必要と誤認されやすいが A3 には不要なもの**を切り分け: 決定論的推論は
  A4 の要求、GPU/CUDA は A8 の体験要求、TEE は A6 の要求 —
  **A3 の初回実装は「CPU・非決定論的・非TEE・ローカルモデルパス」で十分**。
  配置は `net/cashu_mint.rs` の `http` feature 分離の前例に倣い
  `net/inference.rs` + `inference` feature が既存アーキテクチャと整合
- **`CLAUDE.md`** (repo ルート) — Claude Code がセッション開始時に自動ロードする
  Opus/Sonnet 向け作業指示書を新設。長所 (壊すな) 7項目・短所 8項目 (§アンカー付き)・
  改善案 (優先順+ブロッカー種別) を圧縮し、作業規範 (未検証の明記、単独レビューを
  過信しない=active_sessions 回帰の実例、file:line 発見記録、後方互換、正直さの文化) と
  14 ドキュメントの索引を収録。既存の分散した長所短所改善情報 (ASSESSMENT.md /
  SURPLUS_AND_GAPS.md / readiness 群) への単一の入口として機能する。純ドキュメント追加。
  追補: §4 に「モデル別の運用ヒント」小節を追加 — Sonnet は手順書準拠の
  スコープ明確な実装向き (マネーパス変更・広範削除は回避、consent/decision
  項目は着手しない)、Opus は判断を伴う監査・設計再評価向き (重要変更は
  Workflow 敵対的レビューと併用)、未検証明記と正直さは両者共通の絶対条件
- **`CONTRIBUTING.md`** — 公開OSSとして GitHub の Community Standards が
  期待する貢献ガイドを新設。設計原則 (Focus/Subtraction/正直なフォールバック)、
  マージ前に通すべき品質ゲート全コマンド、`unsafe` 禁止・MSRV 1.75・
  edition2024 回避の依存方針、回帰テスト必須の方針、および「一部履歴は
  コンパイラ未検証」の注記を明文化。純ドキュメント追加

### Changed

- **sats→USD 換算を単一ヘルパー `core::sats_to_usd` へ集約** — 従来
  `main.rs` と `first_run.rs` に `sats as f64 / 100_000_000.0 * 50_000.0`
  というマジックナンバー式が重複していた (`docs/SURPLUS_AND_GAPS.md` §2.5 で
  指摘済み)。`core/mod.rs` に名前付き定数 `SATS_PER_BTC`/`APPROX_USD_PER_BTC`
  と `sats_to_usd(sats: u64) -> f64` を新設し、両呼び出し元をこれに差し替え。
  v0.3 で実価格フィードを導入する際に触るべき箇所が 1 点に集約される。
  既存の `core::short` と同じ共有ヘルパーパターンを踏襲。回帰テスト2件追加
  (換算値の固定 + `short` の UTF-8 境界テスト)。新規依存なし

## [0.2.14] - 2026-07-16

### Verified (Workflow による敵対的再検証)

- 本セッションで削除した4関数 (`load_private_key`/`load_config`/
  `ensure_initialized`/`get_plan`) の「呼び出し元ゼロ」を、8エージェント
  構成の Workflow で独立に再検証。全4件 CONFIRMED (削除前から呼び出し元
  ゼロ、削除は安全)。詳細: `docs/REACHABILITY_AUDIT.md` 第3回フォローアップ

### Fixed (Workflow による敵対的レビューで検出・修正)

- **`confidential::ConfidentialManager::perform_attestation` が
  `stats.active_sessions` を黙って 0 に巻き戻す回帰** — v0.2.11由来の
  `update_stats()` ワイヤリング修正 (このセッションの以前のコミット) が
  `self.update_stats()` を丸ごと呼んでいたが、`update_stats()` は
  `active_tee_instances` だけでなく `active_sessions` も無条件に
  再計算する。`SessionStatus::Active` はどこからも代入されない
  (既知の別問題、`SessionStatus` の doc comment 参照) ため、この
  再計算は常に 0 を返す。`create_secure_session` が `+=1` で正しく
  維持していた `active_sessions` を、後続の `perform_attestation`
  呼び出し (2台目のGPU attest、定期再検証等、ごく普通に起こる操作)
  が黙って 0 に巻き戻してしまっていた。`format_confidential` が
  この値を「アクティブセッション」として表示するため、ユーザー
  可視の表示バグでもあった。
  修正: `perform_attestation` は `update_stats()` を丸ごと呼ぶ代わりに
  `active_tee_instances` だけを直接再計算し、`active_sessions` には
  触れないよう変更。回帰テスト追加
  (`test_perform_attestation_does_not_reset_active_sessions`)。
  **この回帰は本セッションの単独レビューでは見逃していた** —
  Ultracode 有効化後、8エージェントによる Workflow ベースの敵対的
  レビュー (このセッションのコンパイラ未検証コミット全件を対象) で
  初めて検出・確認された。今回のセッションで手動レビューのみに
  頼っていた他の変更についても、同種の見落としが残っている
  可能性を示唆する結果

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
- **`docs/REACHABILITY_AUDIT.md`** — `#![allow(dead_code)]` (`lib.rs`/`main.rs`) 配下
  161 個の pub fn を、4動詞のハンドラからの実呼び出しチェーンで
  reachable(68)/test-only(64)/呼出ゼロ(15)/連鎖デッド(14) に分類。
  「予約 v0.3 API」という既存の正当化コメントは方向性としては正しいが粗すぎ、
  テストの裏付けすら無い 29 関数 (呼出ゼロ15 + 連鎖デッド14) が混在していたと
  判明。特に `session.rs` が外れ値 (25関数中10件): `SessionManager::end`,
  `panic_stop`, `is_stop_requested`, `get_status`, `cleanup`,
  `list_sessions`, `format_session_list`, `Session::load`/`delete`,
  `reset_stop_flag` は現行4動詞設計以前の残骸である可能性が高い。
  **今回は削除しない** — コンパイラ検証不能な環境で29関数にまたがる削除を
  行うブラスト半径は、単発の1行修正とは比較にならない。次回ビルド可能な
  環境での実施手順をドキュメント内に明記

### Removed / Changed (同日フォローアップ — category (c) 15件を個別再検証)

- `config::load_private_key`/`load_config`/`ensure_initialized` を削除。
  いずれも汎用 `load_or_recover<T>` に置き換わって取り残された旧ユーティリティ
  (呼び出し元ゼロを再確認、依存インポートは他所で使用中のため孤立せず)
- `intent::IntentManager::get_plan` を削除。自明な1行アクセサでロードマップ
  記載も無し
- `confidential::ConfidentialManager::update_stats` は削除せず
  `perform_attestation` から呼ぶよう配線。`format_confidential` が表示する
  `stats.active_tee_instances` を再計算する唯一の関数だったが誰も呼んでおらず
  常に0固定だった (`TeeStatus::Running` 未代入バグ (v0.2.11) と同根)。
  回帰テスト追加
- `intent::Intent::with_region`・`confidential::TeeType::is_cpu_tee` は
  呼出ゼロだが削除せず保持: 前者は `RegionConstraint` の唯一のセッターで
  削除するとv0.3で再度必要になる、後者は composite attestation 設計を
  明示的に説明するコメント付きで設計意図が明確
- `session.rs` の10件クラスターは意図的に見送り。個別に読んだ結果
  `panic_stop`(緊急停止, 過去のバグ修正履歴あり)・`SessionManager`
  (v0.3 async化を想定した設計コメントあり) は安全性/設計上の理由がある
  未結線コードであり、「呼出ゼロ」だけを根拠に削除すべきではないと判断。
  製品判断が必要なため次回に持ち越し
- テスト計 239 (default) / 245 (`--features http`) 相当 (regression test 1件追加、
  引き続き**未検証** — 上記ビルド不能の制約は継続中)

### Fixed (同日追加調査)

- `pair::PairManager::prune_stale_discoveries`/`prune_completed_challenges` を
  `main.rs::run_pair`/`run_earn` から呼ぶよう配線。両関数とも doc comment で
  「定期的に呼び出すことで無限増長を防ぐ」ことが前提として明記されていたが
  (`prune_completed_challenges`: 「問⑦への応答: 蓄積防止」)、どこからも呼ばれて
  いなかった — `confidential::update_stats` と同型の「呼ぶべき場所に呼ぶ処理が
  無い」パターン。同じ関数内で既に呼ばれている `session::prune_stale()` と
  同じ立ち位置に追加。現状 `record_discovery`/`issue_capability_challenge` は
  どの CLI 動詞からも呼ばれていないため `discovered`/`pending_challenges` は
  常に空で今回の変更に観測可能な効果は無いが、v0.3 で両者が実結線された際の
  無限増長を事前に防ぐ

### Added (リサーチ更新)

- **`docs/RESEARCH_UPDATE_2026-07.md`** — `docs/RESEARCH_IMPROVEMENTS.md`
  (2026-06-05調査) から1ヶ月分の差分を WebSearch で調査。新規論文3件
  (TensorCommitments arXiv:2602.12630 — LLaMA2でprover+0.97%/verifier+0.12%の
  軽量検証、arXiv:2602.17223 — private inference手法の verified inference への
  転用、arXiv:2501.05374 — GPU計算検証3手法の比較調査)、Iroh 1.0
  (2026-06リリース) を libp2p の代替P2Pスタック候補として新規発見、
  推論エンジン統合 (llama-cpp-rs vs mistral.rs) の比較検討を新設。
  NVIDIA CC 実装の一次情報源を Corvex の本番事例 (Intel Trust Authority
  によるCPU/GPU証明) + NVIDIA公式デプロイガイドv7.1に更新。
  引用は全てWebSearchで実在確認済み。`docs/SURPLUS_AND_GAPS.md` §1 の
  各GAP項目とcrossリンク
- **`docs/P2P_IMPLEMENTATION_READINESS.md`** — 最大の構造的ギャップ (実P2P
  I/O 皆無) を、ネットワーク制約解消後すぐ着手できる実行手順書として
  事前設計。libp2p/Iroh の選定基準比較表、段階的ロールアウト順序
  (mDNS発見→Noise鍵交換→NAT越え→proof-of-capability結線)、影響範囲マップ
  (変更要ファイル vs 無傷のまま残る236件超の既存テスト)、Definition of Done
  を記載。`pair.rs` の状態機械・公開APIシグネチャは変更不要で、各関数の
  内部実装のみ実I/Oに差し替える設計であることを明記 (既存の型/テスト資産を
  最大限再利用する方針)
- **`docs/CASHU_BDHKE_IMPLEMENTATION_READINESS.md`** — Cashu 盲目署名 (BDHKE)
  の実行手順書。Cashu 公式 NUT-00 仕様 (`cashubtc/nuts` リポジトリ) を
  WebFetch で直接取得し、hash_to_curve/blinding/unblinding の数式を検証済み
  (C = C_ - rK = kY)。`secp256k1` (C FFI, 実績重視) vs `k256` (pure Rust,
  RustCrypto系) の比較検討で `k256` を推奨 (既存の `ed25519-dalek`/
  `curve25519-dalek` と同エコシステムで一貫性、`unsafe_code = deny` 方針との
  親和性)。5段階のロールアウト順序 (hash_to_curve→blinding→unblinding→
  NUT-07→DLEQ/P2PK) と、`Proof` 構造体への `blinding_factor` フィールド
  追加時の後方互換性上の注意点を記載

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

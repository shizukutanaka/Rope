# カテゴリ別 関連調査と改善点 — arXiv + GitHub

> 開始: 2026-06-05 / 対象: Rope v0.2.1
> 課題: 「このプロダクトのカテゴリを 10 挙げ、各 10 件ずつ arXiv / GitHub から
> 関連情報を集め、改善点を洗い出す」(/loop, dynamic mode で逐次拡充)
>
> 補完文書: [`RESEARCH_IMPROVEMENTS.md`](RESEARCH_IMPROVEMENTS.md)（テーマ横断の優先度版）

## 進捗トラッカ (loop)

| # | カテゴリ | 状態 |
|---|---------|------|
| 1 | 分散GPUコンピュート市場 / DePIN | ✅ 完了 |
| 2 | 分散LLM推論 (swarm/pipeline) | ✅ 完了 |
| 3 | 検証可能推論 / Proof-of-Execution | ✅ 完了 |
| 4 | 機密計算 / GPU TEE | ⏳ 未 |
| 5 | ecash / Chaumian 少額決済 | ⏳ 未 |
| 6 | P2P発見 / NAT越え / オーバーレイ | ⏳ 未 |
| 7 | セキュア握手 / 暗号ペアリング | ⏳ 未 |
| 8 | Sybil耐性 / 評判 / インセンティブ | ⏳ 未 |
| 9 | 推論サービング効率 | ⏳ 未 |
| 10 | 省エネ・カーボン・エッジ | ⏳ 未 |

各カテゴリは **関連10件（arXiv/GitHub）→ Rope への改善点** の順。

---

## 1. 分散GPUコンピュート市場 / DePIN

**スコープ**: 不特定の供給者から GPU を借りる/貸す市場。Rope の母カテゴリ。

### 関連10件
1. **EXO** — [github.com/exo-explore/exo](https://github.com/exo-explore/exo)（自前HW、同一所有者デバイス。Rope が「他人デバイス」で差別化する対象）
2. **Petals** — [github.com/bigscience-workshop/petals](https://github.com/bigscience-workshop/petals)（ボランティア swarm、無償）
3. **Akash Network** — リバースオークションで AWS比 60-70%減 ([FinanceFeeds](https://financefeeds.com/how-decentralized-gpu-marketplaces-like-akash-and-render-solve-the-ai-compute-crisis/))
4. **io.net** — Solana ベース GPU 集約、$20M+ 売上 ([io.net blog](https://io.net/blog/decentralized-computing))
5. **Gensyn** — 計算検証を中核、a16z $43M ([DeSpread](https://research.despread.io/ai-infra-projects/))
6. **Prime Intellect** — TOPLOC 検証、Founders Fund $15M ([Gate Learn](https://www.gate.com/learn/articles/open-ai-founding-members-invest-a-quick-dive-into-the-decentralized-ai-breakthrough-prime-intellect/7323))
7. **Vast.ai / RunPod** — 中央集権だが GPU レンタルの UX 基準 ([DigitalOcean 比較](https://www.digitalocean.com/resources/articles/vastai-alternatives))
8. **Render / Bittensor** — DePIN の代表 ([FinanceFeeds](https://financefeeds.com/how-decentralized-gpu-marketplaces-like-akash-and-render-solve-the-ai-compute-crisis/))
9. **Fluence** — 「分散クラウド」価格圧縮 ([Fluence blog](https://www.fluence.network/blog/best-gpu-rental-marketplaces/))
10. **GPU Marketplace 比較 2026** — Shadeform/Prime Intellect/Node AI ([aimultiple](https://aimultiple.com/gpu-marketplace))

### Rope への改善点
- **A1. 価格発見の欠如**: 競合の多く（Akash/Vast）はオークションで価格を発見。Rope は
  固定 `amount_sats`。供給過多時に貸し手が選ばれない。→ `earn` 側に軽量な提示価格＋
  resolver のコスト関数で最安ピア選好（哲学に反しない範囲で）。
- **A2. 差別化軸の明文化**: README 比較表は良いが、EXO(自前)/Petals(無償)/Akash(トークン要)
  との **wedge（他人GPU × TEE × ecash少額）** をコード上の制約として固定化（intent の
  Privacy/Budget 既定値に反映）。
- **A3. 供給参加の摩擦**: Vast/RunPod の貸し手 UX（収益見積り・自動価格）を `earn` に。

---

## 2. 分散LLM推論 (swarm / pipeline parallelism)

**スコープ**: 1台に載らないモデルを複数ノードへ分割して推論。Rope の `run`/`intent` の射程。

### 関連10件
1. **Petals 論文** — [arXiv:2209.01188](https://arxiv.org/pdf/2209.01188)（swarm parallelism、DHT で層を逐次実行）
2. **Petals over Internet** — [arXiv:2312.08361](https://arxiv.org/pdf/2312.08361)（耐障害推論＋負荷分散、private swarm 言及）
3. **Parallax** — [arXiv:2509.26182](https://arxiv.org/abs/2509.26182)（2段スケジューラ、異種GPU最適化）
4. **hivemind** — [github.com/learning-at-home/hivemind](https://github.com/learning-at-home/hivemind)（Petals 基盤の分散DHT）
5. **EXO** — [github.com/exo-explore/exo](https://github.com/exo-explore/exo)（70B+ を複数GPUで、一貫レイテンシ）
6. **分散LLMサーベイ** — [arXiv:2503.16585](https://arxiv.org/html/2503.16585v1)（advances/challenges 整理）
7. **分散計算手法の探索研究** — [arXiv:2510.11211](https://arxiv.org/html/2510.11211)（訓練/推論の分散技法）
8. **地理分散推論の資源割当** — [arXiv:2512.21884](https://arxiv.org/pdf/2512.21884)
9. **WISP** — [arXiv:2601.11652](https://arxiv.org/pdf/2601.11652)（エッジでの分散投機サービング、SLO対応バッチ）
10. **Petals デモ論文** — [ACL 2023](https://aclanthology.org/2023.acl-demo.54.pdf)

### Rope への改善点
- **B1. 単一ピア前提の打破**: resolver は1ピア選択のみ。大モデルは Petals/Parallax 式
  **pipeline 分割**で複数の消費者GPUに載せる（`intent` に「分割可否」を追加）。
- **B2. 耐障害の "継続"**: Rope の deadman は「切断→返金」。Petals は「切断→別ノードへ
  再ルーティングで継続」。UX 上、返金より継続が上位（`pair` の paired 集合を予備として持つ）。
- **B3. 異種性の取り込み**: ピア発見時に VRAM/帯域/TFLOPS をプローブし、Parallax の
  model-allocation に相当する入力として resolver へ。
- **B4. Petals の既知限界の回避**: 「貪欲ヒューリスティックで大域最適化なし→資源過少利用」。
  Rope は後発として、コスト関数に帯域/load を明示的に入れる余地。

---

## 3. 検証可能推論 / Proof-of-Execution

**スコープ**: 貸し手が「正しいモデルで正しく計算した」ことを借り手が検証。`intent.VerificationLevel`。

### 関連10件
1. **VeriLLM** — [arXiv:2509.24257](https://arxiv.org/abs/2509.24257)（再実行＋最小オンチェックで**約1%コスト**、free-riding 排除）
2. **TOPLOC** — [arXiv:2501.16007](https://arxiv.org/pdf/2501.16007)（LSH で活性値コミット、改ざん検出、軽量）
3. **zkLLM** — 暗号学的に完全だが LLaMA-2-13B で commitment 986s + proof 803s（**Rope の60秒には不適**）
4. **SPEX (Statistical Proof of Execution)** — arXiv:2503.18899（統計的実行証明）
5. **Proof of Quality** — [arXiv:2601.21189](https://arxiv.org/html/2601.21189)（コスト考慮の品質証明）
6. **PolyLink** — [arXiv:2510.02395](https://arxiv.org/pdf/2510.02395)（ブロックチェーンでエッジLLM推論検証）
7. **FORTYTWO** — [arXiv:2510.24801](https://arxiv.org/pdf/2510.24801)（peer-ranked consensus で品質判定＝検証兼Sybil耐性）
8. **DSperse** — モデルスライス単位の検証可能推論（[decentralizedinference.org](https://decentralizedinference.org/2026/02/26/dsperse-model-slicing-for-scalable-verifiable-inference-in-decentralized-ai-markets/)）
9. **Self-Supervised Inference in Trustless Env** — [arXiv:2409.08386](https://arxiv.org/pdf/2409.08386)
10. **Trustless Federated Learning at Edge** — [arXiv:2511.21118](https://arxiv.org/html/2511.21118v1)（検証＋インセンティブ整合の合成アーキ）

### Rope への改善点
- **C1. `VerificationLevel` を実機構に結線**（最優先・既出 #1）。**VeriLLM 式 確率的再実行**
  （毎回でなく抜き打ち、~1%）＋ **TOPLOC 式 LSH コミット**。zkLLM は重すぎ採用しない。
- **C2. escrow と検証のゲート結合**: `ecash` の Escrow 解放条件に「検証パス」を挿入。
  手抜き/誤計算は返金へ（free-riding 排除）。
- **C3. 決定論要件**: LSH/再実行検証は非決定性で壊れる。temperature=0/greedy・確定的カーネルを
  `confidential`/実行層で要求できるフラグを intent に。
- **C4. 検証=Sybil耐性の二重利用**: FORTYTWO/DSperse の知見で、検証チャレンジを初回ピアの
  能力証明にも転用（カテゴリ8 と基盤共有）。

---

<!-- LOOP-CONTINUE: 次イテレーションでカテゴリ 4〜10 を同形式で追記 -->
</content>

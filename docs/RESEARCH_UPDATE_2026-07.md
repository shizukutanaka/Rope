# リサーチ更新 (2026-07) — 最新論文・業界動向の反映

> 調査日: 2026-07-10 (WebSearch 実施) / 前回調査 (`RESEARCH_IMPROVEMENTS.md`,
> `docs/CATEGORY_RESEARCH.md`): 2026-06-05。約1ヶ月の差分を対象に、
> ユーザー指示「関連最新論文、動画、情報を調べて改善点を洗い出す」に基づき実施。
>
> 位置づけ: 既存2文書を置き換えるのではなく差分アップデート。引用は全て
> WebSearch で実在確認済み (タイトル・識別子・要旨の一致を確認)。
> 動画は明示的に検索したが、この分野の専門解説動画はヒット数が少なく、
> 見つかった2件も本題そのものではなく周辺トピックだったため「参考」扱いとする。

---

## 1. 検証可能推論 (`docs/RESEARCH_IMPROVEMENTS.md` #1 の更新) — 優先度: 最高、変わらず

### 新規論文 (前回調査には無かった)

| 論文 | 提出日 | 要旨 | Rope への意味 |
|---|---|---|---|
| **TensorCommitments** [arXiv:2602.12630](https://arxiv.org/abs/2602.12630) | 2026-02-13 | Tensor-native な proof-of-inference。LLM の層/ヘッド構造に沿った multivariate Terkle Tree でコミット。LLaMA2 で **prover +0.97%、verifier +0.12%** のオーバーヘッドのみ、かつ既存最良の verifier-GPU 方式より攻撃耐性 +48%。 | VeriLLM (再実行方式、~1%コスト) と同水準のオーバーヘッドで、コミットメント方式のため **再実行そのものが不要** — 60秒デモの体験を壊しにくい。既存推奨 (TOPLOC+VeriLLM) の**有力な代替/併用候補**として追加検討に値する。 |
| **NANOZK** [arXiv:2603.18046](https://arxiv.org/pdf/2603.18046) | 2026-03 | Layerwise ゼロ知識証明による LLM 推論検証。 | zkLLM (旧調査で「LLaMA-2-13Bで1789秒、実用外」と却下) の後継系だが、layerwise 分割によりコスト構造が変わっている可能性。**要詳細検証** (今回は概要のみ確認、フルコスト評価は未実施)。 |
| **Privacy-Preserving Mechanisms Enable Cheap Verifiable Inference of LLMs** [arXiv:2602.17223](https://arxiv.org/abs/2602.17223) | 2026-02-19 | 核心的な洞察: **private LLM inference の手法を流用すると、限界コストのみで verified inference が得られる**。ZKP不要。2つの新プロトコルを提案。 | Rope の「プライバシー (TEE)」と「検証」は現状**別々の機構**として設計されているが、この論文は両者を**同じ基盤で統合できる**可能性を示す。`docs/RESEARCH_IMPROVEMENTS.md` #1 と #2 (TEEギャップ) を同時に前進させうる **アーキテクチャ変更を要する大きな検討事項** として新規追加。 |
| **Validation of GPU Computation in Decentralized, Trustless Networks** [arXiv:2501.05374](https://arxiv.org/pdf/2501.05374) | 2025-01 (前回調査の直前だが未収録) | 3方式 (モデルフィンガープリンティング、意味的類似度分析、GPUプロファイリング) を比較。**exact recomputation は GPU間の非決定性で破綻、TEEは専用HW必須、FHEはコスト過大**、と明記。 | Rope が既に「決定論的推論が VeriLLM 系再実行の前提」と認識している点 (#1改善案3) を**裏付ける**独立文献。GPUプロファイリングによる異常検知は、proof-of-capability (`pair.rs`) の追加シグナルとして軽量に組み込める可能性がある — 新規改善案として追加。 |

### 産業実装の動き (前回調査に無かった)

- **Theta EdgeCloud** — ブロックチェーンの公開ランダムネスビーコンを使った distributed verifiable LLM inference サービスを提供開始。Rope が「検証」を独自実装する代わりに、既存の verifiable-inference サービス層を **委譲先として利用する可能性**も設計上排除しない方が良い (ただし Rope の「独自トークン不要」哲学とブロックチェーン依存は緊張関係にある点に注意)。

### 改善案 (更新)

1. (既存) TOPLOC 風 LSH コミット + VeriLLM 風確率的再実行。
2. **[新規]** TensorCommitments のコミットメント方式を、再実行不要の代替として比較検討。60秒デモとの相性 (再実行の待ち時間が要らない) で有利な可能性。
3. **[新規]** arXiv:2602.17223 の「private inference→verified inference」統合アイデアを、TEE (#2) と検証 (#1) の設計統合案として検討事項に追加。
4. (既存) 決定論的推論の必要性 — arXiv:2501.05374 により独立に裏付けられた。

**優先度: 最高、変わらず。** ただし新規3論文により「TOPLOC/VeriLLM 一択」から
「複数の実現方式を比較検討すべき」段階に移った。

---

## 2. GPU TEE / attestation (`docs/RESEARCH_IMPROVEMENTS.md` #2, #3 の更新) — 優先度: 最高、変わらず

### 産業動向

- **Corvex, Inc.** が 2026-03-03、NVIDIA HGX B200 (Blackwell) 上での confidential
  computing を**本番環境で検証済みデプロイ**したと発表。NVSwitch/NVLink 間の
  暗号化通信 + **Intel Trust Domain Extensions と Intel Trust Authority (ITA) による
  CPU/GPU リモートアテステーション**を実運用。
  ([PR Newswire](https://www.prnewswire.com/news-releases/corvex-among-the-first-companies-to-achieve-verified-production-deployment-of-confidential-computing-for-ai-on-nvidia-hgx-b200-systems-302702992.html))
- NVIDIA・Apple・Google が Blackwell CC を Apple Intelligence の Private Cloud
  Compute に採用 ([NVIDIA Blog](https://blogs.nvidia.com/blog/nvidia-confidential-computing-apple-private-cloud-compute/)) —
  「TEE プライバシー」訴求の市場的な裏付けが強化された。
- CC のオーバーヘッドは業界公表値で **2-5%** (transformer 推論)。
- NVIDIA 公式デプロイガイド `docs.nvidia.com/cc-deployment-guide-tdx.pdf`
  (2026-04 版, DU-12302-001_v7.1) が既存の `docs/CATEGORY_RESEARCH.md` の
  NVIDIA nvtrust 系リンクを補完する一次情報源として利用可能。

### Rope への意味

- `src/core/confidential.rs` の `attestation_service_url()` は現状 NVIDIA NRAS と
  Intel の汎用エンドポイントを指しているが、**Corvex の実運用構成 (Intel Trust
  Authority を GPU 側にも使う設計)** は、実装時の一次リファレンス実装として
  より具体的。実装フェーズで `docs.nvidia.com/cc-deployment-guide-tdx.pdf`
  (2026-04版) と Intel Trust Authority のドキュメントを直接参照すべき、と
  優先実装先を具体化できる。
- 「消費者GPUはCC非対応」という Rope 自身の弱点認識 (#2) は業界動向からも
  変わっていない — Corvex/Apple の事例はいずれもデータセンター GPU (B200) で
  あり、コンシューマ RTX 側の CC 対応拡大の兆候は見つからなかった
  (誇大表示リスクの是正は引き続き必須)。

**優先度: 最高、変わらず。** 実装時の一次情報源が更新された (nvtrust の
古い版から `cc-deployment-guide-tdx.pdf` v7.1 + Intel Trust Authority へ)。

---

## 3. Cashu / ecash (`docs/RESEARCH_IMPROVEMENTS.md` #4 の更新) — 優先度: 高、変わらず

### 新規 NUT / CDK リリース

- **NUT-28 (Pay-to-Blinded-Key, P2BK)** — 新規。既存調査は NUT-11 (P2PK) を
  「escrow をジョブ公開鍵に暗号学的束縛する」ための改善案として挙げていたが、
  P2BK はその発展形の可能性がある。**要個別調査** (今回は存在確認のみ、
  NUT-11 との使い分けは未検証)。
- **NUT-29 (Batch Minting)** — mint 発行のバッチ化。Rope の「1秒単位課金」
  streaming (`ecash.rs::tick_stream`) の効率化に使える可能性。
- **NUT-26 (Payment Request)**、BIP-353 (Lightning アドレス解決)、
  BIP-321 (Bitcoin Payment Request のパース/生成) — CDK の CLI 機能として
  追加。Rope 自体の QR ペアリング (`pair.rs`) とは別レイヤーだが、将来
  ecash 側の QR/URL ベース決済フローを設計する際の参考になる。

### 改善案 (更新)

1. (既存) BDHKE 実装 (secp256k1) — ネットワーク制約で本セッションは着手不能、変わらず。
2. **[新規]** NUT-28 (P2BK) が escrow の鍵束縛に NUT-11 より適するか、
   実装着手前に比較検討する項目として追加。
3. (既存) DLEQ (NUT-12) — 変わらず未実装。

**優先度: 高、変わらず。**

---

## 4. P2P / NAT越え (`docs/RESEARCH_IMPROVEMENTS.md` #5 の更新) — 優先度: 高、**推奨スタックの見直しを提案**

### 最重要の新規発見: Iroh 1.0 (2026年6月、本番運用可能版としてリリース)

- **Iroh** ([iroh.computer](https://www.iroh.computer/)) が v1.0.0 に到達
  (2026年6月)。QUIC ベースで NAT 越え (hole punching)・自動リレーフォールバック・
  エンドツーエンド暗号化 (QUIC/TLS 1.3) を**単一ライブラリで統合**して提供。
  公開鍵でダイヤルするだけで NAT 越え・リレー・経路最適化が自動化される
  ([比較記事](https://www.iroh.computer/blog/comparing-iroh-and-libp2p))。
- **穴あけ成功率で libp2p を上回る**との主張: 「libp2p のホールパンチング成功率は
  上限約70%、Iroh は Tailscale が開拓した手法を応用しさらに高い成功率」
  ([StackRadar 記事](https://stackradar.tech/posts/iroh-1-0-released-a-new-era-for-peer-to-peer-data-transfer-mqg226cf))。
  この数値は Iroh 側の発信源による比較であり中立的な第三者ベンチマークでは
  ないため **要検証**、ただし libp2p 自体の資料 (`docs/CATEGORY_RESEARCH.md`
  引用の Ethereum2.0 Lighthouse 実績) でも Endpoint-Independent NAT で
  85-95%、Symmetric NAT で 5-30% とばらつきが大きく、Iroh の主張は
  この弱点 (symmetric NAT) を直接狙ったものと解釈できる。
- `libp2p-iroh` ([crates.io](https://crates.io/crates/libp2p-iroh)) という
  実験的ブリッジも存在 (2025年10月時点) — Iroh の QUIC 接続を libp2p の
  transport として使う統合レイヤー。まだ実験段階。

### Rope への意味

`docs/IMPROVEMENT_SYNTHESIS.md` Part 1 は libp2p (`tcp/quic/noise/identify/
autonat/dcutr/relay/yamux/kademlia` の約50依存) を前提に実装コードまで
用意している。Iroh は:

- **依存が少ない** (単一クレート、QUIC ネイティブ、Noise 実装済み内蔵) —
  Rope の「最小依存主義」(`README.md` 設計原則 #3 Subtraction) とより整合する。
- ただし **mDNS/Bluetooth 発見・DHT** など Rope が `pair.rs` の状態機械で
  既に前提としている発見方式との統合方法は未調査。libp2p は Kademlia DHT を
  含むが Iroh の発見機構は別設計の可能性がある。
- **Noise 実装が既に内蔵**されているなら、`docs/RESEARCH_IMPROVEMENTS.md` #5 の
  「Noise 握手を snow で自前配線」という改善案自体が不要になる可能性がある
  (Iroh 採用時)。これは実装方針の分岐点であり、**新規依存追加が可能になった
  次のセッションで、libp2p 案と Iroh 案を並べて比較検討することを強く推奨**。

### 改善案 (更新)

1. (既存) libp2p (AutoNAT + DCUtR + Relay v2) — 実装コードは
   `docs/IMPROVEMENT_SYNTHESIS.md` に準備済み。
2. **[新規・要検討]** Iroh 1.0 を代替候補として比較評価。依存数の少なさと
   symmetric NAT 耐性の主張が Rope の設計原則と噛み合う。mDNS/Bluetooth
   発見層との統合方法が要調査ポイント。
3. (既存) snow による Noise XX/IK 自前配線 — Iroh 採用時は不要になりうる。

**優先度: 高、変わらず。ただし推奨実装手段の再検討を新規に提起。**

---

## 5. Sybil耐性 / 評判 (`docs/RESEARCH_IMPROVEMENTS.md` #10 の更新) — 優先度: 高、変わらず

- **Game of Coding** [arXiv:2410.05540](https://arxiv.org/abs/2410.05540)
  (2026-01 に更新版) — 「敵対ノードが増えても悪用可能性は増大しない」ことを
  示す sybil耐性のある分散ML手法。Rope の既存 proof-of-capability
  (`pair.rs::CapabilityChallenge`, FORTYTWO式) とは異なるアプローチ
  (符号化理論ベース) — 直接の実装対象ではないが、理論的な参照先として
  `docs/RESEARCH_IMPROVEMENTS.md` #10 に追加する価値がある。

**優先度: 高、変わらず。新規論文は理論的補強のみで実装方針への直接影響は無し。**

---

## 6. 推論エンジン統合 (新規カテゴリ — 既存文書に独立した節が無かった)

既存の `docs/RESEARCH_IMPROVEMENTS.md` #1/#11/#12 は検証・配布・効率化を
扱うが、**「そもそも Rust から推論エンジンをどう呼ぶか」自体の選択肢比較が
既存文書に無い**ことに気づいた。この欠落自体が改善点。

| 選択肢 | 特徴 | Rope との相性 |
|---|---|---|
| **llama-cpp-rs** ([eugenehp/llama-cpp-rs](https://github.com/eugenehp/llama-cpp-rs)) | llama.cpp への薄い Rust バインディング (FFI)。2026年6月版で TurboQuant・投機的マルチトークン予測に対応。 | 実績豊富だが C++ ビルド依存が増える (Rope は現状 `unsafe_code = deny` を crate 全体に強制 — FFI ラッパー自体は `unsafe` を内包するため、境界の扱いに設計判断が要る)。 |
| **mistral.rs** ([EricLBuehler/mistral.rs](https://github.com/ericlbuehler/mistral.rs)) | **純 Rust実装**(FFIでなく再実装)。Apple Silicon/CPU/CUDA 対応、マルチモーダル。`mistralrs` crate で埋め込み可能。 | Rope の「pure Rust」志向 (現状 `core/` は sync/no I/O の pure Rust) とビルドの単純さの面で有利な可能性。ただし性能・量子化対応の成熟度は llama.cpp 系に劣る可能性があり要比較検証。 |

### 改善案 (新規)

1. **[新規]** `docs/RESEARCH_IMPROVEMENTS.md` に「推論エンジン統合方式の比較」
   節を新設し、llama-cpp-rs (FFI, 実績重視) vs mistral.rs (pure Rust, ビルド
   単純性重視) を明示的に比較検討する。現状「実推論エンジン自体が未統合」
   とだけ記録されており、**どちらを採用するかの検討軸が無かった**。

**優先度: 中〜高 (#1 検証機構の前提条件でもあるため、着手時期は#1と連動)。**

---

## 7. DePIN GPU市場の動向 (`docs/CATEGORY_RESEARCH.md` #1 の更新) — 記録のみ、Rope の方針に直接影響なし

- Akash: 2026年Q1で月間$500万の compute 支出、利用率80%超、NVIDIA GB200
  GPU 最大7,200基を Starbonds 経由で確保予定。AkashML で日次17億トークン処理。
- io.net: 300,000超のGPUを55ヶ国以上で集約。2026 Q2に Incentive Dynamics
  Engine (供給量50%削減目標のトークノミクス再設計) を投入。
- 業界全体で「トークン投機からの卒業、実収益への転換」がテーマ化
  ([BlockEden.xyz](https://blockeden.xyz/blog/2026/04/12/depin-revenue-pivot-token-subsidies-ai-compute-akash-render-ionet/))。

**Rope への意味**: Rope の「独自トークン不要」という差別化軸 (README比較表)
は、この業界トレンド (トークン中心モデルからの脱却) とむしろ**方向性が
一致**しており、既存の設計判断を裏付ける。新規の改善案は無し、記録のみ。

---

## 8. 動画 (参考、本題ではないが検索指示に基づき記録)

- [State of LLMs 2026: RLVR, GRPO, Inference Scaling — Sebastian Raschka](https://www.youtube.com/watch?v=K5WPr5dtne0)
  — LLM 全般の2026年動向解説。Rope の検証/推論トピックと間接的に関連するが
  P2P/分散推論に特化した内容ではない。
- [Optimizing LLM Inference for the Rest of Us — Abdel Sghiouar (Google)](https://www.youtube.com/watch?v=xLum3amp6h0)
  — 推論最適化の一般解説。`docs/RESEARCH_IMPROVEMENTS.md` #12 (プロバイダ側
  推論効率) の参考になりうるが、Rope 固有の分散/検証文脈は扱っていない。

いずれも「Rope の改善点」に直結する専門動画ではなかった。分散GPU検証・
P2Pペアリングに特化した動画コンテンツは今回のWeb検索では見つからず
(この分野はまだ論文・ブログが主要な情報源で、動画解説はニッチなカンファレンス
発表以外に少ない可能性が高い)。

---

## 9. 優先度サマリ (今回の差分のみ)

| 変化 | 項目 | 詳細 |
|---|---|---|
| 🆕 新規検討事項 | 検証方式に TensorCommitments/NANOZK/プライバシー統合型検証を追加 | §1 |
| 🆕 新規検討事項 | P2P スタックを libp2p 一択から libp2p vs Iroh 比較へ | §4 |
| 🆕 新規節 | 推論エンジン統合方式 (llama-cpp-rs vs mistral.rs) の比較検討 | §6 |
| 🔧 一次情報源更新 | TEE実装時の参照を NVIDIA CC Deployment Guide v7.1 (2026-04) + Intel Trust Authority に更新 | §2 |
| 🔧 検討候補追加 | Cashu NUT-28 (P2BK) が escrow 鍵束縛の代替になりうるか要調査 | §3 |
| ✅ 既存判断の裏付け | 「決定論的推論が検証の前提」「独自トークン不要の方針」は独立文献/業界動向により裏付け強化 | §1, §7 |
| 変化なし | 優先度自体 (#1検証・#2TEEギャップが最高) は不変 | — |

すべて `docs/SURPLUS_AND_GAPS.md` §1 (GAPS) の各項目とリンクする —
実装がネットワーク制約 (§0) の解消を前提とする点も変わらない。

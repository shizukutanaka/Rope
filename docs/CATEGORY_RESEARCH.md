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
| 4 | 機密計算 / GPU TEE | ✅ 完了 |
| 5 | ecash / Chaumian 少額決済 | ✅ 完了 |
| 6 | P2P発見 / NAT越え / オーバーレイ | ✅ 完了 |
| 7 | セキュア握手 / 暗号ペアリング | ✅ 完了 |
| 8 | Sybil耐性 / 評判 / インセンティブ | ✅ 完了 |
| 9 | 推論サービング効率 | ✅ 完了 |
| 10 | 省エネ・カーボン・エッジ | ✅ 完了 |

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

## 4. 機密計算 / GPU TEE

**スコープ**: 処理中データを GPU 内で暗号化し、貸し手にもプロンプトを見せない。`confidential.rs`。

### 関連10件
1. **NVIDIA nvtrust** — [github.com/NVIDIA/nvtrust](https://github.com/NVIDIA/nvtrust)（CC 補助OSS）
2. **local_gpu_verifier** — [nvtrust/.../local_gpu_verifier](https://github.com/NVIDIA/nvtrust/blob/main/guest_tools/gpu_verifiers/local_gpu_verifier/README.md)（**SPDM 1.1 MEASUREMENT** を解析、署名検証、RIM golden 値と突合）
3. **Attestation SDK (Python)** — [nvtrust/.../attestation_sdk](https://github.com/NVIDIA/nvtrust/tree/main/guest_tools/attestation_sdk)（attestation API、PyPI 配布）
4. **ppcie-verifier** — [nvtrust/.../ppcie-verifier](https://github.com/NVIDIA/nvtrust/tree/main/guest_tools/ppcie-verifier)（マルチGPU/PCIe スイッチ構成）
5. **NVIDIA Attestation docs** — [docs.nvidia.com/attestation](https://docs.nvidia.com/attestation/index.html)（**NRAS + RIM + OCSP**）
6. **HCC Whitepaper** — [PDF](https://images.nvidia.com/aem-dam/en-zz/Solutions/data-center/HCC-Whitepaper-v1.0.pdf)（CVM=SEV-SNP/TDX 前提、Hopper+ 限定）
7. **GPU CC Demystified** — [arXiv:2507.02770](https://arxiv.org/html/2507.02770v1)（初の詳細セキュリティ解析）
8. **H100 CC 性能ベンチ** — [arXiv:2409.03992](https://arxiv.org/pdf/2409.03992v2)（CPU-GPU 転送=PCIe 暗号がボトルネック）
9. **First Confidential GPUs** — [ACM Queue](https://queue.acm.org/detail.cfm?id=3623391)
10. **Intel Trust Authority GPU attestation** — [docs](https://docs.trustauthority.intel.com/main/articles/articles/ita/concept-gpu-attestation.html)（マルチベンダ検証の前例）

### Rope への改善点
- **D1. 実検証フロー（既出 #3）**: `nvtrust/local_gpu_verifier` を範に、SPDM レポート→
  署名検証→RIM golden 突合→OCSP 失効確認→nonce 鮮度、を `net/` に実装。**全パス時のみ**
  `confidential` セッションを `Established` に。
- **D2. 消費者GPUギャップ（既出 #2、最重要）**: CC は Hopper+ 限定。"アイドルGPU" 主供給の
  RTX 等は非対応。`Privacy::Confidential` は **attested TEE ピア限定ルーティング**にし、
  非TEEピアへ機微プロンプトを送らない。
- **D3. 性能オーバーヘッドの明示**: H100 CC は CPU-GPU 転送が PCIe 暗号で律速。小ジョブ/
  小バッチで相対オーバーヘッド大。intent の latency 見積りに CC ペナルティを反映。
- **D4. nvtrust は Python**: Rope は Rust。FFI かサブプロセス委譲か、Rust で SPDM/RIM 検証を
  再実装かを設計判断（最小依存主義との整合）。

---

## 5. ecash / Chaumian 少額決済

**スコープ**: チャネル不要・匿名・オフライン検証可能な bearer トークンで秒単位課金。`ecash.rs`。

### 関連10件
1. **Cashu CDK** — [github.com/cashubtc/cdk](https://github.com/cashubtc/cdk)（Rust の wallet/mint 実装、**そのまま参照可**）
2. **cashu crate** — [crates.io/crates/cashu](https://crates.io/crates/cashu)（CDK のコア型）
3. **awesome-cashu** — [github.com/cashubtc/awesome-cashu](https://github.com/cashubtc/awesome-cashu)（実装カタログ）
4. **NUT-11 P2PK** — [cashubtc.github.io/nuts/11](https://cashubtc.github.io/nuts/11/)（公開鍵ロック＋Schnorr 解錠＝escrow 裏付け）
5. **NUT-12 DLEQ** — [cashubtc.github.io/nuts/12](https://cashubtc.github.io/nuts/12/)（mint 秘密鍵なしのオフライン検証）
6. **cashu-zk-engine (BDHKE)** — [github.com/AbdelStark/cashu-zk-engine](https://github.com/AbdelStark/cashu-zk-engine)（Blind DH 鍵交換実装例）
7. **Chaumian Mint proof-of-reserves** — [arXiv:2306.12783](https://arxiv.org/pdf/2306.12783)（準備金証明＋二重使用防止）
8. **Cashu Nutshell** — [nobsbitcoin v0.14](https://www.nobsbitcoin.com/cashu-nutshell-v0-14-0/)（P2PK/DLEQ 参照実装の挙動）
9. **NUT-07 checkstate** — proof 二重使用検証（`net/cashu_mint.rs` で既実装の HTTP）
10. **Production blind-signature ecash in Rust** — [DEV](https://dev.to/chronocoders/building-a-production-ready-blind-signature-ecash-system-in-rust-4kdf)

### Rope への改善点
- **E1. BDHKE 実装（既出 #4）**: secp256k1 ブラインド署名で Proof を実体化。**CDK を依存に
  取り込む**のが最短（自前実装より監査済みで安全）。最小依存主義とトレードオフを記録。
- **E2. DLEQ(NUT-12) オフライン検証**: ストリーミング秒課金で mint 往復を省く鍵。受領時に
  偽造拒否でき streaming の現実性が出る。
- **E3. P2PK(NUT-11) で escrow ロック**: 3-way escrow をジョブ公開鍵に暗号的に束縛。
- **E4. nullifier を `HashSet` 化（既出、即時）**: `Vec<String>` O(n)→O(1)。
- **E5. 準備金/二重使用**: mint federation（型は既存）＋ NUT-07 checkstate を結線、
  proof-of-reserves 表示で mint 信頼最小化。

---

## 6. P2P発見 / NAT越え / オーバーレイ

**スコープ**: 別NAT背後の「他人のGPU」を発見し直接接続する。`pair.rs` の Mdns/Bluetooth/Dht。

### 関連10件
1. **rust-libp2p** — [github.com/libp2p/rust-libp2p](https://github.com/libp2p/rust-libp2p)（DHT+NAT越えの Rust デファクト）
2. **DCUtR (hole punching)** — [libp2p tutorial](https://docs.rs/libp2p/latest/libp2p/tutorials/hole_punching/index.html)（Connect/Sync で同時 dial）
3. **hole-punching spec** — [libp2p/specs](https://github.com/libp2p/specs/blob/master/connections/hole-punching.md)
4. **AutoNAT** — 自ノードが公開到達可能か判定し hole punch 要否を決める
5. **Circuit Relay v2** — TURN 相当の中継（endpoint-dependent NAT 向け）
6. **mdns-sd** — [github.com/keepsimple1/mdns-sd](https://github.com/keepsimple1/mdns-sd)（safe Rust、Avahi/dns-sd 互換、`_rope._tcp.local` 実装に最適）
7. **libmdns** — [github.com/librespot-org/libmdns](https://github.com/librespot-org/libmdns)（responder 専用）
8. **btleplug** — [github.com/deviceplug/btleplug](https://github.com/deviceplug/btleplug)（近接 BLE 発見、Win/mac/Linux）
9. **hivemind DHT** — [github.com/learning-at-home/hivemind](https://github.com/learning-at-home/hivemind)（Petals の広域発見実証）
10. **libp2p hole punching 解説** — [IPFS Blog](https://blog.ipfs.tech/2022-01-20-libp2p-hole-punching/)

### Rope への改善点
- **F1. NAT越えの実装（既出 #5、致命）**: 現状モデルのみ。**libp2p DCUtR + AutoNAT +
  Circuit Relay v2** を採用しないと「他人GPUを60秒で」はLANデモ止まり。
- **F2. mDNS 実結線**: `mdns-sd` で `_rope._tcp.local` を実装、LAN即時発見を確定（設計コメント済）。
- **F3. BLE 近接発見**: `btleplug` で `DiscoveryMethod::Bluetooth` を実体化（host/central のみ点に注意）。
- **F4. DHT 広域**: libp2p Kademlia（または hivemind 流）で LAN外ピア発見。
- **F5. フォールバック順序**: endpoint-independent NAT は DCUtR、依存NATは Relay。
  両者併用で全到達ケースを被覆（spec の指針）。`pair` の発見モードに段階化。

---

## 7. セキュア握手 / 暗号ペアリング

**スコープ**: MITM 無しに初対面ピアと暗号セッションを張る。`pair.rs` Noise XX/IK + QR フォールバック（blake3 MAC 実装済）。

### 関連10件
1. **snow** — [github.com/mcginty/snow](https://github.com/mcginty/snow)（Rust Noise 実装、spec rev34、暗号差替可）
2. **snowstorm** — [github.com/black-binary/snowstorm](https://github.com/black-binary/snowstorm)（noise+snow の async ストリーム最小実装）
3. **libp2p Noise 標準化** — [libp2p/specs#195](https://github.com/libp2p/specs/issues/195)
4. **Magic Wormhole** — [readthedocs](https://magic-wormhole.readthedocs.io/en/latest/welcome.html)（短コードで安全ペアリングの好例）
5. **SPAKE2 (PAKE)** — Abdalla-Pointcheval / RFC9382（低エントロピー短コード→強鍵、MITM は初回の推測のみ）
6. **SAS (短認証文字列) 設計** — [whitenoise.systems eprint 2025/1598](https://whitenoise.systems/blog/eprint-2025-1598/)（meet と bind を分離、残留誤結合 ≈ 2^-t）
7. **Noise IK vs XX** — IK は 1-RTT・initiator privacy・**responder static 既知が前提**（[snow params](https://github.com/mcginty/snow/blob/main/src/params/mod.rs)）
8. **Noise_NNpsk0**（Wormhole Dilation の peer 暗号）
9. **magic-wormhole PAKE 選定議論** — [issue#348](https://github.com/warner/magic-wormhole/issues/348)
10. **magic-wormhole-protocols security policy** — [GitHub](https://github.com/magic-wormhole/magic-wormhole-protocols/security/policy)

### Rope への改善点
- **G1. Noise を `snow` で実結線（既出 #5）**: 初対面は **XX**、`trust_store` に static 既知なら
  **IK**（1-RTT・initiator privacy）にパターン切替。
- **G2. QR ペアリングの MITM 対策**: 現状の blake3 MAC は完全性のみ。初回 DH の能動的 MITM は
  **SPAKE2（短コード）or SAS 比較**でセッション鍵を束縛して初めて防げる。AirDrop 式 QR フローを
  ここで堅牢化（Magic Wormhole が実証済みの設計）。
- **G3. チャネルバインディング**: ecash の P2PK 公開鍵 / attestation を Noise transcript に束縛し、
  relay 介在・misbinding を排除（カテゴリ 4・5 と連結）。

---

## 8. Sybil耐性 / 評判 / インセンティブ

**スコープ**: 不特定多数の供給者から安全に選ぶ。偽アイデンティティ抑止＋品質評価。`pair.trust_store`。

### 関連10件
1. **Bittensor 批判的分析** — [arXiv:2507.02951](https://arxiv.org/abs/2507.02951)（TAO 登録コストで Sybil 抑止、Yuma consensus）
2. **FORTYTWO** — [arXiv:2510.24801](https://arxiv.org/pdf/2510.24801)（**経済ステーク不要**の compute-anchored 耐性＋peer-ranked consensus）
3. **Gensyn** — 計算検証で信頼最小化 ([DeSpread](https://research.despread.io/ai-infra-projects/))
4. **Trustless FL at Edge** — [arXiv:2511.21118](https://arxiv.org/html/2511.21118v1)（検証＋インセンティブ整合の合成アーキ）
5. **Self-Supervised Inference in Trustless Env** — [arXiv:2409.08386](https://arxiv.org/pdf/2409.08386)
6. **Proof of Quality** — [arXiv:2601.21189](https://arxiv.org/html/2601.21189)（コスト考慮の品質判定）
7. **DSperse** — モデルスライス検証で市場スケール
8. **PolyLink** — [arXiv:2510.02395](https://arxiv.org/pdf/2510.02395)（ブロックチェーン検証）
9. **Decentralized AI Inference Markets** — [BlockEden](https://blockeden.xyz/blog/2025/07/28/decentralized-ai-inference-markets/)（Bittensor/Gensyn/Cuckoo 比較）
10. **Bittensor RL agentic** — [Medium](https://medium.com/@gwrx2005/decentralized-reinforcement-learning-for-agentic-llms-on-the-bittensor-blockchain-66f0ba43e6a1)

### Rope への改善点
- **H1. proof-of-capability チャレンジ（既出 #10）**: 初回ピアに既知回答チャレンジ（FORTYTWO 式）。
  **トークン不要**で能力確認 → Rope の "独自トークン不要" wedge と完全整合。
- **H2. `trust_store` に評判スコア**: 成功率/検証パス率/切断率を蓄積し resolver の入力に。
  低評価は自動降格（PoS の slashing 相当だがトークン無し版）。
- **H3. ステークの代替**: Bittensor は TAO ステーク必須。Rope は **ecash escrow を skin-in-the-game**
  として流用（貸し手の手抜きは escrow 没収＝経済的 disincentive）。
- **H4. 冗長ピアの consensus**: 同ジョブを複数ピアに投げ結果照合（統計的外れ値検出）。
  カテゴリ 3（検証）と基盤共有。

---

## 9. 推論サービング効率

**スコープ**: 貸し手 GPU のスループット最大化＝「1秒課金」の採算と低レイテンシ。`earn`/実行層。

### 関連10件
1. **vLLM** — [github.com/vllm-project/vllm](https://github.com/vllm-project/vllm)（PagedAttention＋連続バッチ＋投機デコード＋prefix caching、2000+ contributors）
2. **PagedAttention** — KV を OS ページング式に小ブロック化、prefix 共有で VRAM 利用率ほぼ100%
3. **Orca 連続バッチ** — iteration-level scheduling（静的バッチの無駄排除）
4. **LMCache** — [arXiv:2510.09665](https://arxiv.org/html/2510.09665v2)（KV を GPU 外退避、query 間 prefix 再利用）
5. **Survey: Taming the Titans** — [arXiv:2504.19720](https://arxiv.org/pdf/2504.19720)
6. **Nightjar (adaptive 投機デコード)** — [arXiv:2512.22420](https://arxiv.org/pdf/2512.22420)
7. **QuantSpec** — [arXiv:2502.10424](https://arxiv.org/pdf/2502.10424)（self-spec＋量子化KV）
8. **投機デコード サーベイ** — [arXiv:2401.07851](https://arxiv.org/pdf/2401.07851)
9. **KV cache 戦略比較** — [arXiv:2604.05012](https://arxiv.org/html/2604.05012v1)
10. **Awesome-LLM-Inference** — [github.com/xlite-dev/Awesome-LLM-Inference](https://github.com/xlite-dev/Awesome-LLM-Inference)

### Rope への改善点
- **I1. 実行は外部エンジンに委譲**: vLLM / llama.cpp を `earn` の実行バックエンドにし、Rope は
  intent→engine 翻訳に徹する（7モジュール最小主義と整合、車輪の再発明回避）。
- **I2. 連続バッチ＋PagedAttention で複数借り手を1GPUに**: 貸し手の収益密度が上がり「1秒課金」が成立。
- **I3. 投機デコードで短ジョブの latency 短縮**: 60秒 haiku デモにも直接効く。
- **I4. prefix KV 再利用（LMCache 式）**: 共通システムプロンプト等を query 間で共有。

---

## 10. 省エネ・カーボン・エッジ スケジューリング

**スコープ**: `intent` の `EnergyPreference`/`RegionConstraint`（現状 dead field）を実機能に。

### 関連10件
1. **FREESH** — [arXiv:2511.00807](https://arxiv.org/pdf/2511.00807)（**異種GPU**のエネルギー特性×地域カーボンの時空間協調）
2. **SLIT** — [arXiv:2505.23554](https://arxiv.org/abs/2505.23554)（TTFT/カーボン/水/電力の協調最適化）
3. **EcoServe** — arXiv:2502.05043（operational＋**embodied** 排出）
4. **エネルギー/カーボン定量化** — [arXiv:2507.11417](https://arxiv.org/abs/2507.11417)
5. **carbon-aware shifting の限界** — [arXiv:2306.06502](https://arxiv.org/html/2306.06502v2)（過度な期待への警告）
6. **AI 推論=可搬電力需要** — [arXiv:2604.27855](https://arxiv.org/html/2604.27855)（latency 制約下のエネルギー地理）
7. **CSGO エッジ cold start** — [arXiv:2508.11287](https://arxiv.org/pdf/2508.11287)
8. **分散LLM over エッジ（エネルギーハーベスティング）** — [arXiv:2408.15907](https://arxiv.org/abs/2408.15907)
9. **HydraServe（cold start）** — [arXiv:2502.15524](https://arxiv.org/pdf/2502.15524)
10. **WISP（エッジ SLO バッチ）** — [arXiv:2601.11652](https://arxiv.org/pdf/2601.11652)

### Rope への改善点
- **J1. dead field の配線 or 削除（既出 #13）**: `EnergyPreference`/`RegionConstraint` を resolver の
  実入力にし低カーボン/再エネピアを選好（FREESH/SLIT）。やらないなら削除（API 誇大表示の是正）。
- **J2. 異種エネルギー効率**: ピア発見時に消費電力/効率メタを取得し、`EnergyPreference` 指定時に反映。
- **J3. 過度な約束を避ける**: arXiv:2306.06502 は carbon-aware shifting の効果限界を指摘。控えめに。
- **J4. embodied carbon の positioning**: 「他人の**アイドル**GPU 再利用」は新規製造を伴わず
  embodied carbon が本質的に低い（EcoServe の枠組み）。**文献に裏打ちされた差別化メッセージ**。

---

## 全体総括 — 10カテゴリ横断の所見

100 件（10カテゴリ×10）の arXiv/GitHub を Rope の実コードに突き合わせた結論:

1. **最大の構造ギャップは「検証 (カテゴリ3) × Sybil耐性 (8) × TEE (4)」の三位一体が未実装**。
   この3つは基盤を共有でき（proof-of-capability チャレンジ＝検証＝初回信頼）、
   **1つの仕組みで3カテゴリを同時に埋められる**のが最大の設計レバレッジ。
2. **"アイドルGPU × TEE プライバシー" の矛盾 (4)** が最優先の是正対象（消費者GPUはCC非対応 →
   機微ジョブは attested TEE ピア限定ルーティング、README 是正）。
3. **実 I/O の3本柱（Noise握手7・NAT越え6・Cashu暗号5）は既存OSSで最短化可能**:
   `snow` / `libp2p(DCUtR)` / Cashu `CDK` を採れば自前実装の監査リスクを回避（最小依存主義との
   トレードオフは要記録）。
4. **効率 (9) と省エネ (10) は外部エンジン委譲＋既存フィールド配線**で、Rope 本体を肥大させずに対応可。
5. **即時の low-hanging fruit**: nullifier `HashSet` 化 / 非TEEルーティング拒否 / dead field 整理 /
   モデル重み CID 化（完全性検証も同時に得る）/ README プライバシー注記。

> 優先度付きの統合バックログは [`RESEARCH_IMPROVEMENTS.md`](RESEARCH_IMPROVEMENTS.md) を参照。
</content>

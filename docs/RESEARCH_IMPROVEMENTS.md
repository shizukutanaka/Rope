# 改善点の洗い出し — 同種ソフト + arXiv 調査

> 調査日: 2026-06-05 / 対象: Rope v0.2.1 (7 core + 1 net モジュール)
> 目的: 同種ソフト (EXO / Petals / Akash / io.net / Vast.ai / RunPod / Gensyn /
> Prime Intellect / Parallax 等) と arXiv 論文を参照し、Rope の改善点を列挙する。

この文書は **方向性の提案** であり、ROADMAP ではない。各項目は
「現状コードの事実」→「同種ソフト/論文の参照」→「具体的改善案」→「優先度」の順で記述する。

---

## 0. 調査で判明した最重要ギャップ（要約）

Rope の現状は **設計の極めて洗練された "状態機械スケルトン"** である。型と状態遷移は
丁寧に定義されているが、`pair` / `ecash` / `confidential` の **実 I/O・実暗号は未結線**
（コード内コメントが明言: ecash「実際の暗号は後で接続」、Cargo.toml で
`x25519-dalek` / `aes-gcm` は「Noise 実装時に復活予定」とコメントアウト）。
唯一の実 I/O は `http` feature 配下の `net/cashu_mint.rs`。

調査の結果、**3 つの構造的ギャップ**が浮かんだ。これらは個別バグではなく、
製品の core value proposition（README の比較表）を支える前提が未実装である点で重大:

| # | ギャップ | README の主張 | 実態 | 参照 |
|---|---------|--------------|------|------|
| G1 | **検証（proof-of-execution）が未実装** | `verification` フィールド有 | `VerificationLevel` enum のみ、実機構なし | VeriLLM, TOPLOC, SPEX, Gensyn |
| G2 | **TEE プライバシーが消費者GPUで成立しない** | 「TEE プライバシー ✅」 | CC は H100/H200/Blackwell 限定。"アイドルGPU" の主供給源 (RTX 等) は非対応 | NVIDIA HCC WP, arXiv:2507.02770 |
| G3 | **アテステーション検証が struct のみ** | 「アテステーション（リモート検証）」 | NRAS/RIM/OCSP/SPDM 検証フロー未実装 | NVIDIA Attestation docs, arXiv:2507.02770 |

以下、モジュール別に詳述する。

---

## 1. 検証 / Proof-of-Execution（最優先）

### 現状
- `intent.rs` に `VerificationLevel` があるが、貸し手が「本当にそのモデルを正しく
  実行したか」を借り手が検証する機構が無い。
- `ecash.rs` の escrow は「期限切れ自動返金 (deadman)」はあるが、
  **"間違った/手抜き計算でも金が払われる"** フリーライド問題に未対応。

### 同種ソフト / arXiv
- **Gensyn** — 計算検証機構を中核に据え、特定の当事者を信頼せず ML 計算を検証
  ([a16z Series A $43M](https://research.despread.io/ai-infra-projects/))。
- **Prime Intellect / TOPLOC** ([arXiv:2501.16007](https://arxiv.org/abs/2501.16007)) —
  Locality-Sensitive Hashing で活性値をコミットし、モデル/プロンプト/精度の
  改ざんを検出。full recompute より大幅に軽量な検証。
- **VeriLLM** ([arXiv:2509.24257](https://arxiv.org/abs/2509.24257)) —
  軽量な「再実行 (empirical rerunning)」＋最小限のオンチェック で、
  **元の推論コストの約 1%** で検証可能。free-riding を排除。
- **zkLLM** — 暗号学的に完全だが LLaMA-2-13B で commitment 986 秒 + proof 803 秒 と
  実用上重い（= Rope の "60秒 wow" には不適）。
- **Proof of Quality** ([arXiv:2601.21189](https://arxiv.org/html/2601.21189)) —
  コスト考慮型の品質証明。

### 改善案
1. `intent.rs::VerificationLevel` を実機構に結線する。推奨は **TOPLOC 風 LSH コミット**
   + **VeriLLM 風 確率的再実行**（毎回でなく抜き打ち再実行で ~1% コスト）。zkLLM は不採用。
2. escrow 解放条件に「検証パス」を追加（`ecash.rs` の Escrow 状態遷移に
   `VerificationPassed` ゲートを挟む）。手抜き計算は返金へ。
3. 検証の前提として **決定論的推論**（greedy/temperature=0、確定的カーネル）を
   `confidential`/実行層で要求できるようにする（LSH 系検証は非決定性で崩れる）。

**優先度: 最高**（README 比較表の「ジョブ中断耐性 ✅escrow」の信頼基盤）。

---

## 2. GPU TEE プライバシー — 消費者GPUギャップ（最優先）

### 現状
- README: 「TEE プライバシー ✅」「GPU は安全（プロンプトは相手に見えません）」。
- しかし `confidential.rs` は H100/H200/Blackwell の CC が前提。

### 同種ソフト / arXiv
- **NVIDIA HCC** ([Whitepaper](https://images.nvidia.com/aem-dam/en-zz/Solutions/data-center/HCC-Whitepaper-v1.0.pdf),
  [Technical Blog](https://developer.nvidia.com/blog/confidential-computing-on-h100-gpus-for-secure-and-trustworthy-ai/)) —
  GPU CC は **Hopper 以降のデータセンター GPU 限定**、かつ CPU 側 CVM
  (AMD SEV-SNP / Intel TDX) が必須。
- **arXiv:2507.02770** "NVIDIA GPU Confidential Computing Demystified" — CC アーキの
  初の詳細セキュリティ解析。
- **Petals** ([arXiv:2312.08361](https://arxiv.org/abs/2312.08361)) は明確に
  「public swarm では他人がデータに触れうる。機微データは信頼できる private swarm
  か、将来 SMPC/準同型暗号で」と限界を明記している。

### 問題の核心
Rope の供給ナラティブは「他人の**アイドル GPU**」（= 大半が RTX 等の消費者 GPU、
CC 非対応）。一方プライバシー保証は CC 前提。**この 2 つは両立しない。**
非 TEE ピアに平文プロンプトを送れば「プロンプトは相手に見えません」は偽になる。

### 改善案
1. **プライバシーと供給の型レベル分離**: `Privacy` 制約が `Confidential` のとき、
   **アテステーション検証済み TEE ピアのみ**にルーティング（`intent` resolver で
   非 TEE ピアを除外）。CC 非対応ピアには機微ジョブを**絶対に送らない**。
2. 非 TEE ピア向けには README の主張をトーンダウン or 明示警告（「このピアは
   TEE 非対応。プロンプトは相手に見える可能性」）。誇大表示の是正。
3. 中期: Petals が挙げる **SMPC / HE** を privacy fallback として `intent` の
   選択肢に（コスト高なので opt-in）。

**優先度: 最高**（虚偽表示リスク。製品の中核主張の整合性）。

---

## 3. アテステーション検証フローの実装（高）

### 現状
`confidential.rs` の `AttestationReport` は型のみ。前ラウンドで blake3 + nonce による
リプレイ識別性は入れたが、**NVIDIA の実検証チェーンは未実装**。

### 同種ソフト / arXiv
- **NVIDIA Attestation** ([docs](https://docs.nvidia.com/attestation/index.html)) —
  検証は **NRAS**(Remote Attestation Service) + **RIM**(Reference Integrity Manifest)
  + **OCSP**(証明書失効) の 3 点セット。トランスポートは **SPDM**。
- **Intel Trust Authority** も GPU attestation を提供（マルチベンダ検証の前例）。
- **arXiv:2507.02770** が攻撃面・弱点を解析。

### 改善案
クライアント側に最低限の検証フローを実装（`http` feature 配下、`net/` に
`nras.rs` 新設を想定）:
1. **nonce 鮮度**: 借り手生成 nonce をレポートに含め、リプレイ排除（基盤は実装済）。
2. **NRAS 検証**: GPU が出した署名付きレポートを NRAS に投げ verdict を得る、
   または証明書チェーンをローカル検証。
3. **RIM 突合**: 計測値を Reference Integrity Manifest と比較（既知良ファームウェア）。
4. **OCSP**: デバイス証明書の失効確認。
5. これら**全パス時のみ** `confidential` セッションを `Established` に遷移。

**優先度: 高**（#2 のルーティング判断が正しく機能する前提）。

---

## 4. ecash / Cashu の暗号的正しさ（高）

### 現状
- `ecash.rs` は純状態機械。**BDHKE（ブラインド署名）未実装**、nullifier はただの文字列。
- `spent_nullifiers: Vec<String>` — 二重使用チェックが **O(n) 線形探索**。
- DLEQ / P2PK 未対応 → オフライン検証も escrow ロックも暗号的裏付けなし。

### 同種ソフト / arXiv
- **Cashu NUTs**: [NUT-11 P2PK](https://cashubtc.github.io/nuts/11/)（ecash を受取人
  公開鍵にロック、Schnorr 署名で解錠）、[NUT-12 DLEQ](https://cashubtc.github.io/nuts/12/)
  （mint 秘密鍵なしで署名検証 = **オフライン/受領時検証**）。
- **arXiv:2306.12783** "Proof of reserves and non-double spends for Chaumian Mints" —
  mint の準備金証明と二重使用防止。mint 信頼最小化の理論基盤。

### 改善案
1. **nullifier を `HashSet`/`BTreeSet` に**（O(n)→O(1)）。実装容易・即効。
2. **BDHKE 実装**: secp256k1 ブラインド署名で Proof を実体化（`net/cashu_mint.rs`
   は既に NUT-01/04/05/07 の HTTP を持つので、暗号側を core に追加）。
3. **NUT-12 DLEQ**: 受領時オフライン検証 → mint への問い合わせなしで偽造拒否。
   ストリーミング決済（秒単位）で mint 往復を省け、`ecash` streaming の現実性が増す。
4. **NUT-11 P2PK で escrow をジョブ公開鍵にロック** → 3-way escrow を暗号的に裏付け。
5. mint 信頼: 複数 mint federation（型は既にある）+ proof-of-reserves 表示。

**優先度: 高**（「独自トークン不要 ✅」「1秒課金 ✅」を支える暗号基盤）。

---

## 5. P2P ネットワーキングの実結線（高）

### 現状
`pair.rs` は mDNS / Bluetooth / Kademlia DHT / Noise XX を**モデル化のみ**。
実 I/O なし。`x25519-dalek`/`aes-gcm` は Cargo.toml でコメントアウト。
**致命的に欠けるのは NAT 越え** — 「他人の GPU」は別 LAN/別 NAT 背後にいるのが普通。

### 同種ソフト / arXiv
- **Petals** ([arXiv:2312.08361](https://arxiv.org/abs/2312.08361)) — `hivemind` 基盤の
  DHT + 独自の耐障害プロトコル。BitTorrent 式の実証済み P2P。
- **Parallax** ([arXiv:2509.26182](https://arxiv.org/abs/2509.26182)) — 標準インターネット
  接続越しに異種マシンへモデルをシャード。P2P 基盤前提。
- **libp2p** — DHT(Kademlia) + **DCUtR ホールパンチング** + Circuit Relay で
  NAT 越えの実装デファクト。Rust 実装あり。
- **Noise Protocol** — Rust では `snow` crate が XX/IK パターンを提供。

### 改善案
1. **Noise XX/IK を `snow` で実結線**（コメントアウト依存を復活）。握手の型は既存。
2. **libp2p 採用検討**: DHT 広域発見 + **ホールパンチング(DCUtR)** + relay。
   「アイドル GPU を 60 秒で借りる」は NAT 越えが成立しない限り LAN デモ止まり。
3. mDNS は `_rope._tcp.local`（コメント済の設計）を実装し LAN 即時発見を確定。

**優先度: 高**（README「他人 GPU ✅」「ゼロコンフィグ ✅」の実体化）。

---

## 6. スケジューリング / 異種性 / 大規模モデル対応（中）

### 現状
`intent` resolver は最小コストプラン選択だが、**単一ピア前提**。1 台の消費者 GPU に
載らない大規模モデル（70B 等）を複数ピアに分割する pipeline parallelism が無い。

### 同種ソフト / arXiv
- **Parallax** ([arXiv:2509.26182](https://arxiv.org/abs/2509.26182)) — 2 段スケジューラ
  (model allocation + request-time pipeline selection)。異種 GPU・帯域制約下で
  レイテンシ/スループット最適化。
- **Petals** — swarm/pipeline parallelism + 耐障害推論 + 負荷分散。
  ただし限界として「貪欲ヒューリスティックで大域最適化なし → 異種性/帯域ボトルネックを
  捌けず資源過少利用」が指摘されている（Rope が後発で改善余地）。
- **survey** ([arXiv:2503.16585](https://arxiv.org/html/2503.16585v1)) 分散 LLM の課題整理。

### 改善案
1. 中期: 複数ピアへの **pipeline parallel 分割**（大モデルを 1 台に載せない）。
   Parallax の 2 段スケジューリングを参考に。
2. **耐障害再ルーティング**: ピア切断時に別ピアへ継続（deadman 返金は実装済だが、
   "返金" でなく "継続" の方が UX 良い）。Petals の fault-tolerant inference を参照。
3. 異種性考慮の配置（VRAM/帯域をピア発見時にプローブし resolver の入力に）。

**優先度: 中**（4 動詞のシンプルさとトレードオフ。小モデルは単一ピアで足りる）。

---

## 7. 状態の永続化と耐クラッシュ性（中）

### 現状
ウォレット残高・escrow・streaming・spent_nullifiers はメモリ上の状態機械。
**クラッシュ時に bearer token / escrow を失う**リスク（= 金銭損失）。

### 改善案
1. ecash 状態の **WAL/atomic 書き込み**（`config` のパス解決は既にある）。
   bearer token は再生成不能なので最低限の durability は必須。
2. 既に入れた stale-lock 検出（クロスプラットフォーム）と整合する形で、
   セッション/ウォレットの crash recovery を設計。

**優先度: 中**（実 ecash を結線した瞬間に「高」へ昇格）。

---

## 8. 経済設計（低〜中・設計判断）

### 同種ソフト
- **Akash** — リバースオークション（入札で価格圧縮、約 60-70% コスト減）。
- Rope は固定 `amount_sats` + bearer escrow を選択（README 設計思想と整合）。

### 改善案（任意）
価格発見が必要になれば軽量な逆オークションを `earn` 側に。ただし
「質問しない/推測する」哲学と相反するため、**現状の固定価格は妥当**。記録のみ。

**優先度: 低**（現設計を支持。将来の供給過多時のみ再検討）。

---

## 10. Sybil 耐性 / レピュテーション（高） — 第2巡

### 現状
`pair.rs` の `trust_store` は **TOFU（Trust On First Use）のみ**。一度繋いだ鍵は覚えるが、
**初回のピア選択に信頼の根拠が無い**。攻撃者は安価に多数の偽 GPU アイデンティティを
作れる（Sybil 攻撃）。「他人の GPU」を不特定多数から借りる以上、ここは中核リスク。

### 同種ソフト / arXiv
- **Bittensor** ([arXiv:2507.02951](https://arxiv.org/abs/2507.02951)) — 新規 miner 登録に
  TAO 支払いを課し Sybil スパムを抑止。Yuma consensus で品質スコアに応じ報酬、
  低品質はレピュテーション "slashing" で経済的に淘汰。
- **FORTYTWO** ([arXiv:2510.24801](https://arxiv.org/pdf/2510.24801)) — **経済ステークに
  依存しない compute-anchored Sybil 耐性**: 新規ノードに test call（能力証明）を課す
  + peer-ranked consensus。Rope の "トークン不要" 思想と相性が良い。
- **DSperse** — モデルスライス単位の検証可能推論で市場をスケール。

### 改善案
1. **#1 の検証機構を Sybil 耐性に転用**: 初回ピアに小さな既知回答のチャレンジ
   （proof-of-capability、FORTYTWO 式）を投げ、TEE/トークン無しでも能力を確認。
2. `trust_store` に **レピュテーションスコア**（成功/検証パス/切断率）を持たせ、
   resolver のピア選択入力に。低評価は自動降格（slashing 相当）。
3. escrow と連動: 検証失敗・途中切断はスコア減点 → 再選択で回避。

**優先度: 高**（不特定多数から借りる製品の安全性の前提。#1 と基盤を共有）。

---

## 11. モデル配布 / コールドスタート（高） — 第2巡

### 現状
「60 秒 wow」はピアが**目的モデルを既に保持している**前提に見える。保持していなければ
モデル重みのフェッチ（数 GB〜数十 GB）が支配的になり、60 秒が崩れる。`intent`/`first_run`
にモデル配送・コールドスタート対策が無い。

### 同種ソフト / arXiv
- **HydraServe** ([arXiv:2502.15524](https://arxiv.org/pdf/2502.15524)) — サーバレス LLM の
  コールドスタート最小化。リモートストレージからの重みフェッチがボトルネック。
- **ParaServe** — pipeline parallelism で重みを複数サーバに分散しコールドスタート短縮。
  「モデルロードを計算・通信とオーバーラップ」させる latency-aware スケジューリング。
- **IPFS / content-addressed (CID)** — 重みを CID で immutable に参照・P2P 配信。
  改ざん検出（ハッシュ一致）も同時に得られる。

### 改善案
1. **content-addressed 配布**: モデル重みを CID（blake3 はもう依存にある）で参照し、
   近傍ピア/LAN から P2P 取得。ハッシュ一致で**重みの完全性も検証**（#1 と相乗）。
2. **ロードと計算のオーバーラップ**: レイヤを取得しながら前段レイヤを実行
   （ParaServe/pipeline）。`first_run` の 60 秒予算に組み込む。
3. resolver で「モデル保持済みピア」を優先（cold-start ペナルティをコスト関数に加算）。

**優先度: 高**（README の中核 "60 秒" 約束の実体化に直結）。

---

## 12. プロバイダ側 推論効率（中） — 第2巡

### 現状
`earn`（貸し出し側）に推論効率の最適化が無い。スループットが低いと貸し手の採算が
合わず、「1 秒課金」の単価が成立しにくい（供給が痩せる）。

### 同種ソフト / arXiv
- **Survey "Taming the Titans"** ([arXiv:2504.19720](https://arxiv.org/pdf/2504.19720)) —
  連続バッチ / KV キャッシュ / 投機デコードの効率化サーベイ。
- **LMCache** ([arXiv:2510.09665](https://arxiv.org/html/2510.09665v2)) — KV キャッシュを
  GPU 外に退避し **prefix を query 間で再利用**。
- **連続バッチ (Orca)** — iteration-level scheduling で静的バッチの無駄を排除。
- **投機デコード** (Nightjar [arXiv:2512.22420](https://arxiv.org/pdf/2512.22420),
  QuantSpec [arXiv:2502.10424]) — 並列トークン生成で latency 削減。

### 改善案
1. `earn` 実装時に **連続バッチ + prefix KV 再利用** を前提化（複数借り手を 1 GPU で捌く）。
2. 単一借り手・短ジョブが多い Rope の特性では **投機デコード**が latency に効く
   （60 秒 haiku デモにも直接効く）。
3. これらは外部推論エンジン（llama.cpp/vLLM 等）に委譲し、Rope は intent→実行の
   翻訳に徹する（モジュール最小主義と整合）。

**優先度: 中**（供給側経済性。実推論エンジン結線時にまとめて）。

---

## 13. エネルギー / カーボン対応（中〜低・既存フィールドの実装） — 第2巡

### 現状
`intent.rs` に `EnergyPreference`（既定: unconstrained）と `RegionConstraint` が **存在するが
未使用の dead field**。宣言だけで resolver が利用していない。

### 同種ソフト / arXiv
- **FREESH** ([arXiv:2511.00807](https://arxiv.org/pdf/2511.00807)) — 地域別カーボン排出率
  × LLM ワークロード × GPU エネルギー特性を統合した時空間協調スケジューリング
  （**異種 GPU** 前提 = Rope と同条件）。
- **SLIT** ([arXiv:2505.23554](https://arxiv.org/abs/2505.23554)) — TTFT/カーボン/水/電力コストの
  協調最適化。
- **EcoServe** (arXiv:2502.05043) — operational + embodied 排出を考慮。

### 改善案
1. `EnergyPreference` を resolver の実入力に: 低カーボン地域/再エネピアを選好
   （`RegionConstraint` と統合、ピア発見時にメタデータ取得）。
2. 少なくとも「未使用フィールドを消す or 配線する」の二択を明確化
   （宣言だけの dead field は API の誇大表示）。

**優先度: 中〜低**（差別化要素だが、まず #1〜#5 の土台が先。ただし dead field 整理は即時可）。

---

## 14. 優先度サマリ（全項目）

| 優先 | 項目 | 一言 |
|------|------|------|
| 最高 | #1 検証機構 | VeriLLM(~1%)/TOPLOC を実装、escrow にゲート |
| 最高 | #2 TEE/消費者GPUギャップ | 機微ジョブは TEE ピア限定ルーティング、誇大表示是正 |
| 高 | #3 アテステーション実検証 | NRAS/RIM/OCSP/SPDM フロー |
| 高 | #4 Cashu 暗号 | BDHKE+DLEQ(NUT-12)+P2PK(NUT-11)、nullifier を Set 化 |
| 高 | #5 P2P 実結線 | Noise(snow)+libp2p NAT越え(DCUtR) |
| 高 | #10 Sybil耐性/評判 | proof-of-capability(FORTYTWO)+評判スコア。#1と基盤共有 |
| 高 | #11 モデル配布/コールドスタート | content-addressed(CID)+ロード/計算オーバーラップ |
| 中 | #6 異種性スケジューリング | 複数ピア pipeline（Parallax 2段） |
| 中 | #7 永続化 | ecash 状態の WAL/crash recovery |
| 中 | #12 プロバイダ側推論効率 | 連続バッチ+prefix KV+投機デコード（earn 採算） |
| 中低 | #13 エネルギー/カーボン | EnergyPreference を配線 or 削除（dead field 整理） |
| 低 | #8 経済設計 | 現状固定価格を支持 |

### すぐ着手できる小改善（low-hanging fruit）
- ✅ **実装済** `spent_nullifiers` を `VecDeque`+`HashSet` 化（#4-1/E4、二重使用検出 O(n)→O(1)）
- ✅ **実装済** ConfidentialCompute + 検証なしを resolver で infeasible 化（#2-1×#3、未検証TEEへの平文送信を拒否）
- ✅ **実装済** README のプライバシー主張に TEE/attestation 前提の注記（#2-2、誇大表示是正）
- ✅ **実装済(一部)** `EnergyPreference` を resolver に配線（#13-2、`MinimizeWatts` で最小エネルギーへ寄せ + `EnergyAware` 最適化）。`RegionConstraint` はプロバイダの地域メタデータ未整備のため保留
- モデル重みを CID 参照にしハッシュ一致で完全性検証（#11-1、blake3 は既存依存）

---

## 参照一覧

**同種ソフト**
- Petals — [arXiv:2209.01188](https://ar5iv.labs.arxiv.org/html/2209.01188),
  [arXiv:2312.08361](https://arxiv.org/abs/2312.08361),
  [GitHub](https://github.com/bigscience-workshop/petals)
- Parallax — [arXiv:2509.26182](https://arxiv.org/abs/2509.26182)
- Gensyn / Prime Intellect / Akash / io.net —
  [DeSpread Research](https://research.despread.io/ai-infra-projects/),
  [io.net blog](https://io.net/blog/decentralized-computing)

**検証 (proof-of-execution)**
- VeriLLM — [arXiv:2509.24257](https://arxiv.org/abs/2509.24257)
- TOPLOC — [arXiv:2501.16007](https://arxiv.org/abs/2501.16007)
- Proof of Quality — [arXiv:2601.21189](https://arxiv.org/html/2601.21189)
- PolyLink — [arXiv:2510.02395](https://arxiv.org/pdf/2510.02395)

**GPU TEE / Confidential Computing**
- NVIDIA HCC Whitepaper — [PDF](https://images.nvidia.com/aem-dam/en-zz/Solutions/data-center/HCC-Whitepaper-v1.0.pdf)
- NVIDIA Attestation docs — [docs.nvidia.com](https://docs.nvidia.com/attestation/index.html)
- GPU CC Demystified — [arXiv:2507.02770](https://arxiv.org/html/2507.02770v1)
- H100 CC Performance — [arXiv:2409.03992](https://arxiv.org/pdf/2409.03992v2)
- ACM Queue: First Confidential GPUs — [queue.acm.org](https://queue.acm.org/detail.cfm?id=3623391)

**ecash / Cashu**
- NUT-11 P2PK — [cashubtc.github.io/nuts/11](https://cashubtc.github.io/nuts/11/)
- NUT-12 DLEQ — [cashubtc.github.io/nuts/12](https://cashubtc.github.io/nuts/12/)
- Chaumian Mint proof-of-reserves — [arXiv:2306.12783](https://arxiv.org/pdf/2306.12783)

**Sybil 耐性 / インセンティブ（第2巡）**
- Bittensor 批判的分析 — [arXiv:2507.02951](https://arxiv.org/abs/2507.02951)
- FORTYTWO (compute-anchored Sybil 耐性 + peer-ranked consensus) — [arXiv:2510.24801](https://arxiv.org/pdf/2510.24801)

**モデル配布 / コールドスタート（第2巡）**
- HydraServe — [arXiv:2502.15524](https://arxiv.org/pdf/2502.15524)
- Decentralized LLM over Edge (energy harvesting) — [arXiv:2408.15907](https://arxiv.org/abs/2408.15907)

**推論効率（第2巡）**
- Survey: Taming the Titans — [arXiv:2504.19720](https://arxiv.org/pdf/2504.19720)
- LMCache (prefix KV 再利用) — [arXiv:2510.09665](https://arxiv.org/html/2510.09665v2)
- Nightjar (adaptive 投機デコード) — [arXiv:2512.22420](https://arxiv.org/pdf/2512.22420)

**エネルギー / カーボン（第2巡）**
- FREESH (異種 GPU 省エネスケジューリング) — [arXiv:2511.00807](https://arxiv.org/pdf/2511.00807)
- SLIT (carbon/water/energy 協調) — [arXiv:2505.23554](https://arxiv.org/abs/2505.23554)
</content>
</invoke>

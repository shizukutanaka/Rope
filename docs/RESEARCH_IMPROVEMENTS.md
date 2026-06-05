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

## 9. 優先度サマリ

| 優先 | 項目 | 一言 |
|------|------|------|
| 最高 | #1 検証機構 | VeriLLM(~1%)/TOPLOC を実装、escrow にゲート |
| 最高 | #2 TEE/消費者GPUギャップ | 機微ジョブは TEE ピア限定ルーティング、誇大表示是正 |
| 高 | #3 アテステーション実検証 | NRAS/RIM/OCSP/SPDM フロー |
| 高 | #4 Cashu 暗号 | BDHKE+DLEQ(NUT-12)+P2PK(NUT-11)、nullifier を Set 化 |
| 高 | #5 P2P 実結線 | Noise(snow)+libp2p NAT越え(DCUtR) |
| 中 | #6 異種性スケジューリング | 複数ピア pipeline（Parallax 2段） |
| 中 | #7 永続化 | ecash 状態の WAL/crash recovery |
| 低 | #8 経済設計 | 現状固定価格を支持 |

### すぐ着手できる小改善（low-hanging fruit）
- `spent_nullifiers: Vec<String>` → `HashSet`（#4-1、数行）
- 非 TEE ピアへの機微ジョブ送信を resolver で拒否（#2-1、型で表現）
- README のプライバシー主張に TEE 前提の注記（#2-2、誇大表示是正）

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
</content>
</invoke>

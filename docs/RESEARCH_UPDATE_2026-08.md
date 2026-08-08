# リサーチ更新 (2026-08) — 第一原理監査が示した優先順位に沿った調査

> 調査日: 2026-08-08 (WebSearch/WebFetch 実施)。
> 前回調査: `RESEARCH_UPDATE_2026-07.md` (2026-07-10)。約1ヶ月の差分。
>
> **今回の調査設計が前回までと異なる点**: 前回までは既存バックログの
> カテゴリ順 (検証 → TEE → Cashu → P2P → …) に調べていた。今回は
> [`FIRST_PRINCIPLES_AUDIT.md`](FIRST_PRINCIPLES_AUDIT.md) が演繹した
> **公理の依存順 (A1 発見 → A3 実行 → A4 検証)** に沿って調べる。
> 目的は「新着を網羅する」ことではなく、**演繹で最優先と判定した箇所の
> 実装可否を一次情報で確定させる**こと。
>
> 引用は全て WebSearch で実在確認済み。arxiv.org は本環境の egress proxy が
> ブロックするため、論文本文は未読 — **要旨レベルの引用に留め、実装判断の
> 根拠にする際は本文の確認を必須とする** (下記 §1 の扱いに明記)。

---

## 1. 🔴 最重要の新着: Hollow-LLM 攻撃 — Rope の検証設計への直接的反証

**arXiv:2607.28884** "Hollow-LLM Attack: Computationally Trivial Weights in
Zero-Knowledge Verification of LLM Inference" (**IEEE S&P'26 採択**)。
前回調査 (7/10) 以降の新着。

### 攻撃の要点

不正なプロバイダが、**宣言したアーキテクチャとパラメータ数を維持したまま**、
代数構造が実効計算を潰す「ゴースト重み」を埋め込む。ZK 証明は
**「結果が方程式を満たすこと」は検証するが「どれだけの計算が必要だったか」は
検証しない** — この *effort gap* を突き、検証を通過しながら小さいモデル相当の
計算コストしか払わない。

### Rope への含意 (これが本質)

Rope の `proof_satisfies` (`ecash.rs:778`) は**コミットメントのハッシュ一致**を
見るだけであり、Hollow-LLM が破る ZK 証明よりも**さらに弱い**。
第一原理監査 §2 の A4 判定 (「計算が実際に行われた証明にはならない」) を、
査読付き攻撃論文が独立に裏付けた形になる。

さらに重要なのは、この攻撃が **`docs/RESEARCH_IMPROVEMENTS.md` #1 が推奨する
TOPLOC/VeriLLM 系の「軽量検証」路線そのものに刺さる可能性**である:
検証コストを下げるほど effort gap は広がりやすい。TensorCommitments
(arXiv:2602.12630、前回調査で採用候補としたもの) が prover +0.97% という
極めて低いオーバーヘッドを謳う点は、**Hollow-LLM の観点では再評価が必要**。

### 判断: 採否は保留、ただし設計方針に反映する

- **本文未読** (egress ブロック) のため、TensorCommitments が Hollow-LLM に
  耐性を持つかは**未確定**。要旨は「tailored LLM attacks への robustness を
  改善」と述べており、Hollow-LLM 系を想定している可能性はあるが、
  IEEE S&P'26 の攻撃論文の方が新しい。
- **確定している設計上の教訓** (本文なしでも言える): **A4 (検証) の設計時に
  「出力の正しさ」だけでなく「投入された計算量」を検証対象に含めること。**
  これは検証手法の選定基準そのものを変える指摘であり、
  `RESEARCH_IMPROVEMENTS.md` #1 の受け入れ基準に追加すべき。
- **ビルド可能環境での次アクション**: arxiv.org にアクセスできる環境で
  2607.28884 と 2602.12630 の両本文を読み、TensorCommitments の
  effort gap 耐性を確認してから A4 の手法を確定する。

関連する同時期の論文 (いずれも要旨のみ確認):
- **arXiv:2606.16352** "Communication-Efficient Verifiable Attention"
  (VeriAttn) — attention の線形/非線形計算を GPU にオフロードし **TEE が検証**
  する構成。Rope の A6 (TEE) と A4 (検証) を**同一機構で扱う**設計であり、
  両者を別々に実装しようとしている現状の設計とは異なるアプローチ。
- **arXiv:2603.18046** NanoZK — layerwise ZK。モデル重みを晒さずに
  「宣言したモデルを実行した」ことを証明。プロバイダの IP 保護と
  借り手の確証を両立する構成。

---

## 2. A3 (推論実行) — 第一原理で最優先とした箇所の実装可否が確定

`FIRST_PRINCIPLES_AUDIT.md` §8 で「A3 の初回実装は **CPU・非決定論的・
非TEE・ローカルモデルパス**で十分」と演繹した。今回の調査でこの前提を
満たす crate が実在することを確認した。

### mistral.rs — 演繹した最小要件と噛み合う

- **pure Rust** (Hugging Face **Candle** フレームワーク上に構築)。
  Rope の `unsafe_code = "deny"` 方針・pure Rust 依存方針
  (`ed25519-dalek`/`blake3` は全て pure Rust) と整合する。
- **CPU で動く** — Candle が CPU / NVIDIA GPU / Apple Silicon を
  ベンダ固有コードなしで扱う (Intel MKL / Apple Accelerate 対応)。
  §8 の「初回は CPU で十分」が実際に成立する。
- **2026 年時点で Candle 0.9.2 の crates.io 公式リリースへ移行済**
  (バックエンド依存の安定化)。GGUF / HF モデル、2-8bit 量子化に対応。
- リポジトリ: `EricLBuehler/mistral.rs`。

### llama-cpp-2 — 対抗馬だが Rope の方針と衝突

- llama.cpp の C API への **FFI バインディング**。`llama-cpp-sys` を同梱し
  llama.cpp 本体に追随する方針で、**安定 API を目指さず semver も
  意味を持たない**と作者自身が明言。
- Rope にとっての問題は 2 点: (a) **C++ ツールチェーン依存**が
  「依存最小主義」と衝突、(b) **API 不安定**が MSRV 1.75 固定・
  edition2024 回避という既存の慎重な依存管理と噛み合わない。

### 結論 (前回 §6 からの更新)

前回 (7/10) は「llama-cpp-rs vs mistral.rs」を並列に挙げるに留めた。
**今回、Rope の設計制約 (pure Rust / unsafe 禁止 / 依存最小 / MSRV 固定) と
突き合わせた結果、mistral.rs を推奨とする。** llama-cpp-2 は性能面で優位でも、
FFI と API 不安定性のコストが Rope の既存方針と正面から衝突する。

**未確定事項** (crates.io の詳細ページは本環境で取得できず): mistral.rs の
MSRV が 1.75 を満たすか、依存が edition2024 を要求しないかは
**ビルド可能環境で `cargo add` 直後に確認が必須**
(`CASHU_BDHKE_IMPLEMENTATION_READINESS.md` §4 が k256 について述べたのと
同じ注意)。

---

## 3. A1 (発見) — Iroh 1.0 正式リリースを確認、前回の推奨変更が裏付けられた

前回調査 (§4) で「libp2p → Iroh へ推奨スタックの見直しを提案」とした。
今回、その前提が確定した:

- **Iroh v1.0.0 が 2026年6月に正式リリース**され、production-ready と
  位置づけられた (前回調査時点では 1.0 のリリース状況が確定していなかった)。
- QUIC ベース、NAT ホールパンチング + **リレー fallback**、
  E2E 暗号化 (QUIC/TLS 1.3) を含む完全なネットワークスタック。
- **NAT 越え成功率**: Tailscale が確立した手法を採用し、
  **libp2p のホールパンチング成功率 ~70% を上回る**とされる。
  Rope の W2 (NAT 越えなし = 実質 LAN 止まり) に直接効く。
- `libp2p-iroh` (iroh QUIC を libp2p transport として使う crate) も存在し、
  **段階的移行の逃げ道がある** — libp2p のプロトコル資産を保ちながら
  transport だけ Iroh にする選択肢。

→ `P2P_IMPLEMENTATION_READINESS.md` の「libp2p vs Iroh」比較表に
**「Iroh 1.0 正式リリース済 (2026-06)」「libp2p-iroh による段階移行が可能」**
を追記すべき (本書がその一次情報)。

---

## 4. A6 (秘匿) — NVIDIA CC は本番運用フェーズに入った

- **Blackwell 世代 (RTX PRO 6000 / HGX B200 / HGX B300) は CC が
  ハードウェアに組み込み済**。B200/B300 は最大 8 GPU across で
  NVLink 暗号化に対応。
- **NRAS** (NVIDIA Remote Attestation Service) は、GPU のハードウェアレポートと
  CPU TEE 測定値 (AMD SEV-SNP / Intel TDX) を結合した署名済み evidence bundle を
  **RIM (reference integrity manifest)** と照合する。
  **attestation は起動時のみ**に行われ、**推論リクエストのランタイムに
  レイテンシ影響を与えない**。
- 本番事例: **Apple の Private Cloud Compute** が次世代 Apple Intelligence 機能で
  Blackwell + CC を Google Cloud 上で使用。Red Hat / Intel / Anjuna /
  Fortanix / Edgeless Systems 等がゼロトラスト AI ファクトリを構成。

### Rope への含意

- 「attestation は起動時のみ、ランタイム影響なし」という NRAS の設計は、
  Rope の `attestation_interval_seconds` による**鮮度チェック設計と整合する**
  (セッション確立時に検証し、以降は間隔内なら再検証しない)。既存設計は正しい。
- ただし Rope の `build_evidence_signature` (`confidential.rs:701`) は
  `unverified-digest:` prefix の偽署名のままであり、
  **NRAS の evidence bundle 形式に置き換える具体的な作業は未着手**。
  A6 の実装時は NRAS API を一次情報として参照すること。
- **W4 (TEE 訴求と消費者 GPU の矛盾) は解消していない** — CC 対応は
  依然としてデータセンター級 GPU (Hopper/Blackwell) に限られ、
  「他人のアイドル GPU」の主要供給源である消費者 RTX には無い。

---

## 4b. A2 (信頼確立) — 未知の他人をどう信頼するか、2 件の新着

第一原理監査 §2 で A2 は「TOFU のみ = 初回ピア選択に信頼の根拠なし」と判定した。
既存の `RESEARCH_IMPROVEMENTS.md` #10 は Bittensor / FORTYTWO を扱っているが、
以下 2 件は未収載であり、**いずれも Rope の既存資産と噛み合う**。

### (a) TraceRank — 支払いそのものを推薦として使う (arXiv:2510.27554)

"Sybil-Resistant Service Discovery for Agent Economies" (Operator Labs)。

- **中核アイデア**: 各支払いを「推薦 (endorsement)」とみなし、**支払者の評判で
  重み付け**して伝播させる。取引額と時間的近さでも重み付ける。
- **なぜ Sybil に強いか**: 支払い数を数えるだけなら Sybil スパムを招き、
  取引量で順位付けすると wash trading を招く — どちらも「誰が払ったか」を
  無視して量に偏る。TraceRank では、新規ウォレット (seed ≒ 0) が何回払っても
  寄与は無視できる一方、実績ある支払者の 1 回が大きく効く。
  結果として「多数の低評判支払者を持つスパム」は「少数の高評判支払者を持つ
  正規サービス」より下位になる。

**Rope への含意 (ここが重要)**: Rope は **既に ecash 決済を持つ** (A5)。
つまり TraceRank が必要とする「支払いフロー」は、A5 が結線された時点で
**副産物として自動的に手に入る**。A2 のために別途評判インフラを構築する必要はなく、
**A5 の決済履歴を A2 の信頼根拠に転用できる**。これは第一原理の依存グラフ
(§4) に対する新しい知見 — A5 は A2 の後段と位置づけていたが、
**A5 が動くと A2 の解法が一つ増える**という逆方向の依存が存在する。

ただし注意: Rope の ecash は **bearer token (無記名)** であり、Cashu の
プライバシー特性上、支払者の同一性を追跡しない設計。TraceRank をそのまま
適用すると **A6 (秘匿) / ecash の匿名性と衝突する**。採用するなら
「評判を担う識別子」と「支払いの無記名性」をどう両立するかの設計が必要 —
これは次の (b) が扱う問題そのもの。

### (b) DARTIC — 匿名性・評判・スケールの三立 (arXiv:2605.18146)

"Decentralized Anonymous Reputation at Scale for Trustworthy Crowdsourcing"。

- **問題設定**: 既存の分散評判は「匿名性・評判の紐付け・スケール」を
  同時に満たせていない、という指摘。上記 (a) の衝突とまさに同じ問題。
- **手法**: **dual-ledger** により、依頼者/作業者が相互作用ごとに異なる
  擬似名を使いつつ、**単一のアクセストークンに暗号学的に束縛**する。
  → 相互作用間の unlinkability (追跡不可能性) を保ちながら、
  **whitewashing (悪評を捨てて再参加する攻撃) を防ぐ**。
- **評判モデル**: **明示的なフィードバックではなく、検証可能な実行結果から
  評判を駆動する**。報復や操作のリスクを下げる。
- **スケール**: proof aggregation と Layer-2 実行、バッチ化で
  オンチェーン検証コストを削減。

**Rope への含意**: 2 点ある。

1. **A2 と A6 の衝突を解く既存研究が存在する** — 「匿名だが評判は持てる」は
   設計として実現可能であり、Rope が A2 を実装する際に
   「TOFU か、匿名性を捨てた実名評判か」の二択で考える必要はない。
2. **「検証可能な実行結果で評判を駆動」は §1 の Hollow-LLM と直結する** —
   評判の入力が「実行結果の検証」である以上、その検証が effort gap を
   見逃せば評判システムごと汚染される。**A2 の評判設計は A4 の検証強度に
   依存する**。第一原理の依存グラフに `A4 → A2` の辺を追加すべき。

### 判断: どちらも「採用」ではなく「A2 実装時の設計入力」として記録

- Rope は現時点で A1 (発見) すら動かないため、評判システムの実装は時期尚早。
- ただし **A2 の設計を始める時点で、上記 2 件を読んでから始めるべき**。
  特に「ecash の無記名性と評判の両立」は、後から接ぎ木すると
  アーキテクチャ全体をやり直すことになる種類の設計判断。
- **本文未読** (arxiv.org egress ブロック)。要旨レベルの記録に留める。

その他、同時期に見つかった関連 (要旨のみ、Rope への直接の含意は薄いが記録):
- **arXiv:2606.24942** — Proof-of-Useful-Work + ポスト量子安全な分散 AI 経済。
  ハッシュパズルを「外部価値のあるタスク」に置き換えつつ検証可能性と
  Sybil 耐性を保つ路線。Rope の「トークン不要」思想とは前提が異なる。
- **arXiv:2603.19452** TrustFlow — トピック認識のベクトル評判伝播。
- **arXiv:2605.00073** AgentReputation — エージェント向け分散評判フレームワーク。

---

## 4c. A5/A7 (対価移転・中断耐性) — DLEQ の優先度判定が第一原理では誤り

A5/A7 は第一原理監査 §2 で「実装済みだが未到達」(△) と判定され、
§4 の依存グラフでは「最も完成に近い、結線待ち」と位置づけた。
今回この前提を一次仕様で検証したところ、**既存文書の優先度判定に
明確な誤りが 1 件見つかった**。

### 発見: NUT-12 (DLEQ) は「任意・優先度低」ではなく A5/A7 の中核

`CASHU_BDHKE_IMPLEMENTATION_READINESS.md` は DLEQ を
**「Step 5 (任意、優先度低)」**に置いている。しかし NUT-12 の一次仕様
(`cashubtc/nuts` の `12.md` を GitHub raw 経由で取得・確認) を読むと、
これは Rope の構造上、**任意機能ではなく必須基盤**である。

**NUT-12 が保証すること**:
- DLEQ proof は `e` (challenge hash) と `s` (response) を含み、
  ユーザー間で渡す際は `r` (blinding factor) も添える。
- **受信者 (Carol) は mint に一切問い合わせずに**、
  `C' = C + r*A` / `B' = Y + r*G` で盲検値を復元し、
  `R1 = s*G - e*A`, `R2 = s*B' - e*C'`, `e == hash(R1,R2,A,C')` を検証できる。
- これにより「mint が `A = a*G` の生成と署名に**同じ秘密鍵を使った**」ことを
  暗号学的に確認でき、**mint が後から「その ecash は発行していない」と
  否認することを防ぐ**。

**なぜ Rope にとって中核なのか (第一原理での再評価)**:

1. **A7 (中断耐性) の前提**: Rope の streaming 課金 (`tick_stream`,
   `ecash.rs:982`) は 1 秒単位で proof をやり取りする設計。各 tick で
   mint に往復すると、**60 秒のジョブで最大 60 回の往復**が発生し、
   ネットワーク断で即座に中断する。DLEQ があれば受領時点でオフライン検証でき、
   **mint 往復なしで課金が進む** — これは既存文書
   (`CATEGORY_RESEARCH.md` §E2) が「ストリーミング秒課金で mint 往復を省く鍵」と
   正しく指摘していたが、**readiness 側の優先度に反映されていなかった**。
2. **A1 未達との相互作用 (これが決定的)**: Rope は現在 NAT 越えを持たず
   実質 LAN 止まり (W2)。**LAN 内取引では mint への到達性自体が保証されない**。
   DLEQ なしの ecash は「mint に繋がらなければ検証できない ecash」であり、
   Rope が最初に動く環境 (LAN・オフライン寄り) と最も相性が悪い。
   **A1 が制約されている今こそ DLEQ の価値が高い**という逆説。
3. **§1 (Hollow-LLM) との接続**: A4 の検証が計算量を見るべきなのと同型で、
   A5 でも「mint の署名が本物か」を**受け手側で独立に検証できる**ことが
   信頼の非対称性を減らす。DLEQ は A5 における effort gap 対策に相当する。

**判断**: `CASHU_BDHKE_IMPLEMENTATION_READINESS.md` の Step 5 から
**DLEQ を切り出し、優先度を引き上げるべき** (P2PK は任意のままでよい)。
本書はその根拠となる一次仕様の確認結果を提供する。

### 参考: SAKSHI の設計 (arXiv:2307.16562) — 経路分離という視点

"SAKSHI: Decentralized AI Platforms" (Princeton / UIUC / 清華 / HKUST +
Witness Chain / EigenLayer)。分散 AI サービスの trust-free 基盤。

- **中核の設計原則**: **データ経路** (AI クエリと応答) / **制御経路**
  (ルータと計算・ストレージホストの管理) / **決済経路** (課金と支払いを
  ブロックチェーン上で管理) の **3 つを分離**する。
- この分離を可能にするのが **"proof of inference" レイヤ**で、
  劣悪な AI サービス・不払い・モデル複製といった多様な不正に
  暗号学的耐性を与える。
- 紛争解決: マイクロペイメント自体が**支払い済みの証明**として機能し、
  「支払いを受けていない」という主張の解決に使える。支払いチャネルを
  紛争対応にするには、対象推論を指す `requestID` と**直前のマイクロペイメントの
  ハッシュ**を含める必要がある (nonce で検証可能)。

**Rope への含意**: Rope の `StreamSession` (`ecash.rs`) は現在、
tick ごとの `drained_sats` を**自分のローカル状態としてのみ**持ち、
「この tick 分を確かに払った」ことを相手に示す証跡が無い。
SAKSHI 式に **各マイクロペイメントへ直前のハッシュを連鎖させる**と、
中断時に「どこまで払ったか」を双方が独立に証明できる — これは A7 の
「中断しても双方が損しない」を実際に成立させる具体的な機構。
ただし Rope はブロックチェーンを持たない (「独自トークン不要」が売り) ため、
SAKSHI の決済経路をそのまま移植はできない。**ハッシュ連鎖の部分だけを
借りる**のが現実的。

**判断**: 採用ではなく、**A7 を実装する際の設計入力**として記録。
Rope の escrow は既に `completion_condition`/`completion_proof` を持つため、
連鎖ハッシュを載せる器は存在する。

---

## 5. 今回の調査が既存文書に要求する更新

| 更新先 | 内容 | 根拠 |
|---|---|---|
| `RESEARCH_IMPROVEMENTS.md` #1 (検証) | **受け入れ基準に「投入計算量の検証」を追加**。出力の正しさだけを見る手法は Hollow-LLM 系に破られる | §1 |
| `RESEARCH_IMPROVEMENTS.md` #1 | TensorCommitments の採否を**保留扱いに変更** (effort gap 耐性が未確認) | §1 |
| `P2P_IMPLEMENTATION_READINESS.md` | Iroh **1.0 正式リリース済 (2026-06)** / `libp2p-iroh` による段階移行 | §3 |
| `FIRST_PRINCIPLES_AUDIT.md` §8 | A3 の推奨 crate を **mistral.rs に確定** (pure Rust / CPU 可 / Candle 0.9.2) | §2 |
| `FIRST_PRINCIPLES_AUDIT.md` §4 | **依存グラフに 2 辺を追加**: `A5 → A2` (決済履歴が信頼根拠に転用できる) と `A4 → A2` (評判の入力が検証結果なら、検証が弱いと評判ごと汚染される) | §4b |
| `RESEARCH_IMPROVEMENTS.md` #10 (Sybil) | TraceRank (2510.27554) / DARTIC (2605.18146) を追加。特に **ecash の無記名性と評判の両立**は A2 着手前に設計判断が要る | §4b |
| `CASHU_BDHKE_IMPLEMENTATION_READINESS.md` Step 5 | **DLEQ (NUT-12) を「任意・優先度低」から格上げ** — streaming 課金の mint 往復回避と、A1 未達 (実質 LAN) 環境での検証可能性に必須。一次仕様の検証アルゴリズムを確認済み | §4c |
| `RESEARCH_IMPROVEMENTS.md` (A7 関連) | SAKSHI (2307.16562) の**マイクロペイメント連鎖ハッシュ**を A7 実装時の設計入力として追加 | §4c |
| `CLAUDE.md` 改善案表 | 推論エンジン行に mistral.rs を明記 | §2 |

---

## 6. 調査の限界 (正直な記録)

- **arxiv.org が egress proxy でブロックされている**ため、
  §1 の 4 論文はいずれも**要旨・検索結果ベース**であり本文未読。
  Hollow-LLM の具体的な攻撃構成、TensorCommitments の防御可否は
  **本文を読むまで確定できない**。実装判断の直前に必ず一次資料を読むこと。
- crates.io の個別パッケージページも本環境では内容取得に失敗した
  (SPA のため)。mistral.rs の MSRV / 依存ツリーは
  **ビルド可能環境での `cargo add` 実測が唯一の確認手段**。
- 本書は「何を調べたか」と「何が未確定か」を分けて記録している。
  未確定を確定として扱わないこと (`CLAUDE.md` §4 規範6「正直さの文化」)。

---

## 参照 (全て 2026-08-08 に WebSearch で実在確認)

- arXiv:2607.28884 — Hollow-LLM Attack (IEEE S&P'26)
- arXiv:2606.16352 — Communication-Efficient Verifiable Attention (VeriAttn)
- arXiv:2603.18046 — NanoZK
- arXiv:2602.12630 — TensorCommitments (前回調査で既出、今回再評価対象)
- arXiv:2510.27554 — Sybil-Resistant Service Discovery / TraceRank (Operator Labs)
- arXiv:2605.18146 — DARTIC (分散匿名評判、dual-ledger)
- arXiv:2606.24942 / 2603.19452 / 2605.00073 — PoUW・TrustFlow・AgentReputation (記録のみ)
- arXiv:2307.16562 — SAKSHI (データ/制御/決済の経路分離、proof of inference)
- `cashubtc/nuts` `12.md` — NUT-12 DLEQ **一次仕様を GitHub raw 経由で取得・検証**
  (本調査で唯一、要旨ではなく仕様本文を確認できた資料)
- `EricLBuehler/mistral.rs` — pure Rust 推論エンジン (Candle ベース)
- `utilityai/llama-cpp-rs` (`llama-cpp-2`) — llama.cpp FFI バインディング
- iroh.computer / StackRadar — Iroh 1.0 (2026-06 リリース)
- NVIDIA Technical Blog / NVIDIA Secure AI whitepaper — Blackwell CC + NRAS
- Apple PCC × NVIDIA Blackwell 発表 (NVIDIA Blog)

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

## 5. 今回の調査が既存文書に要求する更新

| 更新先 | 内容 | 根拠 |
|---|---|---|
| `RESEARCH_IMPROVEMENTS.md` #1 (検証) | **受け入れ基準に「投入計算量の検証」を追加**。出力の正しさだけを見る手法は Hollow-LLM 系に破られる | §1 |
| `RESEARCH_IMPROVEMENTS.md` #1 | TensorCommitments の採否を**保留扱いに変更** (effort gap 耐性が未確認) | §1 |
| `P2P_IMPLEMENTATION_READINESS.md` | Iroh **1.0 正式リリース済 (2026-06)** / `libp2p-iroh` による段階移行 | §3 |
| `FIRST_PRINCIPLES_AUDIT.md` §8 | A3 の推奨 crate を **mistral.rs に確定** (pure Rust / CPU 可 / Candle 0.9.2) | §2 |
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
- `EricLBuehler/mistral.rs` — pure Rust 推論エンジン (Candle ベース)
- `utilityai/llama-cpp-rs` (`llama-cpp-2`) — llama.cpp FFI バインディング
- iroh.computer / StackRadar — Iroh 1.0 (2026-06 リリース)
- NVIDIA Technical Blog / NVIDIA Secure AI whitepaper — Blackwell CC + NRAS
- Apple PCC × NVIDIA Blackwell 発表 (NVIDIA Blog)

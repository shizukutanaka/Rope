# A3+A9 実装レディネス — 推論を実際に走らせる (v1 の第1項目)

> # ✅ 実装完了 (2026-08-18) — ただし本書の前提は 1 つ外れた
>
> `src/net/inference.rs` として実装済み。**本書が推奨した mistral.rs は使って
> いない。**
>
> **外れた前提**: 本書は「A3 は新規 crate (mistral.rs) を要するので `build`
> ブロック」としていた。しかし mistral.rs が提供するのは **GPU カーネル・
> 量子化形式の広さ・速度**であって、**「推論を実行できる」という能力そのもの
> ではない**。Transformer の forward pass は算術であり、`std` の f32 演算で足りる。
>
> **依存ゼロにした結果**:
> - `static.crates.io` が塞がれたこの環境でも**コンパイルでき、テスト 22 件が
>   実際に走る** (crate 全体は今も `cargo test` 不可)
> - A9 の攻撃面が最小 — 外部コードを一切ロードしない
>
> **実装した範囲**: RMSNorm / RoPE / grouped-query attention + KV キャッシュ /
> SwiGLU FFN / temperature・top-p サンプリング / llama2.c legacy-v1 checkpoint
> ローダ / SentencePiece 系トークナイザ (貪欲マージ + バイトフォールバック)。
>
> **実装していない範囲**: GPU バックエンド (`InferenceEngine` トレイトを実装
> すれば足せる)、量子化 (f32 のみ)、プロセス隔離 (A9 の「隔離」の部分)。
>
> **速度**: mistral.rs に遠く及ばない。v1 の要件は「小型モデルが CPU で動く」
> ([`V1_SCOPE.md`](V1_SCOPE.md) §3) であり、それは満たす。
>
> 以下は実装前の調査記録として残す。配線点の記述 (§(a)(b)(c)) は
> **(a) と (c) が解消済**、**(b) (デモの固定文字列) は未着手**。

> 作成: 2026-08-17。`P2P_IMPLEMENTATION_READINESS.md` /
> `CASHU_BDHKE_IMPLEMENTATION_READINESS.md` と同じ形式の runbook。
> **対象読者**: `cargo build` が復旧した後、[`V1_SCOPE.md`](V1_SCOPE.md) の
> 順序 1 (A3+A9) に着手するセッション。
>
> **なぜこの runbook が最後に書かれたか**: A3 は第一原理監査で
> **「型すら存在しない唯一の公理」**と判明した (§2)。他の 2 ギャップには
> 手順書があったが、**最も重い項目に手順書が無い**状態だった。
>
> **A9 と不可分**: 借り手のワークロードを*実行させる*ことは、同時に*隔離する*ことを
> 要求する (監査 §4)。本書は両方を 1 つのスコープとして扱う。

---

## 0. 現状: プレースホルダの正確な位置

**3 箇所すべて grep 確認済み。**

### (a) `main.rs:478-481` — `run_inference` が計画を表示して終わる

```rust
capability_boundary!(
    working: "Intent → ExecutionPlan 構築 (型安全な準備状態)",
    next: "実 GPU 推論実行 (cashu_mint + Noise 結線)"
);
```

**コード自身が「next: 実 GPU 推論実行」と宣言している。ここが配線点。**
直前 (`main.rs:378`) で `mgr.resolve(...)` から `ExecutionPlan` を受け取り、
`main.rs:382` (`format_plan`) で内容を表示し、**実行せずに終わる**。

### (b) `main.rs:226` — デモが固定文字列を注入する

```rust
let sampled = orch.first_run.sample_haiku_response().to_string();
orch.step_demo_completed(&sampled)?;
```

`step_demo_completed` (`first_run.rs:480`) は**渡された文字列を保存するだけ**。
**`rope` 無引数デモの「推論」はここで完全にシミュレート**されている
(README がその旨を明記済み)。

### (c) `intent.rs:574` — `resolve()` が返す計画を誰も実行しない

`ExecutionPlan` (`intent.rs:348-360`) は `steps` / `selected_model_variant` /
`selected_provider` を持つが、**それを受け取って実行する主体が存在しない**。

---

## 1. v1 における A3 の定義 (最小充足条件)

[`FIRST_PRINCIPLES_AUDIT.md`](FIRST_PRINCIPLES_AUDIT.md) §8 で演繹済み。
**v1 では以下で十分**であり、これを超えるものは作らない:

| 要素 | v1 | 理由 |
|---|---|---|
| デバイス | **CPU で可** | GPU は A3 の成立条件ではない。60 秒は A8 の体験要求 |
| 決定論性 | **不要** | 決定論は **A4 (検証) の要求**であって A3 の要求ではない |
| TEE | **不要** | **A6 は v1 から削除** ([`V1_SCOPE.md`](V1_SCOPE.md) §2) |
| モデル取得 | **ローカルパスのみ** | **`Train` を削除したので借り手指定 URI は存在しない** |
| ワークロード | **`Workload::Inference` のみ** | `Train`/`Retrieve` は v1 から削除 |

→ **「CPU・非決定論的・非TEE・ローカルモデルパス・プロンプトのみ」**。

---

## 2. crate 選定: mistral.rs (2026-08 調査で確定)

[`RESEARCH_UPDATE_2026-08.md`](RESEARCH_UPDATE_2026-08.md) §2 の結論:

| | mistral.rs | llama-cpp-2 |
|---|---|---|
| 実装 | **pure Rust** (HF **Candle** 上) | llama.cpp への **C FFI** |
| CPU | **動く** (Candle が CPU/CUDA/Metal を吸収) | 動く |
| API 安定性 | Candle 0.9.2 の crates.io 公式リリースへ移行済 | **作者が「安定 API を目指さず semver も意味を持たない」と明言** |
| Rope 方針との整合 | `unsafe_code = "deny"` / pure Rust 依存と**一致** | **C++ ツールチェーン依存が「依存最小主義」と衝突** |

**推奨: `mistral.rs`。** 性能で llama-cpp-2 が上回っても、FFI と API 不安定性の
コストが Rope の既存方針と正面から衝突する。

### ⚠️ 着手時に必ず確認すること (未確認事項)

- **MSRV 1.75 を満たすか**、依存が **edition2024 を要求しないか**。
  `cargo add` 直後に即ビルド確認 (BDHKE runbook §4 が k256 について述べたのと同じ注意)。
  **満たさない場合は llama-cpp-2 へのフォールバックではなく、まず上限固定を試す**
  (`Cargo.toml` の既存パターン: `base64ct`/`getrandom`/`cpufeatures`)。

---

## 3. A9 (不可分) — v1 で必要な隔離は「プロセス隔離」まで

**`Train`/`Retrieve` を削除したことで、A9 の要求が大幅に軽くなっている**
([`V1_SCOPE.md`](V1_SCOPE.md) §2):

| 脅威 | v1 での状態 |
|---|---|
| SSRF (借り手指定 URI) | **消滅** — URI を取る variant が無い |
| pickle RCE (借り手指定 `base_model`) | **消滅** — 借り手はモデル名しか渡さず、**貸し手のローカルモデルを解決する** |
| GPU side channel / CVE-2026-22164 | **残る** — ただし v1 は **GPU レベルの保護を提供しないと開示する** |
| リソース枯渇 | **残る** — `max_output_tokens` (`intent.rs:136`) で上限、+ プロセスの rlimit |

### 使う道具: pure Rust の隔離 crate

**gVisor は不要** — GPU 呼び出しへの介在が要るのは A6/GPU レベル保護の場合であり、
**v1 はそれを提供しないと明言する**ため。**ゼロコンフィグ (A8) を守る方を取る**
(§4j の `A9 ⊥ A8` は、v1 では「A8 を選ぶ」と決着済み)。

候補 (いずれも Linux、`landlock` は 5.13+):
- **`sandbox-rs`** — namespace / cgroup v2 / seccomp BPF / Landlock。
  **unprivileged モードは root 不要** (user namespace + seccomp + landlock + setrlimit)
- **`sandlock-core`** — Landlock + seccomp-bpf。**root も cgroups もコンテナも不要**
- `hakoniwa` / `landlock` — 同系統

**MSRV / 依存ツリーは未確認** — mistral.rs と同じく `cargo add` 実測が必要。

### モデル形式は形式で制約する

**SafeTensors / GGUF のみを受け付ける。`torch.load`/pickle は使わない。**
理由 ([`SURPLUS_AND_GAPS.md`](SURPLUS_AND_GAPS.md) §1.11): pickle はロード中に
任意コードを実行し、**バリデータが見るのは `__reduce__` 実行後**。
スキャナは 63% 回避される。**v1 では貸し手のローカルモデルのみだが、
形式制約は最初から入れる** (後から入れると v2 で `Train` を戻す時に忘れる)。

---

## 4. 実装順序

### Step 1: `net/inference.rs` を新設 (feature 分離)

- **置き場所の根拠**: `core/` は「I/O 無しの pure state machine」が既存方針
  (`ARCHITECTURE.md`)。推論は本質的に I/O。
  **`net/cashu_mint.rs` が `#[cfg(feature = "http")]` で分離している前例**に倣う。
- `Cargo.toml`: `inference = ["dep:mistralrs", ...]`、`default = []` のまま。
- **公開 API は最小に**: `fn run_inference(model_path: &Path, prompt: &str,
  max_tokens: u32) -> Result<String>` 相当 1 本から始める。
- **この Step だけで独立にテストできる** — 他のコードを一切変更しない。

### Step 2: モデル解決層 (借り手の `String` → 貸し手のローカルパス)

- `Workload::Inference { model: String, .. }` の `model` は**素の `String`** で、
  **型が形式も取得元も制約しない** (§1.11)。
- **貸し手側の allowlist で解決する**: 設定 (`~/.rope/models.json` 等) に
  「このモデル名 → このローカルパス」を持たせ、**未登録の名前は拒否**。
- **newtype にする価値がある**: `SafeModelRef` 等で「解決済み・形式検証済み」を
  型で表す。v2 で `Train` を戻す時にこの型が防波堤になる。

### Step 3: `run_inference` の `capability_boundary!` を実呼び出しに置換

- `main.rs:387-390` の macro を、`plan` から
  `selected_model_variant` を取り出して Step 1/2 を呼ぶコードに置換。
- **`capability_boundary!` を消す前に、次に何が未実装かを再宣言する**
  (例: `next: "リモートピアでの実行 (現状はローカル実行のみ)"`) —
  正直さの文化 (`CLAUDE.md` 規範6) を守る。

### Step 4: A9 の隔離を同スコープで入れる

- Step 1 の推論呼び出しを、隔離下で実行するよう包む。
- **`panic_stop` (`session.rs:399-429`) の docker 前提を、選んだ隔離技術に合わせる**
  (`SURPLUS_AND_GAPS.md` §2.4)。
- **同時に `A9→A7` の欠落 (§1.10) を直す** — 緊急停止時に**未完了の** escrow を返金
  (`Deposited | InProgress` のみ。完了済みを返金すると貸し手の攻撃面になる)。

### Step 5: デモを本物にする

- `main.rs:226` の `sample_haiku_response()` を **実推論の出力**に置換。
- **これが完了の最も強い signal** — README の
  「デモはシミュレーション」注記を**正直に外せる**唯一の条件。

---

## 5. 影響範囲マップ

| ファイル | 変更 | リスク |
|---|---|---|
| `Cargo.toml` | `mistralrs` + 隔離 crate を optional 依存で追加、`inference` feature | **中** — edition2024 汚染の再発リスク。`cargo add` 直後に即ビルド |
| `src/net/inference.rs` (新規) | 推論の実呼び出し。**I/O 層** | 中 (新規コード、ただし独立してテスト可能) |
| `src/net/mod.rs` | `#[cfg(feature = "inference")] pub mod inference;` | 低 |
| `src/main.rs:387-390` | `capability_boundary!` → 実呼び出し | 中 (**唯一の到達点なので慎重に**) |
| `src/main.rs:226` | `sample_haiku_response()` → 実出力 | 中 (デモ経路。README の注記と連動) |
| `src/core/intent.rs` | **変更不要** — `ExecutionPlan` はそのまま使える | — |
| `src/core/session.rs` | `panic_stop` の隔離前提を更新 (Step 4) | 中 |
| `src/core/ecash.rs` | `A9→A7` 修正 (Step 4)。§1.8/§1.9 も**この機会に直す** | **高** (マネーパス。敵対的レビュー推奨) |

---

## 6. 注意点

- **A8 のウォーム前提を壊さない**: cold start は実測 40 秒超 (§4f)。
  **v1 のデモモデルは 1B 級のまま** (`first_run.rs:186`) にし、
  **モデルを常駐させる設計**を Step 1 の時点で織り込む
  (後から足すのは困難 — 監査 §4 の `A6⊥A8` と同根)。
- **`Inference` 以外の variant を実装しない**。`Train`/`Retrieve` は v1 で削除済み
  ([`V1_SCOPE.md`](V1_SCOPE.md) §2)。**「ついでに」実装すると SSRF と pickle RCE の
  攻撃面が戻ってくる**。
- **リモート実行はこの runbook のスコープ外**。v1 の Step 3 は
  **ローカル実行で良い** — リモートは A1 (順序 3) が動いてから。
  **A3 とリモート化を同時にやらない**。
- **ecash マネーパスに触るなら敵対的レビューを** (`CLAUDE.md` 規範2)。
  本セッションで active_sessions 回帰が単独レビューを通過した実例がある。

---

## 7. 完了の定義 (Definition of Done)

- [ ] `cargo add` 後にフルゲートが通る (`build` / `test` ×2 feature /
      `clippy -D warnings` ×2 / `fmt --check`)
- [ ] `mistralrs` (と隔離 crate) が **MSRV 1.75 を満たし edition2024 を要求しない**
- [ ] `net/inference.rs` が **ローカルの SafeTensors/GGUF モデルで実際にトークンを返す**
- [ ] **未登録のモデル名は拒否される** (allowlist が効いている)
- [ ] `rope run <model> --prompt "..."` が **モデルの実出力を表示する**
      (`capability_boundary!` が実呼び出しに置換されている)
- [ ] **`rope` 無引数デモが実推論を表示する** — `sample_haiku_response()` 不使用
- [ ] → **README の「デモはシミュレーション」注記を外せる**
      (外すこと自体が DoD。外せないなら A3 は未完了)
- [ ] 推論がプロセス隔離下で走る。**GPU レベルは保護しないと `SECURITY.md` に開示**
- [ ] `panic_stop` が新しい隔離技術で動く
- [ ] **`A9→A7` (§1.10) が修正済** — 緊急停止で未完了 escrow が返金され、
      **完了済みは返金されない**
- [ ] §1.8 / §1.9 のマネーパス不整合が**修正済**
- [ ] `FIRST_PRINCIPLES_AUDIT.md` §2 の **A3 行と A9 行を更新** (△/✗ → ○/✓)

---

関連: [`V1_SCOPE.md`](V1_SCOPE.md) (なぜこれが順序1か) /
[`FIRST_PRINCIPLES_AUDIT.md`](FIRST_PRINCIPLES_AUDIT.md) §8 (最小充足条件の演繹) /
[`RESEARCH_UPDATE_2026-08.md`](RESEARCH_UPDATE_2026-08.md) §2・§4j・§4l (crate 選定・隔離・形式制約) /
[`SURPLUS_AND_GAPS.md`](SURPLUS_AND_GAPS.md) §1.10・§1.11 (同時に直す欠落)

---

## 追記 2026-08-18 — 行番号アンカーの再取得と、削除で先回りされた作業

本書の `file:line` は `Workload::Train`/`Retrieve` 削除
([`V1_SCOPE.md`](V1_SCOPE.md) §2 / [`SURPLUS_AND_GAPS.md`](SURPLUS_AND_GAPS.md)
§1.11) と `format_plan` 結線に伴い**再取得済み** (上記本文に反映)。

この 2 つの変更で、本書の作業の一部は**着手前に不要になった**:

- **URI フェッチの防御 (SSRF / DNS ピン留め / IP 検証) は v1 では書かなくてよい。**
  借り手が URI を渡す経路そのものが型から消えた。`Train` を v2 で戻す時に
  §1.11 の要件がそのまま復活する。
- **`rope run` の計画表示は `format_plan` に一本化済み。** A3 を結線する時に
  触るのは `main.rs:387-390` の `capability_boundary!` だけでよく、表示整形を
  書き足す必要はない。

**残っている A9 の絶対条件は 1 つ**: `Inference.model` が bare `String` である
こと (`intent.rs:134`)。名前から重みを解決する層が **SafeTensors/GGUF のみ受理**し、
pickle を絶対に通さないこと。`SafeModelRef` newtype 化はここで行うのが自然。

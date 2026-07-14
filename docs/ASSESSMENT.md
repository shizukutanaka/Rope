# 現段階の評価 — 長所 / 短所 / 改善点

> 評価日: 2026-06-23 (初版) / 更新: 2026-07-10 (v0.2.13 + Unreleased 分反映) /
> 対象: Rope (branch `claude/deepresearch-ultrathink-improve-wnYtn`)
> 関連: [`RESEARCH_IMPROVEMENTS.md`](RESEARCH_IMPROVEMENTS.md)（優先度バックログ）,
> [`RESEARCH_UPDATE_2026-07.md`](RESEARCH_UPDATE_2026-07.md)（1ヶ月差分）,
> [`CATEGORY_RESEARCH.md`](CATEGORY_RESEARCH.md)（10カテゴリ調査）,
> [`SURPLUS_AND_GAPS.md`](SURPLUS_AND_GAPS.md)（機械可読な過不足一覧）

## 長所 (Strengths)

1. **設計の極端な集中** — 4 動詞 / 7 core モジュール。main.rs 652 行。
   競合機能の交点 (他人GPU × GPU TEE × ecash少額決済) に wedge を絞れている。
2. **状態機械が明快** — `ecash` / `pair` / `confidential` / `intent` が型と状態遷移で
   表現され、純ロジックとして単体テスト可能。I/O と分離されている。
3. **ローカル品質ゲートが厳格** — `clippy -D warnings` / `fmt --check` /
   `unsafe_code = deny` を crate 全体に強制。テスト 239+ (default) / 245+
   (http、本セッションの regression test 込みで未検証)。全 4 動詞 graceful
   exit。**ただし CI 自体は `.github/ci.yml.disabled` のまま未有効化** —
   「ローカルで厳格」と「自動化されている」は別物である点は誤解しないこと
   (詳細: 改善点テーブルの CI 行)。
4. **依存最小主義** — HTTP は opt-in feature、edition2024 を避けた上限固定で
   ビルド再現性を確保。
5. **正直なフォールバック設計** — TEE 非対応や検証なしを「できるフリ」せず、
   明示的に拒否/注記する方向へ転換済み。README冒頭のデモにも「これは
   シミュレーション」と明記 (`README.md:28-34`)、`SECURITY.md` で
   「本物として機能している暗号」と「まだプレースホルダの暗号」を
   明確に分離して開示。
6. **未結線コードが自己文書化されている** — `#![allow(dead_code)]` 配下
   161 関数を全数調査した `REACHABILITY_AUDIT.md`、その要約と判断根拠を
   まとめた `SURPLUS_AND_GAPS.md`、さらに主要な「テスト済みだが到達不能」
   構造体 (`SessionManager`/`EcashManager`/`PairManager`) 自体に
   doc comment でステータスを明記済み。「なぜこのコードが呼ばれていないのか」
   を推測せずコード自身から読み取れる状態になっている。
7. **次の実装に必要な調査が事前に済んでいる** — 最大の2ギャップ (P2P実結線、
   Cashu BDHKE) について、ネットワーク制約解消後すぐ着手できる実行手順書
   (`P2P_IMPLEMENTATION_READINESS.md`, `CASHU_BDHKE_IMPLEMENTATION_READINESS.md`)
   を用意済み。BDHKEの数式はCashu公式仕様を直接検証、P2Pはlibp2p/Iroh
   双方の比較検討まで完了している。

## 短所 (Weaknesses)

1. **実 I/O が未結線** — `pair` の Noise/mDNS/DHT、`ecash` の BDHKE 暗号、
   `confidential` の NRAS 検証はモデルのみ。唯一の実 I/O は `http` の Cashu mint。
   → 現状は「出荷可能なスケルトン」であり、エンドツーエンドでは動かない。
2. **NAT 越えが無い** — 「他人の GPU」は別 NAT 背後が普通だが、ホールパンチング/relay 未実装。
   実質 LAN デモ止まり（最大の機能的欠落）。
3. **検証 (proof-of-execution) が薄い** — `VerificationLevel` は enum 中心。
   再実行/LSH コミット等の実機構は未実装（本セッションで escrow ゲートのみ前進）。
4. **永続化はチェックポイント止まり** — `atomic_write`/`load_or_recover` 自体は健全
   (tmp→rename、破損時デフォルト復帰) だが、`save_ecash` 等の呼び出しは各 CLI 動詞の
   末尾 1 箇所のみ。動詞内の個々の状態遷移 (mint/escrow open/tick) ごとには保存されない。
   現状は実 ecash 操作がどの動詞からも呼ばれていないため実害は無いが、
   v0.3 で `rope run`/`rope earn` に実結線した瞬間、遷移の合間のクラッシュで
   bearer token/escrow を失う金銭損失リスクへ昇格する（要 WAL 化）。
5. **Sybil/評判の根拠が薄い** — `trust_store` は TOFU のみ。初回ピア選択の信頼基盤なし。
6. **単一ピア前提** — 大規模モデルの複数ピア pipeline 分割なし。
7. **「CI green」表記が実態と不一致だった** — CI 定義ファイルは import 時から
   `.github/ci.yml.disabled` という名前・場所にあり、`.github/workflows/` 直下に
   置かれていないため GitHub Actions が一度も認識・実行していなかった
   (v0.1.0 の頃から)。README/CHANGELOG の「CI green」は実際にはローカルでの
   `cargo test`/`clippy`/`fmt` 実行結果であり、CI による自動ゲートではなかった。
   本ラウンドで表記を訂正 (`「cargo test 実通過、ローカル確認」`)。有効化自体は
   リポジトリのシークレットにアクセスする自動パイプラインを起動する操作のため、
   本セッションでは明示承認が下りず保留 — 有効化するかはユーザー判断。

## 改善点と本セッションでの改良 (Improvements)

| 改善点 | 状態 | commit |
|--------|------|--------|
| 二重使用検出 O(n)→O(1) (`SpentNullifiers`) | ✅ 改良済 | `4af5ba7` |
| 機密計算は attestation 検証必須 (#2×#3) | ✅ 改良済 | `9743e4e` |
| README の TEE プライバシー前提を明記 | ✅ 改良済 | `3708da3` |
| `EnergyPreference` を resolver に配線 (#13-2) | ✅ 改良済 | `6bfb6da` |
| escrow 解放に検証ゲート (#1×#5, free-riding 防止) | ✅ 改良済 | `af421e9` |
| **Sybil 耐性 (proof-of-capability + 評判スコア)** | ✅ 改良済 | `a84d162` |
| **ecash 3 欠陥 (問⑪⑫⑬)** | ✅ 改良済 | `741f99d` |
| **session 安全/権限プリミティブ硬化 (問⑭⑮)** | ✅ 改良済 | `b17864a` |
| **confidential/intent 3 欠陥 (問⑯⑰⑱)** | ✅ 改良済 | `06a6693` |
| **mint 額面検証 + lock liveness (問⑲⑳)** | ✅ 改良済 | `036b8fe` |
| **receive_proofs 二重加算 + lock_funds 価値消滅 (問㉑㉒)** | ✅ 改良済 | `f21f826` |
| **LockGuard/prune_stale の exit(1) 契約違反** | ✅ 改良済 | `66d66da` |
| **QR nonce リプレイ検出** | ✅ 改良済 | `eb648bc` |
| **attestation ポリシー鮮度チェック統一** | ✅ 改良済 | `6c54889` |
| **NUT-07 wire format 修正 (Ys/Y) + mint C 形式検証** | ✅ 改良済 | `701d077` |
| **ARCHITECTURE.md 全モジュール行数/テスト数の実測再生成** | ✅ 改良済 | `c579a3a` |
| **README/ASSESSMENT の壊れたリンク・CI green 誤表記の是正** | ✅ 改良済 | `71f9ca0` |
| **src/lib.rs 新設 (core/net を真にライブラリ化)** | ✅ 改良済 | `dd02372` |
| **library_usage.rs を実行可能な例へ書き換え** | ✅ 改良済 | `7f279d3` |
| **`cargo install rope` が実際には動かない問題の是正** | ✅ 改良済 | `71f9ca0` |
| **intent::select_provider が実 TEE 状態を無視する欠陥 (最重要)** | ✅ 改良済 | `a33c7c3` |
| **`rope run --verification` フラグ新設** | ✅ 改良済 | `a33c7c3` |
| **非TEE FederatedPeer 分岐も PairManager の実状態を無視 (問㉖、TEE修正の適用漏れ)** | ✅ 改良済 | `1618a40` |
| **`ecash::LightningLink` 完全不活性構造体の削除 (問㉗)** | ✅ 改良済 | `b13b13a` |
| **proof-of-capability の delete-or-keep 判断 (問㉛、ロードマップ有り→保持)** | ✅ 判定済 (保持、doc明記) | `169b222` |
| **QR ペアリングが CLI 未結線な理由の検証 (問㉙㉚)** | ✅ 検証済 (バグではなく一貫した scope 境界と判定) | `169b222` |
| **SecurityPolicy サブシステムの削除 (問㉜、ロードマップ無しと判定)** | ✅ 改良済 | `e7144e8` |
| **`TeeStatus::Running` 未代入バグ (期限切れ再検証が機能不全)** | ✅ 改良済 | `f2ce715` |
| **死蔵フィールド5件削除 (Intent.tags/duration, JobSpec, ConfidentialStats×2, FirstRun×2)** | ✅ 改良済 | `281ee2e` |
| **`RegionConstraint` の doc comment 明記 (ロードマップ裏付けあり、保持)** | ✅ 改良済 | `281ee2e` |
| **`EcashStats.current_balance_sats` 冗長ミラー削除** | ✅ 改良済 | `f025b3c` |
| **write-only lifetime カウンタ3件を表示配線 (delete-or-wire の wire 判定例)** | ✅ 改良済 | `f025b3c` |
| **`StreamState::Opening/Paused/Closing` 削除 (構築経路ゼロ)** | ✅ 改良済 | `9ebcf65` |
| **`DiscoveryMethod::Contact` 削除 (構築経路ゼロ)** | ✅ 改良済 | `9ebcf65` |
| **`TrustLevel::OwnDevice` の doc comment 明記 (未結線の判定分岐、保持)** | ✅ 改良済 | `9ebcf65` |
| **`SessionStatus` の doc comment 明記 (v0.3 実TEEセッション待ち、保持)** | ✅ 改良済 | `9ebcf65` |
| **QR 入力経路 (`accept_pairing_token`) の `.expect()` を `.context(...)?` に強化** | ⚠️ 改良済・**未検証** (下記参照) | `c8a8061` |
| **`ARCHITECTURE.md` 統計テーブルの同期 (v0.2.5→v0.2.13 時点値)** | ✅ 改良済 | `c8a8061` |
| **README「60秒デモ」がシミュレーションである旨の明示** | ✅ 改良済 | `c8a8061` |
| **`load_confidential/load_pair` の破損vs未設定区別を調査** | ✅ 判定済 (既に `load_or_recover` が対応済、修正不要) | `c8a8061` |
| **`SECURITY.md` 新設** (本物/プレースホルダ暗号の分離開示、信頼モデル) | ✅ 改良済 | `e068e2d` |
| NAT 越え (libp2p DCUtR) / Noise 実結線 | ⏳ 大・新規依存要 (この環境では crates.io 制約で実施不能と判明) | — |
| 検証本体 (VeriLLM 風 再実行 / TOPLOC LSH) | ⏳ 大 (実推論エンジン自体が未実装) | — |
| BDHKE 実装 (secp256k1、現状は unblinding プレースホルダ) | ⏳ 大・新規依存要 (この環境では crates.io 制約で実施不能と判明) | — |
| 永続化の WAL 化 (現状は動詞末尾のチェックポイントのみ) | ⏳ 中 | — |
| **CI 有効化** (`.github/workflows/` への移動) | ⏳ 要ユーザー判断 (secrets アクセス) — auto mode 許可分類器が明示同意無しとしてブロック。`.disabled` 内容は改善済み (全ブランチ trigger 化 + http feature ジョブ追加) | — |
| pair.rs QR/TOFU 系統計7件の表示配線 (診断情報過多の懸念、見送り) | ⏳ 次回判断 | — |
| **`#![allow(dead_code)]` (`lib.rs`/`main.rs`) の到達可能性監査**: 161 pub fn を4動詞からの呼び出しチェーンで分類 (到達68・test-only 64・呼出ゼロ15・連鎖デッド14)。詳細: [`docs/REACHABILITY_AUDIT.md`](REACHABILITY_AUDIT.md) | ✅ 監査完了 | — |
| **呼出ゼロ15件の個別再検証・実施**: 単純に陳腐化した4件を削除 (`load_private_key`/`load_config`/`ensure_initialized`/`get_plan`)、1件は削除ではなく配線 (`confidential::update_stats` — `format_confidential` 表示用キャッシュが常に0固定だった)、2件は設計意図ありと判断し保持 (`with_region`/`is_cpu_tee`) | ✅ 改良済 | `a87d456` |
| **`session.rs` の10件クラスター (`SessionManager`/`panic_stop` 系)**: 個別に読んだ結果、安全性クリティカル (緊急停止) / 設計意図明記 (v0.3 async化コメント) のコードと判明。「呼出ゼロ」のみを根拠に削除すべきでないと判断し、製品判断が必要なため保持 | ⏳ 次回、製品判断が必要 | — |
| **`pair::prune_stale_discoveries`/`prune_completed_challenges` の配線**: doc comment が「定期的に呼び出すこと」を前提としていたが呼び出し元ゼロだった (`update_stats` と同型)。`run_pair`/`run_earn` から呼ぶよう配線 | ✅ 改良済 | `76f32e3` |
| **`refresh_expired_attestations`/`process_deadman`/`check_idle_streams` の配線可否を個別調査**: 前者はTEEルーティング安全性に無関係と判明 (`freshest_verified_instance`が独自に鮮度チェック) のため見送り、後2者は操作対象データを作るCLI経路自体が無いため見送り | ✅ 判定済 (見送り、根拠記録) | `ca59ca3` |
| **`docs/SURPLUS_AND_GAPS.md` 新設**: 過剰/不足の判断を機械可読形式で集約、後続AIエージェント向け | ✅ 改良済 | `72afd51` |
| **`SessionManager`/`EcashManager`/`PairManager` への status doc comment 追加**: 「テスト済みだが到達不能」な理由をコード自身に明記 | ✅ 改良済 | `e7bd2b9`, `54e7265`, `84512a9` |
| **`docs/RESEARCH_UPDATE_2026-07.md` 新設**: WebSearchで1ヶ月分の差分調査。TensorCommitments/Iroh 1.0等の新規発見 | ✅ 改良済 | `75aa636` |
| **`docs/P2P_IMPLEMENTATION_READINESS.md`/`CASHU_BDHKE_IMPLEMENTATION_READINESS.md` 新設**: 最大2ギャップの実装手順書を事前準備。BDHKEはCashu公式仕様(NUT-00)を直接検証 | ✅ 改良済 | `e79dde8`, `6c0d408` |
| **⚠️ 環境の重大な制約発見**: このセッションのコンテナは新規作成で依存クレートキャッシュが空、かつネットワークポリシーが `static.crates.io`/`crates.io` を 403 でブロック — `cargo build` が既存依存関係すら取得できず失敗する状態を確認 (`$HTTPS_PROXY/__agentproxy/status` で `policy denial` 確認)。「未検証」表記の変更は全てこの制約下で手動レビューのみ実施。ビルド可能な環境での再検証が必須。ユーザーが環境作成時のネットワークポリシーを見直せば解消できる可能性が高い (本セッション終盤でも再確認したが変化なし) | 🔴 要対応 | — |
| **Workflow (8エージェント) による敵対的レビューで実バグを発見・修正**: `perform_attestation` の `update_stats()` 丸ごと呼び出しが `stats.active_sessions` を黙って0に巻き戻す回帰。単独の手動レビューでは見逃していたが、複数エージェントによる独立検証で確認。`active_tee_instances` のみを直接再計算するよう修正、回帰テスト追加。**本セッションの単独レビューだけでは見逃す欠陥が実在することを示す実例** — 未検証コミット全体への信頼度を下げる材料 | ✅ 改良済 | `66383b9` |

### ソクラテス式問答 Round 5 — ecash.rs の 3 欠陥

**問⑪「SpentNullifiers の capacity を超えたとき何が起きるか？」**

旧実装は FIFO eviction: 100,001 件目の nullifier 記録時に最古が集合から除去された。
攻撃者は 100,000 件の異なるトランザクションで window を埋め、evicted nullifier を再提示し
二重使用を成立させられた (sliding window double-spend attack)。

修正: eviction を廃止。容量到達時は `record` が `false` を返し `overflow=true` フラグを立てる。
`spend_proofs` は `false` を受け取るとトランザクションを中断 (`anyhow::bail!`)。
古い nullifier は集合に残り続け、二重使用検出は degradation しない。

**問⑫「`rfind("==")` は何を返すか？」**

条件文字列 `"label==value1==value2"` に対し `rfind("==")` は最後の `==` (index 14) を返し、
expected = `"value2"` (末尾のみ) となる。正しくは `"value1==value2"` (最初の `==` 以降全体)。
hex-encoded blake3 ハッシュは `=` を含まないため実害は少ないが、
expected 値自体に `==` が混入した場合 (base64 等) は誤判定を招く。

修正: `rfind` → `find` (最初の `==` をセパレータとし、右辺全体を expected とみなす)。

**問⑬「streaming の elapsed_sec は誰が保証するか？」**

`tick_stream` は `Utc::now()` (呼び出し元のローカルクロック) で elapsed を計測する。
tick 呼び出しを怠ると次回 tick 時に巨大な elapsed が発生し、1 回で大量の sats を消費する
(例: 5 分放置 + rate=10 sat/sec → 3,000 sat を 1 tick で消費)。
`remaining` で総額上限は守られるが、細粒度の決済制御が崩れる。

修正: `EcashConfig::max_tick_interval_seconds` (既定 60 秒) を追加し、
`elapsed_sec = raw_elapsed.min(max_tick_interval_seconds)` で 1 tick の最大消費を制限。

**テスト追加 (+3 件):**
- `test_spent_nullifiers_overflow_rejects_not_evicts` (旧 FIFO test を置換)
- `test_spend_proofs_bails_on_nullifier_overflow`
- `test_proof_satisfies_multi_eq_uses_first_separator`
- `test_tick_stream_elapsed_capped_by_max_interval`

テスト 208 (default) / clippy・fmt 全クリーン。

### ソクラテス式問答 Round 6 — session.rs の「完成して見える」安全プリミティブ

着眼: `session.rs` には**定義だけで未結線**の primitive が 2 つあり、どちらも
「見た目は完成」だが結線した瞬間に効く潜在バグを抱えていた（要件 vs 証明 / フェイルセーフ）。

**問⑭「`panic_stop` は docker が無いとき何をするか？」**

緊急停止は安全最優先の経路なのに、旧実装は `docker ps` の失敗で `?` 早期 return し、
コンテナ kill もセッション掃除もせず終わっていた（**最も脆い経路が最も重要だった**）。
さらに末尾で `cleanup()` を呼び、その中の `reset_stop_flag()` が緊急停止フラグを
即座にリセット → ポーリング中の監視ループが停止シグナルを取りこぼす。

修正:
- docker 段を best-effort 化（`?` 廃止、失敗は warn して続行）し、必ず掃除まで到達。
- `cleanup()` を呼ばず `clear_session_files()`（停止フラグに触れない純掃除）を新設して共有。
  フラグは立てたまま残し、緊急停止が確実に観測される。

**問⑮「`Capability` トークンは誰が検証するか？」**

`Capability`（gpu / vram / runtime / expires / nonce / signature の権限制御トークン）は
構造体として完備だが**検証関数が一切なく、どこからも validate されない死んだ primitive**だった。
結線すれば「ピアが自己申告した任意の権限」「失効済みトークン」が無検査で通る。

修正（ed25519、依存追加なし）:
- `sign()` — 発行者署名（payload は signature 以外の全フィールドを順序保証 JSON 配列で正規化）
- `verify_signature()` — `verify_strict` で改ざん・別発行者・非正規署名を拒否
- `is_expired()` / `authorizes()` — 失効判定と「要求 ≤ 付与上限」の境界チェック
- `validate()` — 署名 ∧ 未失効 ∧ 上限内の結合ゲート（セッション開始前に通す想定）

**テスト追加 (+7 件):** capability 署名 roundtrip / 別 issuer 拒否 / 改ざん検出 /
不正署名形式 / 失効境界 / 上限境界 / 結合ゲート。すべて決定的鍵で hermetic（FS/docker 不使用）。

テスト 215 (default) / clippy・fmt 全クリーン。

### ソクラテス式問答 Round 7 — 実 I/O 経路の「リモートを無検証で信頼する」欠陥

着眼: `cashu_mint.rs` (唯一の実 HTTP I/O) と `config.rs` (鍵・ロックの実 FS I/O) は
外部 (mint / 他プロセス) の応答をそのまま信頼していた。

**問⑲「mint が返した額面 (`sig.amount`) は誰が検証するか？」**

`translate_signatures_to_proofs` は mint 応答の `sig.amount` をそのまま Proof に格納し、
要求額面との一致も keyset id の一致も検証していなかった。
悪意ある/バグった mint が額面をすり替え (例: 8 → 64 sat) たり別 keyset で署名したものを
ウォレットが受理し、残高完全性が壊れる。

修正: `expected_amounts: &[u64]` を引数に追加し、位置ごとに `sig.amount == expected` と
`sig.id == keyset_id` を検証。不一致は `Err`。blinded message の B' は額面を commit している
ので、戻り値での再検証は意味的に正しい。

**問⑳「lock 保持プロセスが取得待ちの途中で死んだら？」**

`LockGuard::acquire` の stale 判定は `attempt == 0` のみだった。保持プロセスが取得待ちの
~2 秒間にクラッシュすると stale が永久に検出されず、全タイムアウトを待って
「別プロセスが保持中」で失敗する (liveness バグ)。

修正: 毎試行で `try_reclaim_stale` を呼び dead holder を即奪取。削除前に PID を再読込し
最初に観測した dead PID と一致する場合のみ削除して、二重奪取レースの窓を狭めた
(生存ロックは保護)。

**テスト追加 (+5 件):** 額面すり替え拒否 / 別 keyset 拒否 / expected_amounts 件数不一致 /
dead holder 奪取 + live holder 保護 / 壊れたロック奪取。

テスト 225 (default) / 231 (http) / clippy・fmt 全クリーン。

## 次の一手 (推奨順)

1. **NAT 越えの実結線** — 製品が LAN を出るための必須条件（libp2p DCUtR + relay）。
   - 現状は他人 GPU を "同一 LAN" に限定、製品の中核価値を実現できない
   - libp2p を依存に追加して DHT+ホールパンチング+relay を統合
2. **検証本体** — `proof_satisfies` を VeriLLM 風 確率的再実行 / TOPLOC LSH へ拡張。
   - escrow gate はコミット一致のみ、実計算検証がない
   - resolver と抜き打ち再実行で ~1% コスト検証フローを実装
3. **永続化** — 実 ecash 結線前に bearer token durability を用意。
   - ウォレット/nullifier/escrow のメモリ状態をファイル WAL へ
</content>

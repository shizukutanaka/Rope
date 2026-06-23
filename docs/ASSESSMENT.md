# 現段階の評価 — 長所 / 短所 / 改善点

> 評価日: 2026-06-23 / 対象: Rope (branch `claude/deepresearch-ultrathink-improve-wnYtn`)
> 関連: [`RESEARCH_IMPROVEMENTS.md`](RESEARCH_IMPROVEMENTS.md)（優先度バックログ）,
> [`CATEGORY_RESEARCH.md`](CATEGORY_RESEARCH.md)（10カテゴリ調査）

## 長所 (Strengths)

1. **設計の極端な集中** — 4 動詞 / 7 core モジュール。main.rs 574 行。
   競合機能の交点 (他人GPU × GPU TEE × ecash少額決済) に wedge を絞れている。
2. **状態機械が明快** — `ecash` / `pair` / `confidential` / `intent` が型と状態遷移で
   表現され、純ロジックとして単体テスト可能。I/O と分離されている。
3. **品質ゲートが厳格** — `clippy -D warnings` / `fmt --check` / `unsafe_code = deny` を
   CI 級に強制。テスト 225 (default) / 231 (http)、全 4 動詞 graceful exit。
4. **依存最小主義** — HTTP は opt-in feature、edition2024 を避けた上限固定で
   ビルド再現性を確保。
5. **正直なフォールバック設計** — TEE 非対応や検証なしを「できるフリ」せず、
   明示的に拒否/注記する方向へ転換中（本セッションの改良）。

## 短所 (Weaknesses)

1. **実 I/O が未結線** — `pair` の Noise/mDNS/DHT、`ecash` の BDHKE 暗号、
   `confidential` の NRAS 検証はモデルのみ。唯一の実 I/O は `http` の Cashu mint。
   → 現状は「出荷可能なスケルトン」であり、エンドツーエンドでは動かない。
2. **NAT 越えが無い** — 「他人の GPU」は別 NAT 背後が普通だが、ホールパンチング/relay 未実装。
   実質 LAN デモ止まり（最大の機能的欠落）。
3. **検証 (proof-of-execution) が薄い** — `VerificationLevel` は enum 中心。
   再実行/LSH コミット等の実機構は未実装（本セッションで escrow ゲートのみ前進）。
4. **永続化なし** — ウォレット/escrow はメモリ状態。クラッシュで bearer token を失う
   金銭損失リスク（実 ecash 結線時に「高」へ昇格）。
5. **Sybil/評判の根拠が薄い** — `trust_store` は TOFU のみ。初回ピア選択の信頼基盤なし。
6. **単一ピア前提** — 大規模モデルの複数ピア pipeline 分割なし。

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
| **mint 額面検証 + lock liveness (問⑲⑳)** | ✅ **本コミット** | (this) |
| NAT 越え (libp2p DCUtR) / Noise 実結線 | ⏳ 大 | — |
| 検証本体 (VeriLLM 風 再実行 / TOPLOC LSH) | ⏳ 大 | — |
| 永続化 (ecash 状態の WAL/crash recovery) | ⏳ 中 | — |
| `RegionConstraint` 配線 (プロバイダ地域メタ必要) | ⏳ 保留 | — |

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

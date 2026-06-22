# 現段階の評価 — 長所 / 短所 / 改善点

> 評価日: 2026-06-05 / 対象: Rope (branch `claude/deepresearch-ultrathink-improve-wnYtn`)
> 関連: [`RESEARCH_IMPROVEMENTS.md`](RESEARCH_IMPROVEMENTS.md)（優先度バックログ）,
> [`CATEGORY_RESEARCH.md`](CATEGORY_RESEARCH.md)（10カテゴリ調査）

## 長所 (Strengths)

1. **設計の極端な集中** — 4 動詞 / 7 core モジュール。main.rs 574 行。
   競合機能の交点 (他人GPU × GPU TEE × ecash少額決済) に wedge を絞れている。
2. **状態機械が明快** — `ecash` / `pair` / `confidential` / `intent` が型と状態遷移で
   表現され、純ロジックとして単体テスト可能。I/O と分離されている。
3. **品質ゲートが厳格** — `clippy -D warnings` / `fmt --check` / `unsafe_code = deny` を
   CI 級に強制。テスト 185 (default) / 191 (http)、全 4 動詞 graceful exit。
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
| **escrow 解放に検証ゲート (#1×#5, free-riding 防止)** | ✅ **本コミット** | (this) |
| NAT 越え (libp2p DCUtR) / Noise 実結線 | ⏳ 大 | — |
| 検証本体 (VeriLLM 風 再実行 / TOPLOC LSH) | ⏳ 大 | — |
| 永続化 (ecash 状態の WAL/crash recovery) | ⏳ 中 | — |
| Sybil 耐性 (proof-of-capability + 評判スコア) | ⏳ 中 | — |
| `RegionConstraint` 配線 (プロバイダ地域メタ必要) | ⏳ 保留 | — |

### 本セッションの改良の要点 (escrow 検証ゲート)

`release_escrow` は従来 **任意の非空 proof で資金解放**していた（手抜き計算でも支払い =
free-riding）。本コミットで `completion_proof` が `completion_condition` を満たすことを
要求する検証ゲート `proof_satisfies` を追加:

- 条件が `"...==<commit>"` 形式なら、`proof == commit` か `blake3(proof) == commit`
  （値そのもの / プリイメージ提出の両対応）でのみ解放。
- 空証拠は常に拒否。自由形式条件は非空証拠で従来互換。

これは研究で最重要とした **検証 × ecash escrow** の結節点を、最小の純ロジックで前進させる。
テスト 185/191、clippy・fmt 全クリーン。

## 次の一手 (推奨順)

1. **NAT 越えの実結線** — 製品が LAN を出るための必須条件（libp2p DCUtR + relay）。
2. **検証本体** — `proof_satisfies` を VeriLLM 風 確率的再実行 / TOPLOC LSH へ拡張。
3. **永続化** — 実 ecash 結線前に bearer token durability を用意。
</content>

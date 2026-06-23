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
| escrow 解放に検証ゲート (#1×#5, free-riding 防止) | ✅ 改良済 | `af421e9` |
| **Sybil 耐性 (proof-of-capability + 評判スコア)** | ✅ **本コミット** | (this) |
| NAT 越え (libp2p DCUtR) / Noise 実結線 | ⏳ 大 | — |
| 検証本体 (VeriLLM 風 再実行 / TOPLOC LSH) | ⏳ 大 | — |
| 永続化 (ecash 状態の WAL/crash recovery) | ⏳ 中 | — |
| `RegionConstraint` 配線 (プロバイダ地域メタ必要) | ⏳ 保留 | — |

### 本セッションの改良の要点 (Proof-of-Capability Sybil 耐性)

初回ピア（Unknown trust level）に対する Sybil 攻撃リスクに対応。FORTYTWO 論文の
compute-anchored 能力証明を実装:

**実装内容:**
- `CapabilityChallenge` struct: チャレンジ ID / task spec / expected output hash / state machine
- PairManager メソッド群:
  - `issue_capability_challenge()` — 初回ピアに task を発行 (nonce + timeout で一意性確保)
  - `verify_capability_proof()` — 出力ハッシュ一致で capability_proven に昇格
  - `reputation_score()` — successful/failed ratio で 0.0-1.0 スコア計算
  - `is_capability_proven_or_trusted()` — proven OR (Familiar|Trusted|OwnDevice) の判定述語
- TrustedIdentity に `capability_proven` / `challenged_at` / `successful_jobs` / `failed_jobs` を追加

**信頼昇格 (Trust Ladder):**
```
Unknown (TOFU) → [challenge issued] → Proven (output hash match) or Failed
Proven → [resolver checks] → Familiar (1+ successful jobs)
Familiar → [user explicit] → Trusted
```

resolver はこれを後続で ピア選択の入力に利用可。トークン不要の Sybil 耐性確立。

**ソクラテス式深掘りでの修正 (問④⑤):** 初版コーパスの固定答えは compute コストゼロ・
全 ID 共有可能で、"compute-anchored" の核心を満たしていなかった。per-identity proof-of-work
`blake3^difficulty(nonce ‖ peer_pubkey)` を導入し、事前計算・ID 間共有・無コストの 3 つを同時に
封じた。`issue_pow_challenge` / `compute_pow_answer` + 回帰テスト 7 件。

テスト 203/209、 clippy・fmt 全クリーン。

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

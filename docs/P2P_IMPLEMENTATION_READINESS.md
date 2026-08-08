# P2P 実装レディネス — ネットワークアクセス復旧後にすぐ実行するための runbook

> 作成: 2026-07-10。対象読者: `cargo build` が復旧した (=このリポジトリ
> 環境のネットワークポリシーが `static.crates.io` を許可した) 後、
> 最初に P2P 実結線に着手するセッション (人間 or AI エージェント)。
>
> 目的: `docs/SURPLUS_AND_GAPS.md` §1.2「実P2P I/Oが無い」は現時点で
> Rope の最大の構造的ギャップ。ここに書いてあることは全て
> **今のセッションでは実行できない** (ビルド不能、`docs/SURPLUS_AND_GAPS.md`
> §0 参照) — だが「何を、どの順で、どのファイルに」実装するかを
> 事前に確定しておけば、ネットワークが復旧した瞬間から実装に入れる。
> 本ドキュメントの記述はすべて事前調査 (コードの実読 + `docs/
> RESEARCH_UPDATE_2026-07.md` の Web 調査) に基づく設計提案であり、
> **実装・コンパイル未検証**。

---

## 0. まず決めること: libp2p か Iroh か

> **2026-08-08 更新** ([`RESEARCH_UPDATE_2026-08.md`](RESEARCH_UPDATE_2026-08.md) §3):
> **Iroh v1.0.0 が 2026-06 に正式リリース済**であることを確認 (本文書作成時は
> リリース状況が未確定だった)。NAT ホールパンチング成功率は Tailscale 由来の
> 手法により **libp2p の ~70% を上回る**とされ、リレー fallback も内蔵。
> さらに **`libp2p-iroh`** (iroh QUIC を libp2p transport として使う crate) が
> 存在するため、**libp2p のプロトコル資産を保ちつつ transport だけ Iroh に
> する段階移行**という第三の選択肢がある。下の比較表と併せて判断すること。

`docs/IMPROVEMENT_SYNTHESIS.md` Part 1 は libp2p 前提で実装コードまで
用意済み。`docs/RESEARCH_UPDATE_2026-07.md` §4 は 2026年6月にリリースされた
Iroh 1.0 を対抗馬として発見した。**この選択が全ての後続作業を左右するため、
実装着手前に必ず結論を出す。**

| 判断軸 | libp2p | Iroh 1.0 |
|---|---|---|
| 依存数 | 約50 (tcp/quic/noise/identify/autonat/dcutr/relay/yamux/kademlia) | 1クレート (`iroh`) — QUIC/NAT越え/暗号化が内蔵 |
| Noise 実装 | 自前で `snow` crate を組合せ (`docs/RESEARCH_IMPROVEMENTS.md` #5) | Iroh 内蔵 (要: `pair.rs` の独自 Noise 状態機械と Iroh 内蔵実装の関係を精査) |
| mDNS/Bluetooth 発見との統合 | libp2p は mDNS discovery を公式サポート、Bluetooth は別途 (`btleplug`) | **調査済 (2026-08-08)** — mDNS 発見は **`MdnsDiscovery` (旧 `LocalSwarmDiscovery`) としてデフォルト有効**、`swarm-discovery` crate 基盤で **インターネット/リレー/DNS 不要**。DHT 相当は pkarr で mainline DHT に署名済 DNS パケットを publish。**Bluetooth は非対応**。詳細と 4 経路の対応表: [`RESEARCH_UPDATE_2026-08.md`](RESEARCH_UPDATE_2026-08.md) §4d |
| symmetric NAT 耐性の実績 | Ethereum2.0 Lighthouse実績: 85-95% (EI NAT) / 5-30% (symmetric) | Iroh側の発信情報のみ (「libp2pの上限70%を上回る」) — **中立的ベンチマーク未確認、要検証** |
| Rope の設計原則との整合 | 実績・エコシステムの厚みで有利 | 「最小依存主義」(README 設計原則#3) との整合で有利 |
| Rust edition/MSRV 制約 | `Cargo.toml` の `rust-version = "1.75"` と `edition2024` 回避方針との互換性要確認 | 同上、Iroh側も要確認 |

**推奨アクション (着手時点で必ず実施)**: 両方を最小構成 (discovery のみ) で
`cargo add` してみて、依存木の edition2024 汚染 (このリポジトリが繰り返し
踏んできた問題、`Cargo.toml` コメント参照) が起きないほうを選ぶ。
どちらも汚染する場合は依存バージョンを個別に固定して回避を試みる
(`base64ct`/`getrandom`/`cpufeatures` で使った手法と同じ)。

---

## 1. 実装順序 (段階的ロールアウト、リスクの低い順)

`pair.rs` は既に完全な状態機械として実装・テスト済み。やることは
「状態遷移の入力を、テストのモックではなく実ネットワークイベントにする」
差し替えのみ — 状態機械自体の再設計は不要。

### Step 1: mDNS 発見のみ実結線 (最もリスクが低い、LANのみ、NAT越え不要)

- 新規ファイル `src/net/discovery.rs` (`docs/ARCHITECTURE.md` の図に
  「v0.3予定」として既に記載されている場所)。
- 実装内容: mDNS で `_rope._tcp.local` (`PairConfig.mdns_service` の値、
  `pair.rs` に既存フィールドあり) をアドバタイズ/リッスンし、応答を
  `pair::PairManager::record_discovery()` (`pair.rs:615`, 既存の公開API、
  signature 変更不要) に渡す。
- `main.rs::run_pair` (`src/main.rs:256-305`) の変更点: 現在
  `capability_boundary!` で早期returnしている箇所を、実際に
  `discovery.rs` のリスナーを起動するコードに置き換える。
  ここが「シミュレーションから実動作への切替点」。
- **検証**: 2台の実機 (または同一LAN上の2コンテナ) で `rope pair` を
  同時実行し、互いを `discovered` に登録できることを確認。

### Step 2: Noise XX 実鍵交換 (`pair.rs` の既存状態機械に実クリプトを接続)

- `pair.rs:198-218` の `NoisePattern`/`HandshakeState` は既に完全な
  状態遷移を定義済み — 追加すべきは各遷移における**実際の鍵交換演算**のみ。
- `begin_handshake`(`pair.rs:770`)/`advance_handshake`(`pair.rs:819`)/
  `complete_handshake`(`pair.rs:854`) の中身を、選定したクレート
  (`snow` または Iroh 内蔵) の Noise XX ハンドシェイクAPIに接続する。
  関数シグネチャ自体は変更不要 — 内部実装のみ差し替え。
- `Cargo.toml:32` のコメント「x25519-dalek, aes-gcm: v0.3のNoise protocol
  実装時に復活予定」を実行する箇所。ただし `snow` 採用時は `snow` が
  x25519/AEADを内包するため、これらを個別依存として追加する必要は
  無い可能性が高い (要確認)。
- **検証**: `examples/library_usage.rs::example_pair_flow()` は既に
  3-message Noise XX の状態遷移を模擬している (`begin_handshake` →
  `advance_handshake` ×3 → `complete_handshake`) — これを実ネットワーク
  越しの2プロセス間で再現できれば成功。

### Step 3: NAT越え (libp2p選定時: AutoNAT+DCUtR+Relay / Iroh選定時: 内蔵機構の検証)

- `docs/IMPROVEMENT_SYNTHESIS.md` Part 1 に libp2p 選定時の設定コード
  スニペット (AutoNAT/DCUtR/Relay v2 の Config) が既に用意されている。
- Iroh 選定時は「内蔵NAT越えが本当に有効か」を実機 (できれば symmetric
  NAT環境、例: 携帯回線テザリング) で検証する必要がある。

### Step 4: proof-of-capability (`pair.rs`) の実結線

- `issue_capability_challenge`/`verify_capability_proof`/
  `reputation_score` は既にテスト済み (`docs/SURPLUS_AND_GAPS.md` §2.2 参照:
  「テストからのみ呼ばれ、実際の CLI 動詞からは到達しない」)。
  Step 1-3 で実ピアが発見・接続できるようになった後、初回接続時に
  これらを呼ぶよう `main.rs::run_pair` に追加する。ロジック変更は不要、
  呼び出し追加のみ。

---

## 2. 影響範囲マップ (変更が必要なファイル)

| ファイル | 変更内容 | リスク |
|---|---|---|
| `Cargo.toml` | libp2p or iroh (+関連feature) を `[dependencies]` に追加。`http` feature と同様、新規 `p2p` feature でデフォルト無効にすることを推奨 (`core/` の「I/Oなしpure state machine」原則を維持するため) | 低 (追加のみ、既存依存に影響なし) |
| `src/net/discovery.rs` (新規) | mDNS/Bluetooth/DHT 発見の実装、`pair::record_discovery` への橋渡し | 中 (新規コード) |
| `src/net/noise.rs` (新規、`ARCHITECTURE.md` に既に記載の予定名) | Noise ハンドシェイクの実装、`pair.rs` の状態機械への橋渡し | 中 |
| `src/core/pair.rs` | **状態機械自体は変更しない** — `begin_handshake` 等の内部で `net::noise` を呼ぶよう差し替えるのみ | 低 (公開APIシグネチャ不変) |
| `src/main.rs::run_pair`/`run_earn` | `capability_boundary!` の早期returnを実処理に置き換え | 中 (動作が変わる箇所) |
| `src/lib.rs` | `pub mod net;` は既存、新規サブモジュール追加のみ | 低 |

**変更しない (安心材料)**: `pair.rs` の型定義・状態遷移ロジック・
既存テスト236件超は無傷のまま。今回のセッションで追加した
`prune_stale_discoveries`/`prune_completed_challenges` の配線
(`main.rs:275-283,424-431`) もそのまま活きる。

---

## 3. 注意点 (このセッションの調査で判明した罠)

- `unsafe_code = "deny"` が crate 全体に強制されている (`Cargo.toml:89`)。
  libp2p/snow/iroh のいずれも内部で `unsafe` を使うが、それは各クレート
  内部の話であり Rope 側のコードが `unsafe` を書くわけではないので
  問題ない ── ただし FFI ベースの依存 (もしあれば) は要注意。
- `edition2024` 回避のための上限バージョン指定パターン (`base64ct`,
  `getrandom`, `cpufeatures` の例、`Cargo.toml` 参照) を新規依存にも
  適用する必要が生じる可能性が高い。1.75 MSRV との衝突は過去に
  何度も発生しているので、`cargo add` 直後に必ず `cargo build` で
  即座に確認すること。
- QRペアリング (`pair.rs` の blake3 keyed-HMAC 部分) は既に本物の暗号
  なので**変更不要**。Step 1-3 は mDNS/BT/DHT の3経路のみが対象。
- `main.rs::run_pair`/`run_earn` を変更すると `capability_boundary!` の
  「working/next」表示が実態と合わなくなるため、同時に更新すること
  (虚偽表示防止 — このリポジトリの一貫した設計原則)。

---

## 4. 完了の定義 (Definition of Done)

- [ ] 2実機間で `rope pair` → 実ピア発見 → 実Noise握手 → `paired` へ登録、まで
      一連が動作する
- [ ] 異なるLAN (NAT越え必要) 間でも同様に動作する、または Symmetric NAT
      環境での成功率を計測しドキュメント化する
- [ ] 既存236件超のテストが全てPASSしたまま (`cargo test --all-targets`)
- [ ] `cargo clippy --all-targets -- -D warnings` / `cargo fmt --all -- --check` green
- [ ] `capability_boundary!` の表示を実態に合わせて更新
      (「working: 実mDNS発見+実Noise握手」等)
- [ ] `SECURITY.md`の「まだ暗号学的に機能していないもの」表からP2P関連行を削除
- [ ] `docs/SURPLUS_AND_GAPS.md` §1.2 を「[DONE]」に更新

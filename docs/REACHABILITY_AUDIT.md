# `core/` 到達可能性監査 (2026-07-06)

`src/lib.rs`/`src/main.rs` の `#![allow(dead_code)]` は「core は完全な状態機械 API を
公開するが、現行の 4 動詞からは一部のみ到達する。残りは v0.3 予約 API」と正当化して
いる。この主張の正確性を、コードを一切変更せずに検証した (Explore agent によるグレップ
ベースの呼び出しチェーン追跡)。

**結論**: 主張は方向性としては正しいが、粗すぎる。`#![allow(dead_code)]` は 3 種類の
異なる状態を一括で隠してしまっている:

| 分類 | 件数 (161 pub fn 中) | 意味 |
|---|---:|---|
| (a) 4動詞から実際に到達 | 68 (42%) | 本当に使われているコード |
| (b) テストからのみ呼ばれる | 64 (40%) | 未結線だがテストで検証済み — 「予約 API」として妥当 |
| (c) **どこからも一切呼ばれない** (テストも含め) | **15 (9%)** | 予約 API と呼ぶには根拠が弱い — 削除候補 |
| (d) (c)を呼ぶだけの関数 (デッドコードの連鎖) | 14 (9%) | 実質的に (c) と同じ扱いをすべき |

(c)+(d) = 29 関数 (18%) が「テストの裏付けすら無い」削除候補。

## 実行結果 (追記, 同日フォローアップ)

初稿では「今回は削除しない」としたが、その後 15件 (category c) を1つずつ
手動で再検証し (自分の grep で呼び出し元ゼロを再確認、周辺コードを読んで
trait 実装や設計意図の有無を確認)、リスクが低いと判断できたものだけ実施した:

- **削除した (4件)**: `config::load_private_key`/`load_config`/
  `ensure_initialized` (単純にジェネリック `load_or_recover<T>` に置き換わって
  取り残された旧ユーティリティ、インポート依存無し), `intent::get_plan`
  (自明な一行アクセサ、ロードマップ記載無し)
- **削除ではなく配線した (1件)**: `confidential::update_stats` —
  `format_confidential` が実際に表示する `stats.active_tee_instances` を
  再計算する唯一の関数だったが、誰からも呼ばれず値が常に0固定だった
  (`TeeStatus::Running` 未代入バグ (v0.2.11) と同根の「計算はあるが結線が無い」
  パターン)。`perform_attestation` から呼ぶよう配線し、回帰テストを追加
- **判断を保留し保持した (2件)**: `intent::with_region` (RegionConstraint
  フィールドの唯一のセッター — 削除すると v0.3 で再度必要になる),
  `confidential::TeeType::is_cpu_tee` (arXiv:2507.02770 準拠の composite
  attestation 設計を明示的に説明するコメント付き、単体では未使用でも
  設計意図が明確)
- **`session.rs` の10件クラスターは今回一切触れなかった** — 個別に読んだ結果、
  `panic_stop` は「docker ps 失敗で早期returnし緊急停止全体を諦めていた」
  過去のバグ修正を経た安全性クリティカルなコードであり、`SessionManager`
  も明示的な設計コメント (v0.3 async化の可能性) を伴う。「呼び出し元ゼロ」
  だけでは削除の十分条件にならないと判断 — 製品判断 (このセッション管理
  レイヤーを v0.3 でどう使うか) が必要であり、コード監査だけで決めるべきでは
  ない

## 第3回フォローアップ — Workflow による削除の敵対的再検証

コンパイラ検証ができない制約下で削除した4関数 (`load_private_key`/
`load_config`/`ensure_initialized`/`get_plan`) について、8エージェント
構成の Workflow (独立検索4件 + 敵対的検証4件) で「呼び出し元が本当に
ゼロか」を再検証した。各検証エージェントは元の grep 結果を鵜呑みにせず、
`pub use` 再エクスポート・trait 経由の間接呼び出し・マクロ展開・
doctest・削除前のコミット (`a87d456^`) での状態など、grep だけでは
見落としうる経路を個別に洗い直した。

**結果: 4件全て CONFIRMED (独立検証でも削除前から呼び出し元ゼロと確認)**。
特に `load_private_key` については「削除によって `Capability::sign` 等の
実運用フローが切断されていないか」まで遡って確認し、そもそも署名フロー
自体が本番未結線であることまで独立に裏付けている。

コンパイラ検証が使えない状況でも、複数の独立したエージェントによる
異なる角度からの grep/構造解析を組み合わせることで、単一の grep 実行
よりも高い確信度が得られることを示す一例。ただし **これは cargo check
の代替にはならない** — ビルド可能な環境での最終確認は依然として必須。

## 第2回フォローアップ — category (b) の「呼ぶべきだが呼ばれていない」パターン捜索

`update_stats` の修正パターン (doc comment は「定期的に呼ぶ」ことを前提として
いるが実際の呼び出し元が無い) が他にも無いか、category (b) の中から
prune/cleanup/refresh 系の関数を洗い出して個別に判断した:

- **配線した**: `pair::prune_stale_discoveries`/`prune_completed_challenges`
  (`run_pair`/`run_earn` の両方に追加。詳細は CHANGELOG 参照)
- **配線を見送った**: `confidential::refresh_expired_attestations` —
  一見 `run_inference` の TEE ルーティング判断前に呼ぶべきに見えたが、実際の
  ルーティング関数 `freshest_verified_instance()` は `attestation_status`
  フィールドに頼らず `attestation_is_fresh()` で独自に鮮度を都度チェックして
  おり、安全性はこの関数の有無に依存しない。配線するには `run_inference` に
  新規の `save_confidential()` 呼び出しを追加する必要があり (現状は読込のみ)、
  リスクの割に得られる価値が表示上の鮮度反映のみと低いため見送り
- **配線を見送った**: `ecash::process_deadman`/`check_idle_streams` —
  escrow/stream 自体がどの CLI 動詞からも作成されない (ecash の CLI 結線が
  丸ごと存在しない) ため、呼び出しを追加しても操作対象データが常に空。
  `pair.rs` の prune 関数とは異なり、対応する「アクティブに状態を作る」動詞が
  無いため自然な呼び出し箇所が無い

**結論**: category (c)/(d) というグレップベースの分類は出発点として有用だが、
機械的に全削除すべきではない。今回のように、各関数を個別に読み、
「単純に陳腐化した重複」か「設計意図のある未結線コード」かを人間の判断で
区別する必要がある。この判断ロジック自体を以下に記録し、次回の判断基準とする。

## 実行方針 (初稿時点、上記フォローアップ前の記述)

コンパイラ検証ができない環境 (`cargo build` が `static.crates.io` への
403 でそもそも失敗する — 詳細は CHANGELOG [Unreleased] 参照) での作業だったため、
1つずつ最大限慎重に判断した。以下は元の一括分類表 (削除実施状況は上記参照)。

## category (c) — どこからも呼ばれない (15件、最優先削除候補)

| モジュール | 関数 | 根拠 |
|---|---|---|
| config.rs | `load_private_key` | 自身の定義以外に一致箇所ゼロ |
| config.rs | `load_config` | 一致箇所ゼロ |
| config.rs | `ensure_initialized` | 一致箇所ゼロ |
| intent.rs | `with_region` | 一致箇所ゼロ (builder メソッドだが誰も連鎖呼び出ししていない) |
| intent.rs | `get_plan` | 一致箇所ゼロ |
| confidential.rs | `TeeType::is_cpu_tee` | 一致箇所ゼロ |
| confidential.rs | `update_stats` | doc comment 内で言及されるのみ、実呼び出しゼロ (v0.2.13 で `SessionStatus` の doc comment に記した「未結線」問題の根本原因そのもの) |
| session.rs | `panic_stop` | 一致箇所ゼロ (他関数の doc comment で言及されるのみ) |
| session.rs | `is_stop_requested` | 一致箇所ゼロ |
| session.rs | `get_status` | 一致箇所ゼロ |
| session.rs | `cleanup` | 一致箇所ゼロ (doc comment に「cleanup() は呼ばない」と明記あり) |
| session.rs | `list_sessions` | 一致箇所ゼロ |
| session.rs | `format_session_list` | 一致箇所ゼロ |
| session.rs | `Session::load` | 一致箇所ゼロ |
| session.rs | `SessionManager::end` | 一致箇所ゼロ (テストも `new`/`current`/`start`/`elapsed_seconds` のみ検証) |

## category (d) — (c) を呼ぶだけ (14件)

| モジュール | 関数 | 唯一の呼び出し元 |
|---|---|---|
| session.rs | `Session::delete` | `SessionManager::end` (category c) |
| session.rs | `reset_stop_flag` | `cleanup` (category c) |
| confidential.rs | `TeeStatus::icon` | `format_confidential` (test-only) |
| confidential.rs | `AttestationStatus::icon` | `format_confidential` (test-only) |
| confidential.rs | `verified_tee_count` | `format_confidential` (test-only) + 直接テスト |
| ecash.rs | `SpentNullifiers::contains` | `receive_proofs` (test-only) |
| pair.rs | `PairingToken::decode` | `accept_pairing_token` (test-only) |
| pair.rs | `verify_hmac` | `accept_pairing_token` (test-only) |
| pair.rs | `PairingToken::is_expired` | `accept_pairing_token` (test-only) |
| pair.rs | `CapabilityChallenge::is_valid` | `is_pending`/`verify_capability_proof` (共に test-only) |
| pair.rs | `compute_pow_answer` | `issue_pow_challenge` (test-only) + 直接テスト |
| pair.rs | `ChallengeCorpus::for_model` | `challenge_for_peer` (test-only) + 直接テスト |
| pair.rs | `record_discovery` | `accept_pairing_token` (test-only) + 直接テスト |
| pair.rs | `reputation_score` | `recommend_for_job` (test-only) + 直接テスト |

**注意**: `pair.rs`/`ecash.rs` の (d) 項目の多くは「テストからは直接呼ばれている」
ため、(c) ほど確信度は高くない — これらのモジュール全体が `pair`/`run`/`earn`
動詞の結線待ち (v0.3) という前提と矛盾しない。**最優先で見るべきは session.rs の
10件** (category c の8件 + category d の2件、`Session::delete`/`reset_stop_flag`) —
`SessionManager`・panic/cleanup 系配線はテストからも一切呼ばれておらず、
「v0.3 予約 API」ではなく現行 4 動詞設計以前の残骸である可能性が高い。

## モジュール別サマリ

| モジュール | pub fn 数 | (a) 到達 | (b) test-only | (c) 呼出ゼロ | (d) 連鎖デッド |
|---|---:|---:|---:|---:|---:|
| config.rs | 18 | 14 | 1 | 3 | 0 |
| pair.rs | 33 | 4 | 21 | 0 | 8 |
| session.rs | 25 | 5 | 10 | **8** | **2** |
| confidential.rs | 17 | 6 | 6 | 2 | 3 |
| intent.rs | 21 | 14 | 5 | 2 | 0 |
| ecash.rs | 24 | 3 | 20 | 0 | 1 |
| first_run.rs | 23 | 22 | 1 | 0 | 0 |

`first_run.rs` はほぼ完全に到達可能 (実オーケストレーション層そのものであるため
当然)。`pair.rs`/`ecash.rs` は「予約 API、テスト済み」の健全なパターン。
`session.rs` が突出した外れ値。

## 副次的発見: `format_*` 関数の大半が未結線

`format_pair` のみが `main.rs` から実際に呼ばれる。`format_confidential`/
`format_ecash`/`format_plan`/`format_session`/`format_session_list`/`format_config`
の 5 件はテストはあるが、どの動詞からも出力されない。将来的にこれらを
diagnostics サブコマンド (例: `rope status --verbose`) として結線するか、
削除するかは別途判断が必要。

## 次回実施手順 (ビルド可能な環境向け)

1. `cargo build`/`cargo test --all-targets` が通ることを確認
2. category (c) の 15 関数から着手 (最も確信度が高い)。1つ削除するごとに
   `cargo check --all-targets` を実行し、想定外の参照が無いことを確認
3. category (d) の 14 関数は、対応する (c) 関数の削除と同時に削除可能
   (呼び出し元ごと消えるため)
4. 全削除後、`src/lib.rs`/`src/main.rs` の `#![allow(dead_code)]` を一時的に
   コメントアウトし `cargo build --all-targets` を実行 — 新たな dead_code
   警告が出なければ、残った (a)/(b) は真に「予約 API」と確定する
5. 警告が出た場合は category (b) の中に実は誰にも呼ばれていない関数が
   紛れていた可能性がある (今回のグレップベース調査の限界: マクロ展開や
   trait dispatch 経由の呼び出しは見落とす場合がある) — 個別に再確認

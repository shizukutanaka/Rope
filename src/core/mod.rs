//! Rope Core — ROPE_2028 7 モジュール構成
//!
//! 状態 (2026-04-18): 122 → 7 モジュール (-94%)
//! 全モジュール配線済 (dead code ゼロ)
//!
//! ## 4 動詞 (main.rs) と core モジュール対応
//!
//! ```text
//! rope            (無引数)  → first_run + pair + ecash + confidential + intent
//! rope pair                 → pair
//! rope run <model>          → intent + confidential
//! rope earn                 → pair + ecash
//! ```
//!
//! ## 7 モジュールの役割
//!
//! - `config`        — 鍵生成、設定パス、storage 基盤 (27 deps)
//! - `pair`          — mDNS+Noise 発見、TOFU 信頼ストア (10 deps)
//! - `confidential`  — GPU TEE attestation、LANDMINES 2 対応 (2 deps)
//! - `ecash`         — Cashu bearer token + escrow + streaming (2 deps)
//! - `intent`        — Workload/Privacy/Budget の上位抽象 (2 deps)
//! - `session`       — 1:1 GPU 貸借セッション状態 (2 deps)
//! - `first_run`     — 60秒 wow moment オーケストレータ (1 dep)
//!
//! ## 削除されたもの (RETIREMENT_MANIFEST 完了)
//!
//! 「とりあえず後で使うかも」の死蔵在庫を含む 115 モジュールを削除。
//! Apple 流: 約束されてない機能のためにスロットを残さない。
//! 必要になったら lib から import すればよい。

pub mod confidential;
pub mod config;
pub mod ecash;
pub mod first_run;
pub mod intent;
pub mod pair;
pub mod session;

/// 表示用に文字列を先頭 `n` 文字へ安全に切り詰める (UTF-8 境界を割らない)。
///
/// `&s[..n]` のバイトスライスはマルチバイト境界に当たると panic する。
/// ID は通常 ASCII だが、ピア名等に任意文字列が入りうるため共通化して防御する。
pub fn short(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// 1 BTC = 100,000,000 sats (定義値)。
pub const SATS_PER_BTC: f64 = 100_000_000.0;

/// sats → USD 換算に用いる BTC 価格 (USD/BTC) の暫定固定概算値。
///
/// **暫定値**: 本来は価格フィードから動的取得すべき。現状 ecash 決済自体が
/// どの動詞からも実行されない (`docs/SURPLUS_AND_GAPS.md` §2.3) ため、この
/// 概算値は予算表示の単位変換に使われるだけで実害は無い。v0.3 で実 ecash を
/// 結線する際は必ず動的な価格ソースへ差し替えること
/// (`docs/SURPLUS_AND_GAPS.md` §2.5 参照)。
pub const APPROX_USD_PER_BTC: f64 = 50_000.0;

/// sats を USD へ概算換算する (予算の単位変換専用)。
///
/// `intent::Intent::with_budget` は USD を取るが、CLI (`rope run --budget`) と
/// 初回デモは予算を sats で指定するため、その境界でのみ用いる。価格は
/// `APPROX_USD_PER_BTC` の固定概算 (上記の注意を参照)。従来 `main.rs` と
/// `first_run.rs` に同一のマジックナンバー式が重複していたのを 1 箇所へ集約した
/// — v0.3 の実価格フィード導入時に触るべき箇所がここ 1 点になる。
pub fn sats_to_usd(sats: u64) -> f64 {
    sats as f64 / SATS_PER_BTC * APPROX_USD_PER_BTC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_respects_utf8_boundaries() {
        // マルチバイト文字でも panic せず char 単位で切り詰める
        assert_eq!(short("あいうえお", 3), "あいう");
        assert_eq!(short("abcdef", 3), "abc");
        // n が長さ以上なら全体を返す
        assert_eq!(short("ab", 5), "ab");
    }

    #[test]
    fn test_sats_to_usd_conversion() {
        // 1000 sats = 0.00001 BTC × $50,000 = $0.50
        assert!((sats_to_usd(1_000) - 0.5).abs() < 1e-9);
        // デモ既定 100 sats = $0.05 (README の「約 $0.0005」は別基準の記載だが
        // ここでは APPROX_USD_PER_BTC=50,000 に基づく換算値を固定する)
        assert!((sats_to_usd(100) - 0.05).abs() < 1e-9);
        // 0 sats は $0
        assert!(sats_to_usd(0).abs() < 1e-9);
        // 1 BTC 相当 = $50,000
        assert!((sats_to_usd(100_000_000) - 50_000.0).abs() < 1e-6);
    }
}

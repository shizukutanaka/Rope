//! Rope core API 使用例 — 実行可能・型検査済み
//!
//! core/ は pure state machine (I/O なし)。テスト・組み込み・外部ツール統合に
//! そのまま使える。net/ は HTTP/wire 変換層 (`--features http` で有効)。
//!
//! 実行: `cargo run --example library_usage`

use rope::core::confidential::{ConfidentialManager, SecurityLevel, TeeType};
use rope::core::ecash::{EcashManager, Mint, MintTrust};
use rope::core::pair::{DiscoveryMethod, PairManager, PeerCapabilities};
use std::net::SocketAddr;

/// ピア発見 → Noise 握手 (state machine のみ、実ネットワーク I/O は v0.3 で結線)
fn example_pair_flow() {
    let mut mgr = PairManager::default();
    let addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();
    let caps = PeerCapabilities {
        gpu_count: 1,
        gpu_model: "H100".to_string(),
        vram_gb: 80,
        accepts_jobs: true,
        accepts_payment: true,
        payment_address: None,
        protocol_version: "1.0".to_string(),
        tee_capable: true,
        tee_attested: false,
    };

    let peer = mgr
        .record_discovery("Bob's H100", "pk-bob", addr, DiscoveryMethod::Mdns, caps)
        .expect("discovery 記録失敗");
    let peer_id = peer.id.clone();

    // 3-message Noise XX handshake state machine
    let session_id = mgr
        .begin_handshake(&peer_id)
        .expect("handshake 開始失敗")
        .id;
    mgr.advance_handshake(&session_id).unwrap(); // → EphemeralSent
    mgr.advance_handshake(&session_id).unwrap(); // → ResponderIdentified
    mgr.advance_handshake(&session_id).unwrap(); // → Established

    mgr.complete_handshake(&session_id, "pk-bob", "key-hash-demo")
        .expect("handshake 完了失敗");

    assert_eq!(mgr.paired.len(), 1, "ペア済みピアが1件登録されるはず");
    println!(
        "✓ pair: 発見 → 握手 → ペア完了 ({} peers)",
        mgr.paired.len()
    );
}

/// ecash escrow flow (Vast.ai「突然切断」問題の解決策)
fn example_ecash_flow() {
    let mut mgr = EcashManager::default();
    mgr.add_mint(Mint {
        id: "mint-1".to_string(),
        url: "https://mint.example.com".to_string(),
        pubkey: "pk-mint1".to_string(),
        supported_denominations_sats: vec![1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024],
        lightning_supported: true,
        onchain_supported: false,
        trust_level: MintTrust::Unknown,
        total_issued_sats: 0,
        total_redeemed_sats: 0,
        last_seen: chrono::Utc::now(),
        verified_keyset_at: None,
    })
    .unwrap();
    mgr.trust_mint("mint-1", MintTrust::Trusted).unwrap();

    let proofs = mgr.mint_tokens("mint-1", 1000).unwrap();
    assert_eq!(proofs[0].secret.len(), 64); // 32 byte hex
    assert_eq!(proofs[0].c.len(), 66); // compressed secp256k1 point (hex)
    assert_eq!(mgr.wallet.total_sats, 1000);

    // escrow (deadman 自動返金付き)
    let e = mgr
        .open_escrow("job-1", "alice", "bob", "mint-1", 500, "hash==deadbeef")
        .unwrap();
    mgr.mark_escrow_in_progress(&e.id).unwrap();
    mgr.release_escrow(&e.id, "deadbeef").unwrap(); // 条件一致 → 解放

    // streaming (1 sat/秒)
    let s = mgr
        .open_stream("job-2", "alice", "bob", "mint-1", 100, 1)
        .unwrap();
    mgr.close_stream(&s.id).unwrap(); // 未使用分は自動返金

    println!(
        "✓ ecash: mint 1000 sats → escrow 500 (解放済) → 残高 {} sats",
        mgr.wallet.total_sats
    );
}

/// TEE attestation flow (Petals の「機微データが平文で見える」問題への回答)
fn example_confidential_flow() {
    let mut mgr = ConfidentialManager::default();

    // H100 (TEE 対応) → attestation 成功
    let h100 = mgr.create_tee_instance(
        "gpu-h100",
        "H100",
        TeeType::NvidiaGpuTee,
        SecurityLevel::High,
    );
    let report = mgr.perform_attestation(&h100.id).unwrap();
    assert!(
        report.verification_result.success,
        "H100 は attestation を通るはず"
    );

    // RTX 4090 (消費者 GPU、TEE 非対応) → attestation 拒否
    let rtx = mgr.create_tee_instance(
        "gpu-rtx",
        "RTX 4090",
        TeeType::NvidiaGpuTee,
        SecurityLevel::High,
    );
    let report2 = mgr.perform_attestation(&rtx.id).unwrap();
    assert!(
        !report2.verification_result.success,
        "RTX 4090 は TEE 非対応のため attestation が失敗するはず"
    );

    println!(
        "✓ confidential: H100 attestation={}, RTX4090 attestation={}",
        report.verification_result.success, report2.verification_result.success
    );
}

fn main() {
    println!("Rope core API 実行例\n");
    example_pair_flow();
    example_ecash_flow();
    example_confidential_flow();
    println!("\n全例が正常終了。詳細は src/core/*.rs を参照。");
}

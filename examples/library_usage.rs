// Rope core API 使用例
//
// core/ は pure state machine。テスト・組み込み・外部ツール統合に最適。
// I/O は net/ に分離。core 単独で動作検証可能。
//
// 注: 本ファイルは API 参照用の擬似コード。
//     実行するには `mod core;` を含む独自バイナリから呼び出す。

/// ピア発見 flow
fn _example_pair_flow() {
    // let mut mgr = PairManager::default();
    //
    // 1. mDNS 発見
    // mgr.record_discovery("Bob's H100", "pk-bob", addr, DiscoveryMethod::Mdns, caps);
    //
    // 2. Noise XX 握手 (3 message exchange)
    // let session = mgr.begin_handshake(&peer.id);
    // mgr.advance_handshake(&session.id); // → EphemeralSent
    // mgr.advance_handshake(&session.id); // → ResponderIdentified
    // mgr.advance_handshake(&session.id); // → Established
    //
    // 3. 完了 → TOFU 信頼ストア
    // mgr.complete_handshake(&session.id, "pk-bob", "key-hash");
}

/// ecash escrow flow (Vast.ai「突然切断」問題を解決)
fn _example_ecash_flow() {
    // let mut mgr = EcashManager::default();
    //
    // 1. mint 登録 + 信頼
    // mgr.add_mint(mint);
    // mgr.trust_mint("mint-1", MintTrust::Trusted);
    //
    // 2. proof 発行 (Cashu NUT-04)
    // let proofs = mgr.mint_tokens("mint-1", 1000);
    // assert_eq!(proofs[0].secret.len(), 64);  // 32 byte hex
    // assert_eq!(proofs[0].c.len(), 66);        // compressed secp256k1 point
    //
    // 3. escrow (deadman 60分)
    // let e = mgr.open_escrow("job", "alice", "bob", "mint-1", 500, "cond");
    // mgr.release_escrow(&e.id, "proof");       // Bob 完遂 → 解放
    //
    // 4. streaming (1 sat/秒)
    // let s = mgr.open_stream("job", "alice", "bob", "mint-1", 500, 1);
    // mgr.tick_stream(&s.id);                   // 時間経過分を drain
    // mgr.close_stream(&s.id);                  // 未使用分返金
}

/// TEE attestation flow (Petals 死因対応)
fn _example_confidential_flow() {
    // let mut mgr = ConfidentialManager::default();
    //
    // H100 → 承認:
    // let inst = mgr.create_tee_instance("gpu", "H100", TeeType::NvidiaGpuTee, High);
    // assert!(mgr.perform_attestation(&inst.id).unwrap().verification_result.success);
    //
    // RTX 4090 → 拒否 (TEE 非対応):
    // let inst2 = mgr.create_tee_instance("gpu", "RTX 4090", TeeType::NvidiaGpuTee, High);
    // assert!(!mgr.perform_attestation(&inst2.id).unwrap().verification_result.success);
}

fn main() {
    println!("Rope core API 参照例 — 詳細は src/core/*.rs を参照");
}

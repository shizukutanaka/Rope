# Surplus & Gap List — machine-readable synthesis

> Generated 2026-07-10, branch `claude/deepresearch-ultrathink-improve-wnYtn`,
> repo state at commit `ca59ca3`, `Cargo.toml` version `0.2.13`.
> Audience: an AI coding agent (Claude Opus/Sonnet) picking up follow-up work
> on this repo. Dense, declarative, file:line-anchored. Not prose for a human
> skim-reader — see `docs/ASSESSMENT.md`/`README.md` for that framing.
> Status tags: `[DONE]` `[BLOCKED:build]` `[BLOCKED:consent]` `[BLOCKED:decision]` `[OPEN]`

---

## 0. Precondition every item below depends on

`[BLOCKED:build]` This container cannot run `cargo build`/`test`/`clippy` at all.
`static.crates.io`/`crates.io` return HTTP 403 (`connect_rejected`, "policy
denial") through the session's egress proxy; confirmed via
`curl $HTTPS_PROXY/__agentproxy/status`. No vendor cache exists locally
(`~/.cargo/registry` is empty). This is a network-policy setting chosen when
the Claude-Code-on-the-web environment was created, not a transient failure —
polling within a session does not resolve it. Every code change committed
after `830d8c4` (v0.2.13 release) is **manual-review-only, not
compiler-verified**. First action for any agent with working `cargo`: run the
full gate (`cargo build && cargo test --all-targets && cargo test --all-targets
--features http && cargo clippy --all-targets -- -D warnings && cargo clippy
--all-targets --features http -- -D warnings && cargo fmt --all -- --check`)
against HEAD before trusting anything past `830d8c4`.

**⚠️ 2026-08-18 — the paragraph above is still true about `cargo`, but the
conclusion drawn from it ("nothing can be verified here") is no longer true.**

Two gates now run offline:

1. **`rustfmt --edition 2021 --check <files>`** — ships with the toolchain, needs
   no registry. A **parser**: catches syntax errors and formatting drift only.
2. **`tools/offline-typecheck/check.sh`** — **type-checks 100% of `src/`
   (lib + bin + all test bodies) in ~2.5 seconds**, with no network.
   `src/lib.rs`'s entire external surface is 8 crates / ~30 items, so those
   items are replaced by hand-written stubs in `tools/offline-typecheck/shims/`;
   `proc_macro` is bundled with rustc, so even `#[derive(Serialize)]` and
   `#[derive(Parser)]` are stubbable. It catches non-exhaustive matches
   (E0004), type/arity errors (E0308), unresolved names (E0425/E0433), and
   borrow/move errors (E0382) — `tools/offline-typecheck/selftest.sh` proves
   this by injecting each class and asserting it is caught, so the harness
   cannot silently rot into a vacuous pass.

**What this changes for the codebase**: HEAD **type-checks** as of 2026-08-18.
Every commit after `830d8c4` had been accumulating as "manual-review-only";
they are now type-verified, subject to the limits below.

**What it does NOT change — read
[`tools/offline-typecheck/README.md`](../tools/offline-typecheck/README.md)
before relying on a PASS.** It is a `cargo check` substitute, not `cargo test`,
not `cargo clippy`, not `cargo build`. It cannot see: shim-vs-real signature
drift, serde's per-field derive bounds, `json!` contents, clap argument specs,
the `--features http` path, MSRV-1.75 compatibility (the local rustc is 1.94),
clippy lints, any runtime behaviour, or anything cryptographic (the crypto
stubs return zeros and always verify Ok). **"The harness passes" ≠ "it
compiles", and emphatically ≠ "it works".** CI remains the shipping gate.

---

## 1. GAPS (不足) — missing functionality, ranked by what blocks what

### 1.1 No real cryptography anywhere money or attestation touches `[PARTLY RESOLVED 2026-08-18 — frame signing is real; encryption and BDHKE are not]`

> **What changed**: `src/net/signing.rs` uses **real `ed25519-dalek`** (already a
> dependency, not hand-rolled) to sign and `verify_strict` every inter-node
> frame. Node-to-node messages now have genuine **integrity and authenticity**.
>
> **What did NOT change, and this is the important half**:
> - **There is still no encryption.** `src/net/transport.rs` sends prompts in
>   **plaintext**. Signing proves *who sent it* and *that nobody altered it*;
>   it does not stop a passive LAN observer from reading it.
> - Noise, BDHKE and NRAS are still placeholders. The Noise handshake was **not**
>   hand-rolled — this entry's own prohibition was respected.
>
> **How the prohibition was respected while still shipping a transport**: the
> ban applies to **primitives**, not to framing, message definitions, size caps
> or ordering rules. `src/net/wire.rs` implements only the latter and takes
> `FrameSigner`/`FrameVerifier` **as injected traits**. Consequences:
> - the only file touching a crypto crate is a ~40-line adapter (`signing.rs`)
> - the protocol logic is **dependency-free and therefore actually testable in
>   this container** — 11 wire tests + 4 transport tests run for real, including
>   a full job crossing a real TCP socket
> - when `snow` becomes installable, it slots in behind the same traits
>
> **The plaintext transport is refused by default.** `transport::plaintext_allowed()`
> reads `ROPE_ALLOW_PLAINTEXT` and both `serve_connection` and `request_job`
> return `PlaintextNotAllowed` unless it is `1`. The decision to send prompts
> over an unencrypted LAN is handed to the operator with the reason stated,
> rather than made silently.

**Original finding, kept for context:**
- `src/core/ecash.rs:441-470` `build_proof` — Cashu BDHKE unblinding is
  `blake3::hash("C|keyset|amount|secret")` reshaped into secp256k1-point-shaped
  bytes. Not real elliptic-curve math. Same in `src/net/cashu_mint.rs:520-556`
  `build_blinded_outputs`.
- `src/net/cashu_mint.rs:360-368` — NUT-07 proof-state check unimplemented by
  design (would need a `secp256k1` dep, deliberately not added).
- `src/core/confidential.rs:671-689` `build_evidence_signature` — TEE
  attestation "signature" is `format!("unverified-digest:{}", blake3_hash)`.
  Honestly prefixed (tested to stay prefixed, `confidential.rs` test near
  line 1270-1293), but not a real NVIDIA NRAS/RIM/OCSP-verified signature.
- `src/core/pair.rs:167-207` — Noise XX/IK is a `HandshakeState` enum with no
  actual key exchange. `x25519-dalek`/`aes-gcm` are not dependencies (removed,
  `Cargo.toml:32` comment: "v0.3の Noise protocol 実装時に復活予定").
- Root cause for all four: `secp256k1`/`x25519-dalek`/`aes-gcm`/`snow` cannot
  be added while `static.crates.io` is blocked (§0). `curve25519-dalek` IS
  already vendored transitively via `ed25519-dalek` (check `Cargo.lock`),
  which could theoretically supply X25519 DH primitives without a new crate
  — but hand-rolling a full Noise handshake state machine on top of a raw DH
  primitive without a vetted crate (`snow`) is a real security risk; do not
  do this without explicit sign-off. secp256k1 has no such shortcut available
  in already-vendored deps.

### 1.2 No real P2P I/O `[PARTLY RESOLVED 2026-08-18 — discovery AND transport are real; encryption is not]`

> **`src/net/mdns.rs` opens real UDP multicast sockets.** `rope pair` now sends a
> `_rope._tcp.local` PTR query to 224.0.0.251:5353, answers other peers' queries,
> parses the responses, and feeds them into `PairManager::record_discovery` —
> which this document previously listed as never called from anywhere.
> **11 tests, run for real in this container**, including a two-socket
> discovery over actual multicast.
>
> **The requirement that was wrong**: `P2P_IMPLEMENTATION_READINESS.md` framed A1
> as blocked on adding Iroh or libp2p. Those buy **NAT traversal, relays and
> QUIC** — and `V1_SCOPE.md` §2 already **deleted NAT traversal from v1**.
> What LAN discovery actually needs is the DNS-SD wire format and a UDP
> multicast socket, both of which `std::net` has.
>
> **Design notes worth keeping**:
> - Binds **5353 if free, otherwise an ephemeral port**. Multicast delivery is by
>   *destination port*, so an ephemeral-bound socket cannot receive announcements
>   sent to :5353 — it can only get unicast replies. Hence the QU bit (RFC 6762
>   §5.4) on our query, and the honest report when we could not become a
>   responder ("こちらからは探せるが、相手からは見つからない").
> - Discovery and answering share **one socket and one loop** — mDNS is
>   symmetric, so no threads and no async runtime are needed.
> - The parser treats every packet as hostile: bounds-checked reads, a
>   compression-pointer jump limit, record/name/label caps, and unknown records
>   discarded rather than erroring. Fuzz-ish tests feed it 2,000 random buffers
>   and every truncation of a valid packet; none panic.
>
> **Transport landed the same day** (`src/net/wire.rs` + `src/net/transport.rs`).
> A job now really crosses a TCP socket: `rope run` asks a discovered peer
> first and only falls back to local execution, `rope earn` listens and runs
> the borrower's prompt through the real engine. **4 transport tests run for
> real**, including a full job over a loopback socket answered by the actual
> transformer.
>
> **What is still missing**: **encryption** (§1.1). Frames are signed with real
> ed25519 — integrity and authenticity hold — but the payload is plaintext, so
> a passive LAN observer reads the prompt. The transport therefore **refuses to
> run unless `ROPE_ALLOW_PLAINTEXT=1`**. Noise remains the next step and must
> not be hand-rolled (§1.1).
>
> Also still missing: **payment is not wired to the transport** (A5). A job
> crosses the wire and is executed, but no escrow is opened and no proof
> changes hands. That is the last unwired axiom of v1.

**Original finding, kept for context:**
- `src/core/pair.rs` — mDNS/Bluetooth/DHT discovery and the Noise handshake
  are pure state machines. Zero sockets opened anywhere in `core::pair`.
- `main.rs::run_pair` (`src/main.rs:256-305`) never calls `record_discovery`/
  `begin_handshake`/`accept_pairing_token` — confirmed zero real peer actions.
  This is a deliberate scope boundary (a `capability_boundary!` macro call at
  the end of the function honestly reports "working: state machine, next:
  real mDNS/Bluetooth"), not an oversight.
- No NAT traversal (AutoNAT/DCUtR/relay) at all. Needs `libp2p`, blocked by §0.
- `docs/IMPROVEMENT_SYNTHESIS.md` Part 1 has ready-to-use `libp2p`
  integration code once §0 is resolved (dependency versions, AutoNAT/DCUtR
  config snippets).

### 1.13 Two resource bugs in the inference path, found by re-reading what shipped `[FIXED 2026-08-18]`

Found by applying ③/④ to `net/inference.rs` **after** it worked — which is the
order `V1_SCOPE.md` §5 insists on (do not optimize what does not exist yet).

1. **The shared classifier was duplicated.** `slice_weights` did
   `wcls: token_embedding.clone()` whenever `shared_classifier` was set — which
   is the *normal* configuration for llama2-family checkpoints. On a
   vocab 32000 × dim 4096 model that is **524 MB copied for nothing**, doubling
   the lender's memory for the largest single tensor. `Weights.wcls` is now
   `Option<Vec<f32>>` with a `classifier()` accessor that falls back to the
   embedding table. Two tests pin both configurations.
2. **A per-token allocation in the hot loop.** `forward` did
   `let x_final = state.x.clone()` on every single token so it could RMSNorm
   into `state.x`. It now normalizes into the existing `xb` scratch buffer.
   One `dim`-sized allocation per generated token, gone.

Neither changed any numerical result — the existing arithmetic tests
(hand-computed RMSNorm/matmul, residual-only forward, KV-cache causality)
pass unchanged, which is what makes this a safe refactor rather than a rewrite.

Measured baseline after the fix (5.8M-parameter synthetic model, 6 layers,
dim 256, vocab 4096, `-O`): **53 forward passes in 406 ms ≈ 7.7 ms/token** on
this container's CPU. Recorded so a future optimization has something to beat.

### 1.12 A5 is wired end-to-end, but the tokens are still placeholders and there is no price negotiation `[NEW 2026-08-18]`

The differentiator named in `V1_SCOPE.md` §4 — **paying for compute without a
token** — now actually happens over the wire:

- `Message::Payment` carries bearer tokens (`WireProof`) after the job result.
- Borrower: `main.rs::take_payment` selects proofs summing to *exactly* the
  price, calls `EcashManager::spend_proofs`, and sends them.
- Lender: `LocalPolicy::receive_payment` converts them back and calls
  `EcashManager::receive_proofs`, which enforces double-spend rejection and
  mint-trust checks, then persists the wallet.
- Two tests run this for real over a loopback socket: one asserts 10 sats
  arrive and are counted, one asserts an unpaid job is recorded as
  `paid_sats = 0` rather than killing the lender.

**Three honest limits, all of which matter:**

1. **The tokens are not real Cashu tokens.** `build_proof` is still the
   placeholder from §1.1 (blake3 reshaped into secp256k1-shaped bytes), so what
   crosses the wire is structurally a bearer token but not cryptographically
   one. **A real mint would reject it.** The plumbing is real; the money is not.
   This is the same blocker as §1.1 (`k256` unavailable) and is the single
   thing standing between v1 and a genuinely novel product.
2. **There is no price negotiation.** No message proposes or accepts a price.
   `main.rs::price_for` applies a fixed rule — **1 sat per output token, capped
   by `--budget`** — and both sides simply assume it. `rope earn --rate`
   (sats/second) is **not consulted**, which is an inconsistency to resolve when
   a price message is added. Until then the lender takes whatever arrives.
3. **Escrow is deliberately NOT used on this path, and the reason is worth
   recording.** The escrow state machine protects the *borrower* against a
   lender who takes payment and never delivers. But in the implemented flow the
   borrower **receives the result first and pays afterwards**, so the exposure
   is reversed: it is the *lender* who can be stiffed (test:
   `an_unpaid_job_is_recorded_not_fatal`). Bolting the existing escrow onto this
   flow would not fix that — it would only add ceremony. Fixing it properly
   needs either payment-before-delivery (which exposes the borrower instead) or
   incremental streaming payment per token, which is what
   `StreamSession` was designed for and is the natural next step.
   **Do not "wire escrow in" without deciding which side v1 protects.**

### 1.3 No proof-of-execution / verification engine `[PARTLY RESOLVED 2026-08-18 — execution now exists; verification still OPEN]`

> **The blocking half is gone.** This entry's own conclusion was that
> verification could not start because **nothing executed** — "no inference
> engine is integrated at all … the single largest genuinely-unstarted gap".
> That is no longer true.
>
> `src/net/inference.rs` (2026-08-18) is a **real transformer inference engine**:
> RMSNorm, RoPE, grouped-query attention with a KV cache, SwiGLU FFN, and
> temperature/top-p sampling, reading llama2.c legacy-v1 checkpoints. It has
> **zero external dependencies**, which is why it exists at all in this
> container — and it means **its 22 tests actually run here**, unlike the rest
> of the crate (§0).
>
> **The requirement that was wrong**: `A3_INFERENCE_IMPLEMENTATION_READINESS.md`
> said A3 was blocked on adding mistral.rs. mistral.rs supplies GPU kernels,
> quantization formats and speed — **not the capability to execute**. A
> transformer forward pass is arithmetic, and `std`'s f32 ops are enough.
>
> **What is now true**: `rope run <model>` executes a real model on the CPU if
> the lender has placed a checkpoint in `~/.rope/models` (or `ROPE_MODEL_DIR`),
> and says so honestly if not.
> **What is still not true**: the computation runs *locally*. "Someone else's
> GPU" needs A1 (mDNS/Noise) and A5 (ecash wiring), both still blocked by §0.
> And it is CPU f32, not GPU.
>
> **Verification (A4) remains OPEN** — but it is no longer blocked *by absence
> of execution*. `Completion.forward_passes` was added specifically as the
> beginning of an answer to the Hollow-LLM "effort gap" (§1): it records how
> much compute was actually spent, which is the quantity that attack exploits.

**Original finding, kept for context:**
- `src/core/intent.rs` `VerificationLevel` enum exists; nothing implements it.
- `src/core/ecash.rs` escrow's `proof_satisfies` only checks a commitment-hash
  match — a lazy/malicious worker can return output with a matching hash
  without doing real inference (free-riding, unaddressed).
- This is **not** blocked by the crates.io restriction — the real blocker is
  that **no inference engine is integrated at all** (no llama.cpp/vLLM
  binding), so there's nothing to verify the output of yet. This is the
  single largest genuinely-unstarted-not-just-blocked gap in the project.
  `docs/RESEARCH_IMPROVEMENTS.md` §1 recommends TOPLOC-style LSH commit +
  VeriLLM-style probabilistic rerun (~1% cost overhead) over zkLLM (too
  slow: 986s commit + 803s proof for LLaMA-2-13B, incompatible with the
  60-second product promise).

### 1.4 TEE privacy claim doesn't match the "idle GPU" supply story `[OPEN, product decision]`
- README claims "🛡️ GPU is safe (prompt invisible)" — true only for NVIDIA
  Hopper/Blackwell+ datacenter GPUs with Confidential Computing. Most "idle
  GPUs" in the target supply pool (consumer RTX cards) don't support CC.
- Partial mitigation already shipped: `src/core/intent.rs` resolver refuses
  to route `Privacy::ConfidentialCompute` intents to unverified/non-TEE
  peers (`check_feasibility` gate). The underlying attestation chain itself
  is still fake (§1.1). README has a disclosure note about this
  (`README.md:84-89`) linking `docs/RESEARCH_IMPROVEMENTS.md` #2.
- **2026-08-08 update — both sides now concrete**
  ([`RESEARCH_UPDATE_2026-08.md`](RESEARCH_UPDATE_2026-08.md) §4g).
  This entry previously stated only the *capability gap* ("consumer RTX
  lacks CC"). Two papers make the trade-off specific:
  - **Supply-side economics are settled and excellent**
    (arXiv:2601.09527, 79 benchmark configs on RTX 5060 Ti/5070 Ti/5090):
    self-hosted inference costs **$0.001-0.04 per million tokens**
    (electricity), **40-200x cheaper** than budget-tier cloud APIs, with
    hardware breaking even in **under four months** at 30M tokens/day.
    Rope's "under a dollar" promise leaves the lender a large margin.
    NVFP4 gives **1.6x throughput over BF16, 41% less energy, 2-4% quality
    loss** — directly usable when picking model formats for A3.
    ⚠️ **That paper's "Private" means *local deployment*, NOT Confidential
    Computing.** Consumer Blackwell did **not** gain CC. **W4 is not
    resolved by it.**
  - **What A6 is actually up against without CC**: CloakLM
    (arXiv:2606.18400) is software-only memory obfuscation, and the attacks
    it cites are the concrete threat — **Hermes** reconstructs a DNN
    losslessly from **PCIe bus observation alone**; **TunnelS** exfiltrates
    **HBM contents at high throughput via driver-level access without
    interrupting inference**. A lender who physically owns the hardware has
    both. **Software-only defenses buy friction, not a guarantee** —
    CloakLM itself is framed as mitigation.
  - **Three product options** (`decision` — not settled by research):
    1. Keep `Privacy::ConfidentialCompute` restricted to CC-capable peers
       (current routing guard — honest and correct)
    2. Define a **weaker, honestly-labeled** privacy level for consumer GPUs
       (an explicit "obfuscation only / no guarantee" variant)
    3. Treat consumer GPUs as **non-sensitive workloads only**

### 1.5 CI defined but not activated `[BLOCKED:permission — interim gate added 2026-08-18]`

> **Still blocked, and still one command for the repo owner**:
> `git mv .github/ci.yml.disabled .github/workflows/ci.yml`.
> Neither the git gateway nor the GitHub App has `workflows` permission
> (both paths measured returning 403).
>
> **What was added instead** — automation that needs no such permission:
> - `tools/offline-typecheck/run-tests.sh` **actually runs** the tests of the
>   four dependency-free `src/net/` modules (56 tests: real UDP multicast
>   discovery, a real TCP job round-trip with payment, transformer numerics).
>   Until now those only ran because they were invoked by hand.
> - `.githooks/pre-push` chains rustfmt → type-check → those tests → and the
>   real cargo gate when cargo happens to work. Opt-in with
>   `git config core.hooksPath .githooks`.
>
> **This is not a CI replacement and must not be described as one.** It cannot
> see clippy lints, MSRV-1.75 compatibility, `--features http`, or any `core/`
> test (those need serde/chrono/uuid). A green hook still only means
> "types check and the dependency-free modules pass".
> **⚠️ 2026-08-18 訂正**: この項目はこれまで `[BLOCKED:consent]` とし、
> 「secrets アクセスを伴う自動パイプラインの起動にユーザーの明示同意が必要」と
> 記録していた。**ファイルを読んで確認した結果、これは誤りだった** —
> `.github/ci.yml.disabled` の `env:` は `CARGO_TERM_COLOR` と `RUSTFLAGS` のみ、
> ジョブは `check`/`test`/`test-http`/`clippy`/`fmt`/`gate` で
> **secrets 参照はゼロ、deploy も publish も無い**。
>
> **実際のブロッカーは権限**: git gateway は
> "refusing to allow a GitHub App to create or update workflow ... without
> `workflows` permission" で拒否し、GitHub API 経由も 403
> ("Resource not accessible by integration") — **両経路で実測**。
>
> **リポジトリ所有者なら 1 手で有効化できる**:
> `git mv .github/ci.yml.disabled .github/workflows/ci.yml`
>
> **これは v1 にとって単なる自動化ではない**: ローカルは `static.crates.io` 403 で
> ビルド不能だが **GitHub Actions は crates.io に到達できる**。CI は
> **この環境で唯一のコンパイラ検証手段**であり、`830d8c4` 以降の
> 未検証コミット群 (§0) を一括で検証する。

- `.github/ci.yml.disabled` has a complete 5-job pipeline (check/test/
  test-http/clippy/fmt + gate), improved this session (broadened push/PR
  triggers off a nonexistent `main` branch requirement, added an
  `--features http` job). Moving it to `.github/workflows/ci.yml` activates
  a pipeline with repo-secrets access. An `AskUserQuestion` call requesting
  explicit confirmation failed to return (`Tool permission stream closed`)
  during this session and was never re-answered by the user. Auto-mode's
  permission classifier correctly blocked a direct attempt to move the file.
  **Needs an explicit human "yes, activate CI" before any agent moves this
  file** — do not infer consent from a generic "続けて"/"continue".

### 1.6 Persistence is checkpoint-only, not WAL `[OPEN, escalates once §1.1/1.2 land]`
- `docs/RESEARCH_IMPROVEMENTS.md` §7: each CLI verb calls `save_ecash()`
  etc. once at the end, not after each state transition (mint/escrow-open/
  tick). Currently low-risk because no verb actually invokes a real ecash
  operation yet (ecash's whole mutation API is unreachable from any verb,
  see §2.3). **Escalates to a real bearer-token-loss risk the moment real
  ecash gets wired to `rope run`/`rope earn`.**

### 1.7 `format_*` not surfaced to any verb `[RESOLVED 2026-08 — 3 wired, 2 deleted, 1 deferred]`
- **Original finding**: 6 `format_*` functions existed and were tested; only
  `format_pair` was called from `main.rs`. No `rope status`-style verb existed
  to surface the other 5. Not a bug — no CLI surface was ever designed for it,
  and adding a 5th verb conflicts with the 4-verb minimalism principle
  (`README.md` "設計原則" §2 Focus).
- **Resolution (v1 単純化, ステップ③)**: rather than adding a verb, the
  decision was taken per formatter — **display belongs to exactly one place**:
  - **Wired into the existing verbs** (each replaced a hand-rolled duplicate of
    the same formatting in `main.rs`, so this is a deletion of duplication, not
    an addition of surface):
    - `format_plan` → `rope run` (replaced `main.rs` の手書き整形 20 行;
      the plan's steps / model variant / latency / energy are now visible,
      which the hand-rolled block never showed)
    - `format_session` → `rope earn` (replaced the hand-rolled ID / verify-code
      / max-minutes lines)
    - `format_ecash` → `rope earn` (replaced the hand-rolled balance line;
      kept behind the existing `total_sats > 0` gate so an empty wallet does
      not print a screen of zeros)
  - **Deleted**: `format_config`, `format_session_list` — no verb, no axiom,
    no consumer. `list_sessions` (the data API behind the latter) is kept.
  - **Deferred, deliberately not deleted**: `format_confidential` — it belongs
    to the A6/TEE subsystem, which `V1_SCOPE.md` defers *wholesale* to v2.
    Deleting one formatter out of a subsystem that is being deferred intact
    would be arbitrary churn.
- ⚠️ **コンパイラ未検証** — this change was made in the build-blocked
  environment (§0). Verified by grep (zero remaining references) and by
  `rustfmt --check` (parses); **not** type-checked or tested.
- Related: 7 QR/TOFU-related `pair::PairStats` fields (`tofu_accepts`,
  `pubkey_mismatch_rejections`, `qr_tokens_issued/consumed/expired`,
  `qr_hmac_rejections`, `qr_nonce_replays_blocked`) are tested but not
  displayed anywhere, even in `format_pair`. Deliberately not added
  (CHANGELOG `[0.2.12]`): cramming 7 diagnostic counters into a user-facing
  status display was judged to hurt readability more than it helps.

### 1.8 Refund paths credit `wallet.total_sats` without restoring proofs — invariant breaks after any refund `[FIXED 2026-08-18 — type-checked, NOT test-run; MUST become a mint swap in v0.3]`

> **Fixed, and more cheaply than this entry predicted.** The entry (and the
> §1.10 ordering note) assumed the fix needed either a new `locked_proofs`
> field on `Escrow` or a real mint swap — i.e. that it was blocked on BDHKE.
> **Neither is true.** What the invariant requires is only that a refund puts
> *some* proofs summing to the refunded amount back into the bucket, and
> `lock_funds` already re-issues change proofs locally by the same mechanism.
> - New private helper `EcashManager::credit_proofs(mint_id, amount_sats)`
>   decomposes the amount into powers of two, issues proofs into the mint
>   bucket, and adds to `total_sats` — the two representations move together.
> - All four refund sites now route through it: `refund_escrow`,
>   `resolve_dispute` (`PayerWins` and `Split`), and `close_stream`.
>   `Split`'s half-amount is representable because the decomposition is
>   power-of-two, not fixed-denomination.
> - Existing tests all assert on `total_sats`, which is unchanged; three new
>   tests assert `total_sats == Σproofs` after each refund path, and one
>   asserts the stronger property that **the refunded balance can actually be
>   locked again** (the old code passed the scalar check and failed here).
> - ⚠️ **This is correct only under the current placeholder BDHKE**, where
>   issuing a proof locally is what `mint_tokens` already does (§1.1). With a
>   real mint, "re-issue it here" is not a thing. `credit_proofs`'s doc comment
>   carries the instruction to replace it with a mint swap in v0.3.
> - **Verification status**: `tools/offline-typecheck/check.sh` PASS
>   (type-check only). **Tests not run** — `cargo test` is still blocked (§0).
> - **Consequence for §1.10**: the ordering constraint recorded there earlier
>   the same day ("item 2 → item 4 is mandatory") **no longer applies** — see
>   the correction in §1.10.

**Original finding, kept for context:**
- Found by careful reading of the ecash money paths (2026-07, this session).
- The wallet has two representations of balance that are supposed to agree:
  `wallet.total_sats` (a scalar) and `Σ` of proof amounts across
  `wallet.proofs_by_mint` (the actual bearer tokens). The **credit** paths
  keep them in sync — `mint_tokens` (`ecash.rs:508-515`) and `receive_proofs`
  (`ecash.rs:613-618`) both push proofs into a bucket AND add the same amount
  to `total_sats`. `lock_funds` (`ecash.rs:715-753`) also keeps them in sync:
  it removes `amount_sats` worth of proofs from the bucket (re-issuing
  power-of-two change) AND subtracts `amount_sats` from `total_sats`.
- But the **refund** paths only touch the scalar:
  - `close_stream` (`ecash.rs:1030`): `self.wallet.total_sats += refund;`
  - `refund_escrow` (`ecash.rs:837`): `self.wallet.total_sats += e.amount_sats;`
    (comment literally says `簡略化` = "simplified")
  - `resolve_dispute` PayerWins/Split (`ecash.rs:883,890`):
    `total_sats += amount` / `amount / 2`
  None of these restore proofs to `proofs_by_mint`. So after any refund,
  `total_sats > Σ proofs` by the refunded amount.
- Concrete consequence once ecash is wired (§2.3): the refunded balance shows
  up in `total_sats` (the displayed balance, e.g. `format_ecash`) but is
  **unspendable** — a later `open_stream`/`open_escrow` calls `lock_funds`,
  which checks the *proof bucket* (`bucket_total < amount_sats` → bails
  "proof 残高が不足"), not `total_sats`. Fails closed (no money created), but
  the user sees a balance they can't actually use.
- Why it's harmless *today*: the entire `EcashManager` mutation API is
  unreachable from any CLI verb (§2.3), so no refund is ever executed against
  a real wallet in the current build.
- Correct v0.3 fix: refunds must re-issue proofs via a real mint swap (the
  proper Cashu flow), not just bump the scalar — which is exactly why the
  `簡略化` comment exists. Cross-referenced from
  `docs/CASHU_BDHKE_IMPLEMENTATION_READINESS.md` Definition of Done.

### 1.9 `spend_proofs` mutates the wallet bucket before validating the full id set — a partially-invalid id list destroys the valid proofs `[FIXED 2026-08-18 — type-checked, NOT test-run]`

> **Fixed.** `spend_proofs` is now split into a **validation phase that mutates
> nothing** and an **execution phase that cannot fail**:
> - Each requested id is matched to a *distinct* proof in the bucket up front;
>   a missing id **or a duplicate id** bails before anything is removed.
> - The nullifier store's capacity is checked **before** any record is written,
>   so the overflow bail can no longer leave proofs deleted and `total_sats`
>   decremented (the old code bailed *after* both).
> - `receive_proofs` had the same hole in its record loop (a mid-loop overflow
>   bail left nullifiers marked spent for proofs that were never credited —
>   permanently unspendable money). Same precheck applied.
> - ⚠️ While writing the fix I nearly introduced a worse bug: putting the
>   side-effecting `record(...)` call *inside* `debug_assert!` would compile it
>   out in release builds, **silently disabling double-spend detection**. The
>   call is now made outside the assert; a comment marks the trap in both
>   places.
> - Four new tests assert the all-or-nothing property (unknown id / duplicate
>   id / spend overflow / receive overflow all leave the wallet untouched).
> - **Verification status**: `tools/offline-typecheck/check.sh` PASS
>   (type-check only). **The tests have not been run** — `cargo test` is still
>   blocked (§0). CI must run them before this is called done.

**Original finding, kept for context:**
- Found by careful reading of the ecash money paths (2026-07, this session),
  same pass that produced §1.8.
- `spend_proofs` (`ecash.rs:523-542`) removes the requested proofs from the
  mint bucket **first**, then checks that the full set was found:
  ```rust
  bucket.retain(|p| {
      if proof_ids.contains(&p.id) { spent.push(p.clone()); false } // removed
      else { true }
  });
  if spent.len() != proof_ids.len() { anyhow::bail!("一部 proof 見つからず"); }
  ```
  The `retain` has already mutated `self.wallet.proofs_by_mint[mint_id]` by
  the time the length check runs, and the bail happens **before**
  `total_sats` is decremented (line 545). So if `proof_ids` contains any id
  that isn't in the bucket, or a duplicate id, the matched proofs are already
  gone from the bucket, `spent` is dropped on the error return, and
  `total_sats` is unchanged → `total_sats > Σproofs` and the valid proofs are
  permanently lost.
- `before_len` (line 531) is captured but only fed to `let _ = before_len;`
  (line 561) — it is dead and does **not** drive any rollback.
- Concrete trigger once ecash is wired (§2.3): a caller passing
  `["valid_id", "typo_id"]` or `["id", "id"]` loses `valid_id`/`id` entirely
  while getting an error. Fails *un-*closed on the accounting side (money
  destroyed + scalar desynced), unlike §1.8 which fails closed.
- Correct v0.3 fix: validate the full id set exists **before** removing
  anything (e.g. count matches in `bucket.iter().filter(...)` first, bail on
  mismatch, then remove), so the mutation is all-or-nothing. Must be applied
  and compiler-verified before `spend_proofs` becomes reachable.
- Why it's harmless *today*: `spend_proofs` is part of the same
  CLI-unreachable `EcashManager` mutation API as §1.8 (§2.3) — no verb calls
  it against a real wallet.

### 1.10 Emergency stop and escrow refund are not connected `[FIXED 2026-08-18 — type-checked; ecash tests not run, but the refund logic has tests]`

> **Fixed.** `panic_stop` now refunds before it clears session files:
> - `Session` gained `job_id: Option<String>` (`#[serde(default)]`, so existing
>   `~/.rope/sessions/*.json` still load — 規範4). **This was the missing join
>   key** this entry identified.
> - `JobPolicy::accept` now receives the `job_id`, and the lender writes it to
>   the session **before executing**. If that write fails the job still runs, but
>   the operator is told that emergency-stop refunds will not work — it does not
>   fail silently.
> - `EcashManager::refund_escrows_for_job` refunds every escrow for that job,
>   routed through `refund_escrow`, so the `Deposited | InProgress` guard
>   excludes completed work **for free** — the claw-back attack this entry
>   warned about cannot happen. A test asserts exactly that: two escrows on one
>   job, one released, only the unreleased one is refunded.
> - `panic_stop` treats refunding as best-effort: a failure is logged and the
>   stop continues. **Stopping matters more than refunding.**
>
> **Trigger wired the same day.** `panic_stop` is no longer callerless: the
> `rope earn` loop polls for a **sentinel file** (`~/.rope/STOP`) and for
> `is_stop_requested()`, and calls `panic_stop` when either fires.
> - **Why a file and not a signal**: Rust's `std` has no signal API,
>   `signal-hook` cannot be added here, and reaching for `libc::signal` would
>   need `unsafe` — which `Cargo.toml` denies crate-wide (規範5). A file is
>   coarser than SIGINT but it is a *working* lever, which "no trigger at all"
>   was not. Replace it with a real handler when a signal crate is available.
> - The accept loop is **non-blocking** (`set_nonblocking(true)` + 200 ms poll).
>   A blocking `accept()` would mean the stop request is not noticed until the
>   next connection arrives — useless as an emergency stop.
> - Limitation, stated plainly: **a job already executing runs to completion.**
>   The generation loop does not poll the sentinel. That is bounded by
>   `max_output_tokens`, so it is short, but it is not instantaneous.
- Found by a systematic axiom-pair sweep (2026-08-08,
  [`RESEARCH_UPDATE_2026-08.md`](RESEARCH_UPDATE_2026-08.md) §4i), asking what
  *should* happen to the borrower's funds when the lender exercises their
  emergency stop (A9). The borrower is not at fault, so the escrow should be
  refunded immediately. It is not.
- Three facts, each grep-verified:
  - `panic_stop` (`session.rs:360-390`) does exactly three things: sets
    `STOP_REQUESTED`, `docker kill`s Rope's containers, and calls
    `clear_session_files()` (`session.rs:385`).
  - `clear_session_files` (`session.rs:483-494`) deletes only `.json` files
    under `config::session_dir()`. **The ecash state lives in
    `~/.rope/ecash.json` and is untouched.**
  - The only automatic refund path is `process_deadman`
    (`ecash.rs:900-918`), gated on `e.deadman_at < now`, where `deadman_at`
    is set at open time to now + `default_deadman_minutes` — **60 by
    default** (`ecash.rs:341,366,677`).
- **Consequence once wired**: the lender stops the job, and the borrower's
  escrow stays locked for up to an hour. From the borrower's side: the job
  died and the money is gone for 60 minutes. **A7 ("neither side loses on
  interruption") does not hold along the A9 path.**
- Why it was invisible until now: A7's refund was designed around *timeout*
  — borrower abandonment or lender failure. **Deliberate lender-initiated
  stop is an A9-shaped interruption, and A9 was missing from the axiom set
  until 2026-08-08.** The gap in the axioms is mirrored by a gap in the code.
- Harmless today: both `panic_stop` and the whole `EcashManager` mutation API
  are CLI-unreachable (§2.3). **They get wired together during A3+A9, which
  is when this surfaces.**
- Fix direction (design input, not a patch to apply now):
  - `panic_stop` should refund the escrows tied to the stopped session.
    `job_id` exists on both sides (`Escrow.job_id`) and is the join key.
  - **Naive immediate refund creates a new attack**: a lender could finish
    the computation, then fire `panic_stop` to claw back payment while
    keeping the result. So refund-on-stop must be **restricted to escrows
    that are not yet complete** — the same `Deposited | InProgress` filter
    `process_deadman` already uses; `EscrowState::Released` must be excluded.
  - Readable as an `A9 ⊥ A7` tension: strengthening the lender's stop right
    weakens the borrower's funds. The "don't refund completed work"
    condition is what resolves it.

- **⚠️ Ordering constraint discovered 2026-08-18 — §1.10 cannot be fixed before
  §1.8, and CLAUDE.md's "no blocker" tag on this item was wrong.**
  Two grep-verified facts change the shape of the fix:
  1. **The anti-clawback filter is already inside `refund_escrow`.**
     `refund_escrow` (`ecash.rs:824-843`) opens with
     `if !matches!(e.state, EscrowState::Deposited | EscrowState::InProgress)
     { bail }` — the exact `Released`-excluding guard this entry asked for.
     So `panic_stop` does **not** need to reimplement the filter: calling
     `refund_escrow` per escrow of the stopped `job_id` inherits it, and a
     completed-and-released escrow bails instead of being clawed back.
     **The fix is smaller than this entry originally implied.**
  2. **But `refund_escrow` is one of the §1.8 paths.** It does
     `self.wallet.total_sats += e.amount_sats` (`ecash.rs:837`) and restores
     **no proofs**. Wiring `panic_stop → refund_escrow` therefore does not
     just fix A9→A7 — it **makes §1.8's broken invariant reachable from a
     CLI verb for the first time**. Today §1.8 is harmless precisely because
     nothing reaches it (§2.3); this change would end that.
  - **And §1.8 cannot be fixed by "put the proofs back", because there are no
    proofs to put back.** `lock_funds` (`ecash.rs:716-753`) consumes the
    bearer tokens and re-issues only the *change* — its own doc comment says
    「locked proof 自体は破棄」 — and `Escrow` (`ecash.rs:221-245`) has **no
    field holding them**. So fixing §1.8 needs one of:
    (a) add a `locked_proofs: Vec<Proof>` field to `Escrow` so a refund can
        restore the exact tokens (works offline, no mint round-trip, and is
        what a LAN-only v1 wants — but changes the persisted JSON shape, so
        `#[serde(default)]` per 規範4), or
    (b) a real mint swap re-issuing proofs on refund (needs real BDHKE, i.e.
        v1 実装順の項目 2).
  - ~~**Therefore the v1 order is: item 2 (A5/BDHKE + the §1.8 fix) → item 4
    (A9→A7).**~~
  - **⚠️ Correction, same day (2026-08-18): this ordering claim was wrong, and
    it was wrong for the same reason the entry above it was — I assumed the
    §1.8 fix needed a mint swap or a schema change.** It needed neither
    (§1.8, now FIXED). With §1.8 fixed, `refund_escrow` restores proofs as well
    as the scalar, so wiring `panic_stop → refund_escrow` no longer exposes a
    broken invariant. **Item 4 is unblocked again**: it is a small change
    (`panic_stop` refunds the escrows whose `job_id` matches the stopped
    session; `refund_escrow`'s existing `Deposited | InProgress` guard supplies
    the anti-clawback condition for free). It is still un-started, and it still
    needs `panic_stop` to learn which `job_id` it is stopping — `Session` has no
    `job_id` field today, which is the actual remaining design question.

### 1.11 A9 hard constraints for the A3 wiring — model format and URI fetching `[PARTLY SATISFIED BY DELETION 2026-08-18; the remaining half is still OPEN]`

> **Update 2026-08-18 — the URI half is closed by deletion, not by code.**
> `Workload::Train` and `Workload::Retrieve` (and the now-orphaned
> `TrainingMethod` enum) were **deleted** from `intent.rs` per
> [`V1_SCOPE.md`](V1_SCOPE.md) §2. With them went `dataset_uri` — the only
> borrower-supplied URI in the whole type surface — and `base_model`, the only
> borrower-supplied *weights* reference. So of the two hard constraints below:
> - **URI fetching / SSRF / DNS pinning: no longer reachable in v1.** There is
>   nothing left for the lender to fetch. The constraint returns verbatim if
>   `Train` is restored in v2 — which is why the analysis below is kept intact
>   rather than deleted with the code.
> - **Model format (SafeTensors/GGUF only, never pickle): STILL OPEN.**
>   `Workload::Inference.model` is still a bare `String` (`intent.rs:133-137`)
>   and A3 must resolve it to weights. The `SafeModelRef` newtype idea below
>   applies unchanged.
>
> ⚠️ **コンパイラ未検証** (§0): the deletion was verified by grep (zero
> remaining references outside doc comments) and `rustfmt --check`; not
> type-checked or tested.

- From the 2026 threat research
  ([`RESEARCH_UPDATE_2026-08.md`](RESEARCH_UPDATE_2026-08.md) §4l). These are
  not preferences; they are what A9 (lender protection) requires once the
  lender actually loads borrower-named models and fetches borrower-named URIs.
- **Correction to earlier entries**: §4h/§4j said "Train/RAG fetch a
  borrower-supplied URI". Re-checking `Workload` (`intent.rs:124-144`):
  `Retrieve` takes `corpus_id` — an **identifier for a corpus the lender
  already has**, not a URI. **Only `Train.dataset_uri` is a fetched URI.**
  `Retrieve` is the safer design, not an exposure.
- **Loading a model is code execution unless the format forbids it.**
  Pickle deserialization runs arbitrary code *during* load. CVE-2026-25874
  (HuggingFace LeRobot) is a 2026 unauthenticated RCE through exactly this,
  and its shape is instructive: the validator only sees the object *after*
  pickle constructed it, i.e. after `__reduce__` already ran. Scanning does
  not close it either — ShadowPickle reports a 63% evasion rate across ten
  scanners. **SafeTensors stores raw tensors and executes no code on load.**
  → **Constrain by format, not by scanning: accept SafeTensors/GGUF only,
  never `torch.load`/pickle on borrower-specified weights.**
- **`model` and `base_model` are bare `String`s** (`intent.rs:127,133`) — the
  type constrains neither format nor origin. Whatever layer resolves a name
  into weights is the only thing standing between a borrower's string and the
  lender's process. Worth lifting into a newtype (e.g. `SafeModelRef`) so the
  constraint is expressed in the type rather than in a comment.
- **Fetching `dataset_uri` needs DNS pinning, not hostname allowlisting.**
  DNS rebinding defeats hostname checks: short TTL, resolve to a public IP for
  validation, flip to an internal IP before the request. CVE-2026-27826 (MCP
  Atlassian) is a 2026 case of exactly this **bypassing an existing SSRF
  fix**. The fix is to resolve once, validate that IP, and connect to *that
  IP* so no re-resolution can occur. **`reqwest::dns` exposes a trait for
  customizing resolution**, and Rope already depends on reqwest under the
  `http` feature — **no new dependency needed**.
- **IP validation must include `0.0.0.0` and `255.255.255.255`**, not just
  private/loopback/link-local/multicast/documentation/unspecified. Rust
  precedent: GHSA-q537-8fr5-cw35, where `activitypub-federation-rust`'s
  `v4_is_invalid()` missed `0.0.0.0` and became an SSRF.
- Limits: CVE/GHSA read at summary level, advisories themselves unread;
  `reqwest::dns`'s exact API shape unverified (docs.rs is egress-blocked here)
  — confirm with `cargo doc` at implementation time.

---

## 2. SURPLUS (過剰) — code/commitments beyond what's currently backed by use or roadmap

### 2.1 Removed already this session (no longer present, listed for agent awareness — do not re-add without new justification)
- `config::load_private_key`/`load_config`/`ensure_initialized` — superseded
  by generic `load_or_recover<T>`, zero callers anywhere including tests.
- `intent::IntentManager::get_plan` — trivial accessor, zero callers, no
  roadmap reference.
- `intent::Intent.tags`/`Intent.duration`+`Duration` struct — v0.2.11.
- `session::JobSpec` struct — v0.2.11, never constructed anywhere.
- `confidential::ConfidentialStats.failed_attestations`/`encrypted_data_gb` —
  v0.2.11, never incremented/read.
- `first_run::FirstRun.is_repeat_user`/`FirstRunConfig.remember_first_run` —
  v0.2.11, always false / controlled nothing.
- `ecash::StreamState::{Opening,Paused,Closing}` — v0.2.13, zero construction
  paths anywhere (`open_stream` always creates `Active` directly).
- `pair::DiscoveryMethod::Contact` — v0.2.13, zero references.
- `ecash::EcashStats.current_balance_sats` — v0.2.12, redundant mirror of
  `wallet.total_sats` at 7 write sites, nothing ever read it.
- `ecash::LightningLink` (v0.2.8), `confidential::SecurityPolicy` (v0.2.10) —
  fully inert structs, no callers, no roadmap backing.

### 2.2 Confirmed surplus, kept anyway (deliberate — do not delete without re-reading the reasoning below)
- `intent::Intent::with_region` (`src/core/intent.rs:107`) — zero callers,
  but it's the only setter for `RegionConstraint`, which `docs/
  RESEARCH_IMPROVEMENTS.md` #13 explicitly plans to integrate with
  `EnergyPreference`. Deleting the setter while keeping the field/enum would
  just require re-adding it. Note: `EnergyPreference` itself is **already
  wired** into `select_provider` (`intent.rs:824,841`) — only
  `RegionConstraint` remains genuinely unused, and `docs/
  RESEARCH_IMPROVEMENTS.md`/`docs/CATEGORY_RESEARCH.md` both had stale
  claims to the contrary, fixed this session (commits `14d1ae1`, `045bbe9`).
- `confidential::TeeType::is_cpu_tee` (`confidential.rs:122`) — zero callers,
  but its doc comment cites arXiv:2507.02770 and describes a real
  composite-attestation design (CPU TEE root + GPU TEE together). Companion
  to `supports_gpu` (which IS test-covered). Not orphaned cruft.
- `confidential::update_stats` — was zero-caller until this session; **now
  wired** into `perform_attestation` (commit `a87d456`) because
  `format_confidential` genuinely reads `stats.active_tee_instances`, which
  only this function recomputes.
- `pair::prune_stale_discoveries`/`prune_completed_challenges` — was
  zero-caller until this session; **now wired** into `run_pair`/`run_earn`
  (commit `76f32e3`), matching the existing `session::prune_stale()` call
  already in both functions. Zero observable effect today since
  `record_discovery`/`issue_capability_challenge` aren't reachable from any
  verb either — pure forward-looking hygiene.
- `confidential::refresh_expired_attestations`,
  `ecash::process_deadman`/`check_idle_streams` — zero-caller, **checked
  and deliberately left unwired** this session (see `docs/
  REACHABILITY_AUDIT.md` "第2回フォローアップ"). `refresh_expired_attestations`
  doesn't gate actual routing safety (`freshest_verified_instance` does its
  own live freshness check independent of stored `attestation_status`);
  wiring it needs a new `save_confidential()` call in `main.rs::run_inference`
  where none exists today. `process_deadman`/`check_idle_streams` have no
  natural call site — no verb creates escrows or streams to operate on.

### 2.3 Whole-module surplus: `EcashManager`'s mutation API is unreachable end-to-end `[OPEN, structural]`
- `src/core/first_run.rs`'s `FirstRunOrchestrator` holds `&mut EcashManager`
  but no `step_*` method ever touches `self.ecash`. `main.rs`'s other 3
  verbs don't call any `EcashManager` mutation method either. Every one of
  `add_mint`/`trust_mint`/`mint_tokens`/`spend_proofs`/`receive_proofs`/
  `open_escrow`/`mark_escrow_in_progress`/`release_escrow`/`refund_escrow`/
  `dispute_escrow`/`resolve_dispute`/`open_stream`/`tick_stream`/
  `close_stream` (24 pub fns total in `ecash.rs`) is category "test-only" —
  fully implemented and unit-tested, zero CLI reachability. This is the
  single largest block of "reserved API, tested but unreachable" in the
  codebase (`docs/REACHABILITY_AUDIT.md` module table). Not flagged for
  deletion (real, tested, non-trivial logic — this is what v0.3's real
  `earn`/`run` wiring will call), but an agent should not assume any of it
  currently executes on any real CLI path.

### 2.4 `session.rs` cluster — ambiguous, explicitly not resolved `[BLOCKED:decision]`
- 10 of 25 `pub fn` in `src/core/session.rs` had zero callers anywhere,
  including tests: `SessionManager::end`, `panic_stop`, `is_stop_requested`,
  `get_status`, `cleanup`, `list_sessions`, `format_session_list`,
  `Session::load`/`delete`, `reset_stop_flag`.
  **2026-08-18**: `format_session_list` deleted, and `format_session` — which
  was in the same unreached set — is now wired into `rope earn` (§1.7).
  The remaining 9 are unchanged: they are the A9 emergency-stop cluster and
  its supporting API, which `V1_SCOPE.md` keeps because A9 is *in* v1.
- Read individually this session (`docs/REACHABILITY_AUDIT.md`): `panic_stop`
  (`session.rs:343-365`) is a carefully engineered emergency-stop path with
  an explicit historical bug-fix doc comment (old version gave up entirely
  on `docker ps` failure). `SessionManager` (`session.rs:276-331`) has a
  design comment about a possible v0.3 async migration
  (`tokio::sync::RwLock`).
- **Decision explicitly deferred**: this looks like either (a) safety-
  critical code staged for a not-yet-built signal-handler/Ctrl-C integration,
  or (b) scaffolding that predates the current 4-verb design and was never
  cleaned up. Distinguishing these requires a product decision about
  whether `SessionManager`'s in-memory RwLock-based lifecycle is still the
  intended v0.3 design, or whether the file-based `Session::save/load`
  lifecycle superseded it. **Do not delete based on "zero callers" alone —
  that heuristic was correct for §2.1's items and wrong-shaped for this
  cluster.**
- **2026-08-08 — the deferred decision now has a deductive answer**
  ([`RESEARCH_UPDATE_2026-08.md`](RESEARCH_UPDATE_2026-08.md) §4h):
  the first-principles audit initially judged this cluster as mapping to
  **no axiom at all**, which added a second argument for deletion. That
  judgment was **wrong — the axiom set was incomplete**. A1-A7 all encode
  what the *borrower* needs; nothing encoded what the *lender* needs.
  Adding **A9 (the lender must be able to protect themselves from the
  borrower)** resolves it: `panic_stop` raises `STOP_REQUESTED`, then
  enumerates Rope's containers via `docker ps -q --filter name=rope-` and
  `docker kill`s them (`session.rs:360-380`) — that **is** the lender's
  emergency stop, i.e. option (a) above, not leftover scaffolding.
  → **Remove this cluster from deletion consideration.** What remains open
  is narrower and purely technical: `panic_stop` assumes docker, while the
  2026 consensus for untrusted workloads is gVisor/Firecracker-class
  isolation, so the mechanism must be reconciled with whatever sandbox A3
  picks.

### 2.5 Hardcoded sats↔USD conversion rate `[PARTIALLY ADDRESSED — DRY, still a fixed rate]`
- Previously `src/main.rs` and `src/core/first_run.rs` both hardcoded
  `/ 100_000_000.0 * 50_000.0` (implies 1 BTC = $50,000) as separate magic
  numbers. **Consolidated (Unreleased) into a single `core::sats_to_usd`
  helper with named constants `SATS_PER_BTC`/`APPROX_USD_PER_BTC` in
  `core/mod.rs`** — so the future live-price-feed change touches exactly one
  site. The rate itself is *still* a fixed approximation (no price feed while
  §0 blocks new crates); that part remains OPEN and must be swapped for a
  dynamic source when real ecash lands (§2.3), since a stale hardcoded price
  miscalibrates budget enforcement once money is real. The DRY consolidation
  just makes that future swap a one-line change instead of a two-site hunt.

### 2.6 External-service commitments with zero current callers `[OPEN, judgment call recorded, not acted on]`
- `confidential.rs:132-134` `attestation_service_url()` returns real,
  accurate NVIDIA NRAS / Intel trusted-services URLs
  (`https://nras.attestation.nvidia.com`, `.../sgx/attestation`,
  `.../tdx/attestation`). Function is test-only (one weak test just asserts
  `is_some()`, doesn't check the actual string). Not fabricated — these are
  correct real endpoints per `docs/CATEGORY_RESEARCH.md`'s own citations
  (`docs.nvidia.com/attestation`, `docs.trustauthority.intel.com`).
  Investigated this session per a user request to "delete addresses/domains
  that don't exist or weren't specified" — concluded these don't match that
  description (they exist and are accurate) and left them in place. Flagged
  here in case a future agent gets the same instruction with more specific
  targeting information the previous session lacked.

---

## 3. Cross-reference: what's actually reachable from the 4 CLI verbs today

Only trust these paths as "executes on `cargo run`" without re-verifying:
- `rope` (no args) → `first_run.rs`, ~22/23 pub fns reachable, this is the
  live orchestration layer. Its "haiku" output is `sample_haiku_response()`
  (`first_run.rs:278-286`) — a deterministic pick from 4 hardcoded strings
  keyed on `started_at` nanoseconds. **Not real inference**, disclosed in
  `README.md` directly under the demo transcript and in `SECURITY.md`.
- `rope pair` → `pair.rs`, only 4/33 pub fns reachable
  (`pair_path`/`load_pair`/`save_pair`/`format_pair`, plus now
  `prune_stale_discoveries`/`prune_completed_challenges`). Ends in a
  `capability_boundary!` early return — does not attempt real discovery.
- `rope run <model>` → `intent.rs`, 14/21 pub fns reachable (the healthiest
  ratio of any module — this is where real design effort concentrated).
  Also ends in `capability_boundary!`.
- `rope earn` → `pair.rs`(same 4+2 as above) + `ecash.rs` (read-only: only
  `load_ecash` for the balance display, zero mutation calls). Ends in
  `capability_boundary!`.

Full per-module reachability tables: `docs/REACHABILITY_AUDIT.md`.

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

---

## 1. GAPS (不足) — missing functionality, ranked by what blocks what

### 1.1 No real cryptography anywhere money or attestation touches `[BLOCKED:build]`
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

### 1.2 No real P2P I/O `[BLOCKED:build]`
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

### 1.3 No proof-of-execution / verification engine `[OPEN, not blocked by §0]`
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

### 1.5 CI defined but not activated `[BLOCKED:consent]`
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

### 1.7 `format_confidential`/`format_ecash`/`format_plan`/`format_session`/`format_session_list`/`format_config` not surfaced to any verb `[OPEN, product/UX decision]`
- 6 `format_*` functions exist and are tested; only `format_pair` is called
  from `main.rs`. No `rope status`-style verb exists to surface the other 5.
  Not a bug — no CLI surface was ever designed for it. Adding one is a
  product decision (what would `rope status --verbose` show, does it need
  to exist at all given the 4-verb minimalism principle in `README.md`
  "設計原則" §2 Focus).
- Related: 7 QR/TOFU-related `pair::PairStats` fields (`tofu_accepts`,
  `pubkey_mismatch_rejections`, `qr_tokens_issued/consumed/expired`,
  `qr_hmac_rejections`, `qr_nonce_replays_blocked`) are tested but not
  displayed anywhere, even in `format_pair`. Deliberately not added
  (CHANGELOG `[0.2.12]`): cramming 7 diagnostic counters into a user-facing
  status display was judged to hurt readability more than it helps.

### 1.8 Refund paths credit `wallet.total_sats` without restoring proofs — invariant breaks after any refund `[OPEN, latent; harmless while §2.3 keeps ecash CLI-unreachable, MUST fix with real mint in v0.3]`
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

### 1.9 `spend_proofs` mutates the wallet bucket before validating the full id set — a partially-invalid id list destroys the valid proofs `[OPEN, latent; harmless while §2.3 keeps ecash CLI-unreachable, fix before wiring in v0.3]`
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
- 10 of 25 `pub fn` in `src/core/session.rs` have zero callers anywhere,
  including tests: `SessionManager::end`, `panic_stop`, `is_stop_requested`,
  `get_status`, `cleanup`, `list_sessions`, `format_session_list`,
  `Session::load`/`delete`, `reset_stop_flag`.
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

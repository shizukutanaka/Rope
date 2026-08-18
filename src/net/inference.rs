//! A3 — 実際に計算を実行する層。
//!
//! **これはモデルの推論を本当に走らせるコードであって、固定文字列を返す
//! プレースホルダではない** (`docs/FIRST_PRINCIPLES_AUDIT.md` の A3)。
//!
//! ## なぜ依存ゼロなのか
//!
//! `docs/A3_INFERENCE_IMPLEMENTATION_READINESS.md` は mistral.rs を推奨していた。
//! しかし mistral.rs が提供するのは **GPU カーネル・量子化形式の広さ・速度**で
//! あって、**「推論を実行できる」という能力そのものではない**。
//! Transformer の forward pass は算術であり、`std` の f32 演算で足りる。
//!
//! 依存ゼロにしたことの実利は 2 つ:
//! - `static.crates.io` が塞がれた環境でも**コンパイルでき、テストが実際に走る**
//! - A9 (貸し手保護) の攻撃面が最小になる — 外部コードを一切ロードしない
//!
//! 速度は mistral.rs に遠く及ばない。v1 の要件は「小さいモデルが CPU で動く」
//! であり ([`docs/V1_SCOPE.md`](../../docs/V1_SCOPE.md) §3)、それは満たす。
//! GPU バックエンドを足す時は [`InferenceEngine`] を実装すればよい。
//!
//! ## チェックポイント形式
//!
//! llama2.c (Karpathy) の **legacy v1 エクスポート形式**をそのまま読む。
//! 独自形式を作らなかったのは、外部で入手・生成した checkpoint がそのまま
//! 使えるようにするため。
//!
//! ## A9 — この層が守る境界
//!
//! - **借り手が渡せるのはプロンプト文字列だけ。** モデルのパスは**貸し手**が
//!   決める (`Workload::Train`/`Retrieve` を v1 で削除したので、借り手が
//!   URI や重みファイルを指定する経路は型として存在しない)
//! - **コードを一切ロードしない。** pickle も動的ライブラリも読まない
//! - **ヘッダは信用しない。** 全次元に上限を課し、宣言サイズとファイル長の
//!   一致を検証してから割り当てる (壊れた/悪意あるファイルでの OOM 防止)
//! - **計算量に上限。** プロンプト長・生成長・総ステップ数を呼び出し側が縛る

use std::fmt;

// ============================================================================
// エラー
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceError {
    /// チェックポイントが壊れている / 形式が違う
    MalformedCheckpoint(String),
    /// ヘッダが A9 の上限を超えている
    LimitExceeded(String),
    /// トークン ID が語彙の外
    TokenOutOfRange { token: u32, vocab_size: u32 },
    /// 文脈長を超えた
    ContextOverflow { pos: u32, seq_len: u32 },
    /// プロンプトが空
    EmptyPrompt,
}

impl fmt::Display for InferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InferenceError::MalformedCheckpoint(m) => write!(f, "チェックポイント不正: {}", m),
            InferenceError::LimitExceeded(m) => write!(f, "上限超過: {}", m),
            InferenceError::TokenOutOfRange { token, vocab_size } => {
                write!(f, "トークン {} が語彙 {} の外", token, vocab_size)
            }
            InferenceError::ContextOverflow { pos, seq_len } => {
                write!(f, "文脈長超過 (pos={}, seq_len={})", pos, seq_len)
            }
            InferenceError::EmptyPrompt => write!(f, "プロンプトが空"),
        }
    }
}

impl std::error::Error for InferenceError {}

// ============================================================================
// A9: ヘッダを信用しないための上限
// ============================================================================

/// チェックポイントのヘッダに課す上限。
///
/// **ヘッダは貸し手のディスク上のファイルから来るが、それでも検証する** —
/// 壊れたファイルで数 TB の割り当てを試みて OOM で落ちるのは、
/// 「crash は絶対に出さない」(main.rs の capability_boundary 方針) に反する。
#[derive(Debug, Clone, Copy)]
pub struct CheckpointLimits {
    pub max_dim: u32,
    pub max_hidden_dim: u32,
    pub max_layers: u32,
    pub max_heads: u32,
    pub max_vocab: u32,
    pub max_seq_len: u32,
    /// 重みの総要素数 (f32 個数) の上限。実質的なメモリ上限。
    pub max_total_weights: u64,
}

impl Default for CheckpointLimits {
    fn default() -> Self {
        // v1 は「小型モデルが CPU で動く」が要件。7B 級までは通す
        // (dim 4096 / 32 層 / vocab 32000 / 4096 文脈)。
        Self {
            max_dim: 8192,
            max_hidden_dim: 32_768,
            max_layers: 128,
            max_heads: 128,
            max_vocab: 256_000,
            max_seq_len: 32_768,
            // f32 で 8G 要素 = 32GB。これ以上は v1 の想定外。
            max_total_weights: 8_000_000_000,
        }
    }
}

/// 1 回の実行に許す計算量の上限 (A9)。
#[derive(Debug, Clone, Copy)]
pub struct ExecutionLimits {
    pub max_prompt_tokens: u32,
    pub max_output_tokens: u32,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_prompt_tokens: 2048,
            max_output_tokens: 512,
        }
    }
}

// ============================================================================
// モデル
// ============================================================================

/// llama2 系 Transformer のハイパーパラメータ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub dim: u32,
    pub hidden_dim: u32,
    pub n_layers: u32,
    pub n_heads: u32,
    pub n_kv_heads: u32,
    pub vocab_size: u32,
    pub seq_len: u32,
    /// 出力層が埋め込みと重みを共有するか (llama2.c は vocab_size の符号で表す)
    pub shared_classifier: bool,
}

impl Config {
    pub fn head_size(&self) -> u32 {
        self.dim / self.n_heads
    }

    /// このヘッダが要求する重みの総要素数 (f32 個数)。
    ///
    /// **ファイル長の検証に使う** — 宣言と実際が 1 要素でも違えば読まない。
    fn weight_count(&self) -> u64 {
        let dim = self.dim as u64;
        let hidden = self.hidden_dim as u64;
        let layers = self.n_layers as u64;
        let vocab = self.vocab_size as u64;
        let seq = self.seq_len as u64;
        let head_size = self.head_size() as u64;
        let kv_dim = head_size * self.n_kv_heads as u64;
        let q_dim = head_size * self.n_heads as u64;

        let mut n = 0u64;
        n += vocab * dim; // token_embedding_table
        n += layers * dim; // rms_att_weight
        n += layers * dim * q_dim; // wq
        n += layers * dim * kv_dim; // wk
        n += layers * dim * kv_dim; // wv
        n += layers * q_dim * dim; // wo
        n += layers * dim; // rms_ffn_weight
        n += layers * hidden * dim; // w1
        n += layers * dim * hidden; // w2
        n += layers * hidden * dim; // w3
        n += dim; // rms_final_weight
        n += seq * head_size / 2; // 旧 freq_cis_real (読み飛ばす)
        n += seq * head_size / 2; // 旧 freq_cis_imag (読み飛ばす)
        if !self.shared_classifier {
            n += vocab * dim; // wcls
        }
        n
    }
}

/// 重み。全て f32 の連続領域を切り出したビュー。
#[derive(Debug)]
pub struct Weights {
    pub token_embedding: Vec<f32>,
    pub rms_att: Vec<f32>,
    pub wq: Vec<f32>,
    pub wk: Vec<f32>,
    pub wv: Vec<f32>,
    pub wo: Vec<f32>,
    pub rms_ffn: Vec<f32>,
    pub w1: Vec<f32>,
    pub w2: Vec<f32>,
    pub w3: Vec<f32>,
    pub rms_final: Vec<f32>,
    /// 出力層。共有時は token_embedding のコピー。
    pub wcls: Vec<f32>,
}

/// ロード済みモデル。
#[derive(Debug)]
pub struct Model {
    pub config: Config,
    pub weights: Weights,
}

impl Model {
    /// llama2.c legacy v1 形式のバイト列からロードする。
    ///
    /// ヘッダを検証してから割り当てるため、壊れた入力で OOM しない。
    pub fn from_bytes(bytes: &[u8], limits: CheckpointLimits) -> Result<Model, InferenceError> {
        const HEADER_INTS: usize = 7;
        const HEADER_BYTES: usize = HEADER_INTS * 4;
        if bytes.len() < HEADER_BYTES {
            return Err(InferenceError::MalformedCheckpoint(format!(
                "ヘッダに満たない ({} バイト)",
                bytes.len()
            )));
        }

        let mut h = [0i32; HEADER_INTS];
        for (i, slot) in h.iter_mut().enumerate() {
            let o = i * 4;
            *slot = i32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        }

        // llama2.c: vocab_size が負なら分類器の重みは非共有
        let shared_classifier = h[5] > 0;
        let vocab_signed = h[5];
        let vocab_abs = vocab_signed.unsigned_abs();

        let pos = |v: i32, name: &str| -> Result<u32, InferenceError> {
            if v <= 0 {
                Err(InferenceError::MalformedCheckpoint(format!(
                    "{} が非正 ({})",
                    name, v
                )))
            } else {
                Ok(v as u32)
            }
        };

        let config = Config {
            dim: pos(h[0], "dim")?,
            hidden_dim: pos(h[1], "hidden_dim")?,
            n_layers: pos(h[2], "n_layers")?,
            n_heads: pos(h[3], "n_heads")?,
            n_kv_heads: pos(h[4], "n_kv_heads")?,
            vocab_size: pos(vocab_abs as i32, "vocab_size")?,
            seq_len: pos(h[6], "seq_len")?,
            shared_classifier,
        };

        check_limits(&config, &limits)?;

        let expected = config.weight_count();
        let available = ((bytes.len() - HEADER_BYTES) / 4) as u64;
        if available != expected {
            return Err(InferenceError::MalformedCheckpoint(format!(
                "重みの要素数が宣言と不一致 (期待 {}, 実際 {})",
                expected, available
            )));
        }

        let floats = read_f32s(&bytes[HEADER_BYTES..]);
        let weights = slice_weights(&config, &floats);
        Ok(Model { config, weights })
    }
}

fn check_limits(c: &Config, l: &CheckpointLimits) -> Result<(), InferenceError> {
    let bad = |m: String| Err(InferenceError::LimitExceeded(m));
    if c.dim > l.max_dim {
        return bad(format!("dim {} > {}", c.dim, l.max_dim));
    }
    if c.hidden_dim > l.max_hidden_dim {
        return bad(format!(
            "hidden_dim {} > {}",
            c.hidden_dim, l.max_hidden_dim
        ));
    }
    if c.n_layers > l.max_layers {
        return bad(format!("n_layers {} > {}", c.n_layers, l.max_layers));
    }
    if c.n_heads > l.max_heads || c.n_kv_heads > l.max_heads {
        return bad(format!(
            "heads {}/{} > {}",
            c.n_heads, c.n_kv_heads, l.max_heads
        ));
    }
    if c.vocab_size > l.max_vocab {
        return bad(format!("vocab_size {} > {}", c.vocab_size, l.max_vocab));
    }
    if c.seq_len > l.max_seq_len {
        return bad(format!("seq_len {} > {}", c.seq_len, l.max_seq_len));
    }
    if c.n_heads == 0 || c.dim % c.n_heads != 0 {
        return Err(InferenceError::MalformedCheckpoint(format!(
            "dim {} が n_heads {} で割り切れない",
            c.dim, c.n_heads
        )));
    }
    if c.n_kv_heads == 0 || c.n_heads % c.n_kv_heads != 0 {
        return Err(InferenceError::MalformedCheckpoint(format!(
            "n_heads {} が n_kv_heads {} で割り切れない",
            c.n_heads, c.n_kv_heads
        )));
    }
    if c.head_size() % 2 != 0 {
        return Err(InferenceError::MalformedCheckpoint(format!(
            "head_size {} が奇数 (RoPE は偶数を要求する)",
            c.head_size()
        )));
    }
    if c.weight_count() > l.max_total_weights {
        return bad(format!(
            "重み総数 {} > {}",
            c.weight_count(),
            l.max_total_weights
        ));
    }
    Ok(())
}

fn read_f32s(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for c in bytes.chunks_exact(4) {
        out.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
    }
    out
}

fn slice_weights(c: &Config, f: &[f32]) -> Weights {
    let dim = c.dim as usize;
    let hidden = c.hidden_dim as usize;
    let layers = c.n_layers as usize;
    let vocab = c.vocab_size as usize;
    let seq = c.seq_len as usize;
    let head_size = c.head_size() as usize;
    let kv_dim = head_size * c.n_kv_heads as usize;
    let q_dim = head_size * c.n_heads as usize;

    let mut o = 0usize;
    let mut take = |n: usize| {
        let s = f[o..o + n].to_vec();
        o += n;
        s
    };

    let token_embedding = take(vocab * dim);
    let rms_att = take(layers * dim);
    let wq = take(layers * dim * q_dim);
    let wk = take(layers * dim * kv_dim);
    let wv = take(layers * dim * kv_dim);
    let wo = take(layers * q_dim * dim);
    let rms_ffn = take(layers * dim);
    let w1 = take(layers * hidden * dim);
    let w2 = take(layers * dim * hidden);
    let w3 = take(layers * hidden * dim);
    let rms_final = take(dim);
    let _skip_real = take(seq * head_size / 2);
    let _skip_imag = take(seq * head_size / 2);
    let wcls = if c.shared_classifier {
        token_embedding.clone()
    } else {
        take(vocab * dim)
    };

    Weights {
        token_embedding,
        rms_att,
        wq,
        wk,
        wv,
        wo,
        rms_ffn,
        w1,
        w2,
        w3,
        rms_final,
        wcls,
    }
}

// ============================================================================
// 数値カーネル
// ============================================================================

/// RMSNorm: `out = x / rms(x) * weight`
pub fn rmsnorm(out: &mut [f32], x: &[f32], weight: &[f32]) {
    let n = x.len();
    let mut ss = 0.0f32;
    for v in x {
        ss += v * v;
    }
    ss = ss / n as f32 + 1e-5;
    let scale = 1.0 / ss.sqrt();
    for i in 0..n {
        out[i] = weight[i] * (scale * x[i]);
    }
}

/// in-place softmax (数値安定版)
pub fn softmax(x: &mut [f32]) {
    if x.is_empty() {
        return;
    }
    let mut max = x[0];
    for v in x.iter() {
        if *v > max {
            max = *v;
        }
    }
    let mut sum = 0.0f32;
    for v in x.iter_mut() {
        *v = (*v - max).exp();
        sum += *v;
    }
    if sum > 0.0 {
        for v in x.iter_mut() {
            *v /= sum;
        }
    }
}

/// `out = W · x` — W は行優先 (n 行 × d 列)
pub fn matmul(out: &mut [f32], x: &[f32], w: &[f32], d: usize, n: usize) {
    for i in 0..n {
        let row = &w[i * d..i * d + d];
        let mut sum = 0.0f32;
        for j in 0..d {
            sum += row[j] * x[j];
        }
        out[i] = sum;
    }
}

// ============================================================================
// 実行状態
// ============================================================================

/// forward pass の作業領域 + KV キャッシュ。
pub struct RunState {
    x: Vec<f32>,
    xb: Vec<f32>,
    xb2: Vec<f32>,
    hb: Vec<f32>,
    hb2: Vec<f32>,
    q: Vec<f32>,
    att: Vec<f32>,
    logits: Vec<f32>,
    key_cache: Vec<f32>,
    value_cache: Vec<f32>,
}

impl RunState {
    pub fn new(c: &Config) -> RunState {
        let dim = c.dim as usize;
        let hidden = c.hidden_dim as usize;
        let layers = c.n_layers as usize;
        let seq = c.seq_len as usize;
        let kv_dim = (c.head_size() * c.n_kv_heads) as usize;
        RunState {
            x: vec![0.0; dim],
            xb: vec![0.0; dim],
            xb2: vec![0.0; dim],
            hb: vec![0.0; hidden],
            hb2: vec![0.0; hidden],
            q: vec![0.0; dim],
            att: vec![0.0; c.n_heads as usize * seq],
            logits: vec![0.0; c.vocab_size as usize],
            key_cache: vec![0.0; layers * seq * kv_dim],
            value_cache: vec![0.0; layers * seq * kv_dim],
        }
    }
}

impl Model {
    /// 1 トークン分の forward pass。`pos` 位置のロジットを返す。
    ///
    /// KV キャッシュを使うため、同じ `RunState` を使い回して逐次呼ぶこと。
    pub fn forward<'s>(
        &self,
        state: &'s mut RunState,
        token: u32,
        pos: u32,
    ) -> Result<&'s [f32], InferenceError> {
        let c = &self.config;
        if token >= c.vocab_size {
            return Err(InferenceError::TokenOutOfRange {
                token,
                vocab_size: c.vocab_size,
            });
        }
        if pos >= c.seq_len {
            return Err(InferenceError::ContextOverflow {
                pos,
                seq_len: c.seq_len,
            });
        }

        let w = &self.weights;
        let dim = c.dim as usize;
        let hidden = c.hidden_dim as usize;
        let head_size = c.head_size() as usize;
        let kv_dim = head_size * c.n_kv_heads as usize;
        let kv_mul = (c.n_heads / c.n_kv_heads) as usize;
        let seq = c.seq_len as usize;
        let posu = pos as usize;

        state
            .x
            .copy_from_slice(&w.token_embedding[token as usize * dim..(token as usize + 1) * dim]);

        for l in 0..c.n_layers as usize {
            rmsnorm(&mut state.xb, &state.x, &w.rms_att[l * dim..(l + 1) * dim]);

            let loff = l * seq * kv_dim;
            let koff = loff + posu * kv_dim;

            matmul(
                &mut state.q,
                &state.xb,
                &w.wq[l * dim * dim..(l + 1) * dim * dim],
                dim,
                dim,
            );
            {
                let (k_slice, v_slice) = (
                    &mut state.key_cache[koff..koff + kv_dim],
                    &mut state.value_cache[koff..koff + kv_dim],
                );
                matmul(
                    k_slice,
                    &state.xb,
                    &w.wk[l * dim * kv_dim..(l + 1) * dim * kv_dim],
                    dim,
                    kv_dim,
                );
                matmul(
                    v_slice,
                    &state.xb,
                    &w.wv[l * dim * kv_dim..(l + 1) * dim * kv_dim],
                    dim,
                    kv_dim,
                );
            }

            // RoPE: q と k の各ヘッド内で 2 要素ずつ回転させる
            apply_rope(
                &mut state.q,
                &mut state.key_cache[koff..koff + kv_dim],
                pos,
                head_size,
                dim,
                kv_dim,
            );

            // multi-head attention
            state.xb.iter_mut().for_each(|v| *v = 0.0);
            for h in 0..c.n_heads as usize {
                let qoff = h * head_size;
                let attoff = h * seq;
                for t in 0..=posu {
                    let koff_t = loff + t * kv_dim + (h / kv_mul) * head_size;
                    let mut score = 0.0f32;
                    for i in 0..head_size {
                        score += state.q[qoff + i] * state.key_cache[koff_t + i];
                    }
                    state.att[attoff + t] = score / (head_size as f32).sqrt();
                }
                softmax(&mut state.att[attoff..attoff + posu + 1]);
                for t in 0..=posu {
                    let voff_t = loff + t * kv_dim + (h / kv_mul) * head_size;
                    let a = state.att[attoff + t];
                    for i in 0..head_size {
                        state.xb[qoff + i] += a * state.value_cache[voff_t + i];
                    }
                }
            }

            matmul(
                &mut state.xb2,
                &state.xb,
                &w.wo[l * dim * dim..(l + 1) * dim * dim],
                dim,
                dim,
            );
            for i in 0..dim {
                state.x[i] += state.xb2[i];
            }

            // FFN: SwiGLU
            rmsnorm(&mut state.xb, &state.x, &w.rms_ffn[l * dim..(l + 1) * dim]);
            matmul(
                &mut state.hb,
                &state.xb,
                &w.w1[l * dim * hidden..(l + 1) * dim * hidden],
                dim,
                hidden,
            );
            matmul(
                &mut state.hb2,
                &state.xb,
                &w.w3[l * dim * hidden..(l + 1) * dim * hidden],
                dim,
                hidden,
            );
            for i in 0..hidden {
                let v = state.hb[i];
                state.hb[i] = v * (1.0 / (1.0 + (-v).exp())) * state.hb2[i];
            }
            matmul(
                &mut state.xb,
                &state.hb,
                &w.w2[l * dim * hidden..(l + 1) * dim * hidden],
                hidden,
                dim,
            );
            for i in 0..dim {
                state.x[i] += state.xb[i];
            }
        }

        let x_final = state.x.clone();
        rmsnorm(&mut state.x, &x_final, &w.rms_final);
        matmul(
            &mut state.logits,
            &state.x,
            &w.wcls,
            dim,
            c.vocab_size as usize,
        );
        Ok(&state.logits)
    }
}

fn apply_rope(q: &mut [f32], k: &mut [f32], pos: u32, head_size: usize, dim: usize, kv_dim: usize) {
    let mut i = 0usize;
    while i < dim {
        let head_dim = (i % head_size) as f32;
        let freq = 1.0f32 / 10000f32.powf(head_dim / head_size as f32);
        let val = pos as f32 * freq;
        let (fcr, fci) = (val.cos(), val.sin());
        // q は常に回す。k は kv_dim までしか無い
        let rotn = if i < kv_dim { 2 } else { 1 };
        for v in 0..rotn {
            let vec: &mut [f32] = if v == 0 { q } else { k };
            let (v0, v1) = (vec[i], vec[i + 1]);
            vec[i] = v0 * fcr - v1 * fci;
            vec[i + 1] = v0 * fci + v1 * fcr;
        }
        i += 2;
    }
}

// ============================================================================
// サンプラー
// ============================================================================

/// llama2.c と同じ xorshift64* 。`rand` crate を使わないのは依存を増やさない
/// ため、および **seed を固定すれば出力が完全に再現する**ようにするため
/// (`A4` 検証を将来入れる時、再現性は前提になる)。
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(if seed == 0 { 0x9E3779B97F4A7C15 } else { seed })
    }
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545F4914F6CDD1D) >> 32) as u32
    }
    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }
}

/// サンプリング方針。
#[derive(Debug, Clone, Copy)]
pub struct Sampler {
    /// 0.0 なら argmax (完全決定論的)
    pub temperature: f32,
    /// nucleus sampling の閾値 (1.0 で無効)
    pub top_p: f32,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            temperature: 0.9,
            top_p: 0.9,
        }
    }
}

impl Sampler {
    pub fn deterministic() -> Sampler {
        Sampler {
            temperature: 0.0,
            top_p: 1.0,
        }
    }

    /// ロジットから次トークンを選ぶ。
    pub fn sample(&self, logits: &[f32], rng: &mut Rng) -> u32 {
        if logits.is_empty() {
            return 0;
        }
        if self.temperature <= 0.0 {
            return argmax(logits) as u32;
        }
        let mut probs: Vec<f32> = logits.iter().map(|l| l / self.temperature).collect();
        softmax(&mut probs);
        if self.top_p >= 1.0 {
            return sample_multinomial(&probs, rng.next_f32()) as u32;
        }
        sample_top_p(&probs, self.top_p, rng.next_f32()) as u32
    }
}

fn argmax(v: &[f32]) -> usize {
    let mut best = 0usize;
    for i in 1..v.len() {
        if v[i] > v[best] {
            best = i;
        }
    }
    best
}

fn sample_multinomial(probs: &[f32], coin: f32) -> usize {
    let mut cdf = 0.0f32;
    for (i, p) in probs.iter().enumerate() {
        cdf += p;
        if coin < cdf {
            return i;
        }
    }
    probs.len() - 1
}

fn sample_top_p(probs: &[f32], top_p: f32, coin: f32) -> usize {
    let mut idx: Vec<usize> = (0..probs.len()).collect();
    idx.sort_by(|a, b| {
        probs[*b]
            .partial_cmp(&probs[*a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut cum = 0.0f32;
    let mut cutoff = idx.len();
    for (rank, i) in idx.iter().enumerate() {
        cum += probs[*i];
        if cum > top_p {
            cutoff = rank + 1;
            break;
        }
    }
    let head = &idx[..cutoff];
    let mass: f32 = head.iter().map(|i| probs[*i]).sum();
    let target = coin * mass;
    let mut acc = 0.0f32;
    for i in head {
        acc += probs[*i];
        if target < acc {
            return *i;
        }
    }
    *head.last().unwrap_or(&0)
}

// ============================================================================
// トークナイザ (llama2.c tokenizer.bin 形式)
// ============================================================================

/// SentencePiece 由来の語彙 + スコアによる貪欲マージ。
#[derive(Debug, Clone)]
pub struct Tokenizer {
    vocab: Vec<String>,
    scores: Vec<f32>,
    /// 語彙の逆引き。線形探索だと encode がプロンプト長 × 語彙数になり、
    /// 32k 語彙で実用にならない (④ サイクルタイムはユーザーにも効く)。
    index: std::collections::HashMap<String, u32>,
}

pub const BOS: u32 = 1;
pub const EOS: u32 = 2;
/// バイトフォールバックの開始位置 (llama2.c と同じ)
const BYTE_FALLBACK_BASE: u32 = 3;

impl Tokenizer {
    /// `tokenizer.bin`: i32 max_token_length, その後 vocab_size 回
    /// [f32 score][i32 len][len バイト]。
    pub fn from_bytes(bytes: &[u8], vocab_size: u32) -> Result<Tokenizer, InferenceError> {
        let bad = |m: &str| InferenceError::MalformedCheckpoint(format!("tokenizer: {}", m));
        if bytes.len() < 4 {
            return Err(bad("ヘッダに満たない"));
        }
        let mut o = 4usize; // max_token_length は使わない
        let mut vocab = Vec::with_capacity(vocab_size as usize);
        let mut scores = Vec::with_capacity(vocab_size as usize);
        let mut index = std::collections::HashMap::with_capacity(vocab_size as usize);
        for i in 0..vocab_size as usize {
            if o + 8 > bytes.len() {
                return Err(bad(&format!("{} 個目の手前で尽きた", i)));
            }
            let score = f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
            let len = i32::from_le_bytes([bytes[o + 4], bytes[o + 5], bytes[o + 6], bytes[o + 7]]);
            o += 8;
            if len < 0 || o + len as usize > bytes.len() {
                return Err(bad(&format!("{} 個目の長さが不正 ({})", i, len)));
            }
            let piece = String::from_utf8_lossy(&bytes[o..o + len as usize]).into_owned();
            o += len as usize;
            // 同じ piece が複数 id を持つ場合、最初の id を採用する
            // (llama2.c の str_lookup と同じ挙動)
            index.entry(piece.clone()).or_insert(i as u32);
            vocab.push(piece);
            scores.push(score);
        }
        Ok(Tokenizer {
            vocab,
            scores,
            index,
        })
    }

    pub fn vocab_size(&self) -> usize {
        self.vocab.len()
    }

    fn lookup(&self, s: &str) -> Option<u32> {
        self.index.get(s).copied()
    }

    /// テキスト → トークン列。llama2.c の encode と同じ貪欲マージ。
    pub fn encode(&self, text: &str, bos: bool, eos: bool) -> Vec<u32> {
        let mut tokens = Vec::new();
        if bos {
            tokens.push(BOS);
        }
        // SentencePiece の慣習: 先頭に区切りスペースを足す
        if !text.is_empty() {
            if let Some(t) = self.lookup("\u{2581}") {
                tokens.push(t);
            }
        }
        // まず 1 文字ずつ。語彙に無ければバイトフォールバック
        for ch in text.chars() {
            let s: String = if ch == ' ' {
                "\u{2581}".to_string()
            } else {
                ch.to_string()
            };
            match self.lookup(&s) {
                Some(t) => tokens.push(t),
                None => {
                    let mut buf = [0u8; 4];
                    for b in ch.encode_utf8(&mut buf).as_bytes() {
                        let id = *b as u32 + BYTE_FALLBACK_BASE;
                        // 語彙にバイトフォールバック領域が無い (小さい語彙の) 場合、
                        // ここで範囲外 id を作ると forward が TokenOutOfRange で
                        // 落ちる。UNK に倒す方が正しい。
                        tokens.push(if (id as usize) < self.vocab.len() {
                            id
                        } else {
                            0
                        });
                    }
                }
            }
        }
        // スコア最大のペアを貪欲にマージ
        loop {
            let mut best: Option<(f32, usize, u32)> = None;
            for i in 0..tokens.len().saturating_sub(1) {
                let a = self.piece(tokens[i]);
                let b = self.piece(tokens[i + 1]);
                // 空の piece とマージすると、隣のトークンが自分自身にマージされて
                // 消える (範囲外 id や空語彙エントリで実際に起きた)。
                if a.is_empty() || b.is_empty() {
                    continue;
                }
                let merged = format!("{}{}", a, b);
                if let Some(id) = self.lookup(&merged) {
                    let sc = self.scores[id as usize];
                    if best.map(|(bs, _, _)| sc > bs).unwrap_or(true) {
                        best = Some((sc, i, id));
                    }
                }
            }
            match best {
                Some((_, i, id)) => {
                    tokens[i] = id;
                    tokens.remove(i + 1);
                }
                None => break,
            }
        }
        if eos {
            tokens.push(EOS);
        }
        tokens
    }

    fn piece(&self, token: u32) -> &str {
        self.vocab
            .get(token as usize)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// トークン → 表示文字列。`prev` は直前のトークン (BOS 直後の空白を落とす)。
    pub fn decode(&self, prev: u32, token: u32) -> String {
        let p = self.piece(token);
        // BOS の直後は先頭空白を落とす (llama2.c と同じ)
        let p = if prev == BOS {
            p.trim_start_matches(' ')
        } else {
            p
        };
        // "<0xXX>" 形式のバイトトークンを実バイトへ
        if let Some(hex) = p.strip_prefix("<0x").and_then(|s| s.strip_suffix('>')) {
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                return String::from_utf8_lossy(&[b]).into_owned();
            }
        }
        p.replace('\u{2581}', " ")
    }
}

// ============================================================================
// 生成 — ここが A3 の入口
// ============================================================================

/// 実行結果。**実際に計算した事実を数値で残す** — 「本当に走ったのか」を
/// 呼び出し側が確認できるようにするため (将来 A4 検証を足す時の土台)。
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    pub text: String,
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    /// forward pass を回した回数 = 実際に投入した計算量
    pub forward_passes: u32,
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// EOS が出た
    EndOfSequence,
    /// max_output_tokens に到達
    OutputLimit,
    /// モデルの文脈長に到達
    ContextLimit,
}

/// 推論バックエンドの抽象。GPU 実装を足す時はこれを実装する。
pub trait InferenceEngine {
    fn generate(
        &self,
        prompt: &str,
        limits: ExecutionLimits,
        sampler: Sampler,
        seed: u64,
    ) -> Result<Completion, InferenceError>;
}

/// CPU・f32・依存ゼロのバックエンド。
pub struct CpuEngine {
    pub model: Model,
    pub tokenizer: Tokenizer,
}

impl InferenceEngine for CpuEngine {
    fn generate(
        &self,
        prompt: &str,
        limits: ExecutionLimits,
        sampler: Sampler,
        seed: u64,
    ) -> Result<Completion, InferenceError> {
        if prompt.trim().is_empty() {
            return Err(InferenceError::EmptyPrompt);
        }
        let prompt_tokens = self.tokenizer.encode(prompt, true, false);
        if prompt_tokens.len() as u32 > limits.max_prompt_tokens {
            return Err(InferenceError::LimitExceeded(format!(
                "プロンプト {} トークン > 上限 {}",
                prompt_tokens.len(),
                limits.max_prompt_tokens
            )));
        }
        if prompt_tokens.is_empty() {
            return Err(InferenceError::EmptyPrompt);
        }

        let mut state = RunState::new(&self.model.config);
        let mut rng = Rng::new(seed);
        let mut text = String::new();
        let mut forward_passes = 0u32;
        let mut output_tokens = 0u32;
        let mut token = prompt_tokens[0];
        let mut pos = 0u32;
        let mut stop = StopReason::OutputLimit;

        loop {
            if pos >= self.model.config.seq_len {
                stop = StopReason::ContextLimit;
                break;
            }
            let logits = self.model.forward(&mut state, token, pos)?;
            forward_passes += 1;

            let next = if (pos as usize + 1) < prompt_tokens.len() {
                // まだプロンプトを流し込んでいる最中 (prefill)
                prompt_tokens[pos as usize + 1]
            } else {
                let n = sampler.sample(logits, &mut rng);
                if n == EOS {
                    stop = StopReason::EndOfSequence;
                    break;
                }
                output_tokens += 1;
                text.push_str(&self.tokenizer.decode(token, n));
                n
            };

            pos += 1;
            token = next;

            if output_tokens >= limits.max_output_tokens {
                stop = StopReason::OutputLimit;
                break;
            }
        }

        Ok(Completion {
            text,
            prompt_tokens: prompt_tokens.len() as u32,
            output_tokens,
            forward_passes,
            stop_reason: stop,
        })
    }
}

// ============================================================================
// ファイルからのロードと、借り手入力の無害化
// ============================================================================

impl Model {
    /// チェックポイントをファイルから読む。
    pub fn load(path: &std::path::Path, limits: CheckpointLimits) -> Result<Model, InferenceError> {
        let bytes = std::fs::read(path).map_err(|e| {
            InferenceError::MalformedCheckpoint(format!("{} を読めない: {}", path.display(), e))
        })?;
        Model::from_bytes(&bytes, limits)
    }
}

impl Tokenizer {
    pub fn load(path: &std::path::Path, vocab_size: u32) -> Result<Tokenizer, InferenceError> {
        let bytes = std::fs::read(path).map_err(|e| {
            InferenceError::MalformedCheckpoint(format!("{} を読めない: {}", path.display(), e))
        })?;
        Tokenizer::from_bytes(&bytes, vocab_size)
    }
}

impl CpuEngine {
    /// モデルとトークナイザをファイルからロードする。
    ///
    /// トークナイザの語彙数はモデルのヘッダに合わせる — 食い違っていれば
    /// ロード時に落ちるので、実行中に範囲外 id が出ることはない。
    pub fn load(
        model_path: &std::path::Path,
        tokenizer_path: &std::path::Path,
        limits: CheckpointLimits,
    ) -> Result<CpuEngine, InferenceError> {
        let model = Model::load(model_path, limits)?;
        let tokenizer = Tokenizer::load(tokenizer_path, model.config.vocab_size)?;
        Ok(CpuEngine { model, tokenizer })
    }
}

/// モデル名をファイル名の一部として使う前に無害化する (A9)。
///
/// `rope run <model>` のモデル名は、貸し手側から見れば**外から来た文字列**である。
/// これをそのままパスに連結すると `../../etc/passwd` のような読み出しを許す。
/// **モデルの置き場所は貸し手が決める**という原則 (`docs/V1_SCOPE.md` §3) を
/// 型ではなくここで担保する。
///
/// 許すのは ASCII 英数字 / `-` / `_` / `.` のみ。`..` は明示的に拒否する。
pub fn safe_model_stem(name: &str) -> Result<String, InferenceError> {
    let reject = |why: &str| {
        Err(InferenceError::LimitExceeded(format!(
            "モデル名が不正 ({}): {:?}",
            why, name
        )))
    };
    if name.is_empty() || name.len() > 128 {
        return reject("長さ");
    }
    if name.contains("..") {
        return reject("親ディレクトリ参照");
    }
    if name.starts_with('.') {
        return reject("先頭ドット");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return reject("使用できない文字");
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用チェックポイントを組み立てる (llama2.c legacy v1 レイアウト)。
    struct Builder {
        c: Config,
        secs: Vec<Vec<f32>>,
    }

    pub(super) fn build_checkpoint(c: Config, fill: impl Fn(&str, usize) -> Vec<f32>) -> Vec<u8> {
        let dim = c.dim as usize;
        let hidden = c.hidden_dim as usize;
        let layers = c.n_layers as usize;
        let vocab = c.vocab_size as usize;
        let seq = c.seq_len as usize;
        let hs = c.head_size() as usize;
        let kv_dim = hs * c.n_kv_heads as usize;
        let q_dim = hs * c.n_heads as usize;

        let mut secs: Vec<(&str, usize)> = vec![
            ("emb", vocab * dim),
            ("rms_att", layers * dim),
            ("wq", layers * dim * q_dim),
            ("wk", layers * dim * kv_dim),
            ("wv", layers * dim * kv_dim),
            ("wo", layers * q_dim * dim),
            ("rms_ffn", layers * dim),
            ("w1", layers * hidden * dim),
            ("w2", layers * dim * hidden),
            ("w3", layers * hidden * dim),
            ("rms_final", dim),
            ("freq_r", seq * hs / 2),
            ("freq_i", seq * hs / 2),
        ];
        if !c.shared_classifier {
            secs.push(("wcls", vocab * dim));
        }

        let mut out = Vec::new();
        let vocab_signed = if c.shared_classifier {
            c.vocab_size as i32
        } else {
            -(c.vocab_size as i32)
        };
        for v in [
            c.dim as i32,
            c.hidden_dim as i32,
            c.n_layers as i32,
            c.n_heads as i32,
            c.n_kv_heads as i32,
            vocab_signed,
            c.seq_len as i32,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for (name, n) in secs {
            let vals = fill(name, n);
            assert_eq!(vals.len(), n, "section {} size", name);
            for v in vals {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out
    }

    pub(super) fn tiny_config(shared: bool) -> Config {
        Config {
            dim: 4,
            hidden_dim: 4,
            n_layers: 1,
            n_heads: 2,
            n_kv_heads: 2,
            vocab_size: 4,
            seq_len: 4,
            shared_classifier: shared,
        }
    }

    fn identity(n: usize, d: usize) -> Vec<f32> {
        let mut v = vec![0.0; n];
        for i in 0..d {
            v[i * d + i] = 1.0;
        }
        v
    }

    #[test]
    fn rmsnorm_matches_hand_computation() {
        let x = [3.0f32, 4.0, 0.0, 0.0];
        let w = [1.0f32; 4];
        let mut out = [0.0f32; 4];
        rmsnorm(&mut out, &x, &w);
        // ss = (9+16)/4 = 6.25 (+1e-5); scale = 1/2.5 = 0.4
        assert!((out[0] - 1.2).abs() < 1e-4, "{:?}", out);
        assert!((out[1] - 1.6).abs() < 1e-4, "{:?}", out);
    }

    #[test]
    fn softmax_sums_to_one_and_is_stable() {
        let mut x = [1000.0f32, 1000.0, 1000.0]; // オーバーフローしうる入力
        softmax(&mut x);
        let s: f32 = x.iter().sum();
        assert!((s - 1.0).abs() < 1e-5, "{:?}", x);
        for v in x {
            assert!((v - 1.0 / 3.0).abs() < 1e-5);
        }
    }

    #[test]
    fn matmul_matches_hand_computation() {
        // W = [[1,2],[3,4]], x = [5,6] -> [17, 39]
        let w = [1.0f32, 2.0, 3.0, 4.0];
        let x = [5.0f32, 6.0];
        let mut out = [0.0f32; 2];
        matmul(&mut out, &x, &w, 2, 2);
        assert_eq!(out, [17.0, 39.0]);
    }

    #[test]
    fn forward_with_zeroed_blocks_returns_normalized_embedding() {
        // attention と FFN を全てゼロにすると、残差だけが通る。
        // よって logits = wcls · rmsnorm(embedding[token])。wcls = 単位行列。
        let c = tiny_config(false);
        let bytes = build_checkpoint(c, |name, n| match name {
            "emb" => {
                let mut v = vec![0.0; n];
                v[1 * 4 + 1] = 2.0;
                v
            } // token1 = [0,2,0,0]
            "rms_att" | "rms_ffn" | "rms_final" => vec![1.0; n],
            "wcls" => identity(n, 4),
            _ => vec![0.0; n],
        });
        let m = Model::from_bytes(&bytes, CheckpointLimits::default()).expect("load");
        let mut st = RunState::new(&m.config);
        let logits = m.forward(&mut st, 1, 0).expect("forward");
        // rmsnorm([0,2,0,0]) = [0, 2/sqrt(1+1e-5), 0, 0] ≈ [0,2,0,0]
        assert!((logits[1] - 2.0).abs() < 1e-3, "{:?}", logits);
        assert!(
            logits[0].abs() < 1e-6 && logits[2].abs() < 1e-6,
            "{:?}",
            logits
        );
    }

    #[test]
    fn forward_is_deterministic() {
        let c = tiny_config(true);
        let bytes = build_checkpoint(c, |name, n| match name {
            "rms_att" | "rms_ffn" | "rms_final" => vec![1.0; n],
            _ => (0..n).map(|i| ((i % 7) as f32 - 3.0) * 0.1).collect(),
        });
        let m = Model::from_bytes(&bytes, CheckpointLimits::default()).unwrap();
        let run = || {
            let mut st = RunState::new(&m.config);
            let mut acc = Vec::new();
            for pos in 0..3u32 {
                acc.push(m.forward(&mut st, pos % 4, pos).unwrap().to_vec());
            }
            acc
        };
        assert_eq!(run(), run(), "同じ入力は同じ出力 (A4 の前提)");
    }

    #[test]
    fn attention_actually_attends_to_earlier_tokens() {
        // KV キャッシュが効いていれば、pos=1 の出力は pos=0 に何を入れたかに依存する。
        let c = tiny_config(true);
        let bytes = build_checkpoint(c, |name, n| match name {
            "rms_att" | "rms_ffn" | "rms_final" => vec![1.0; n],
            "emb" => (0..n).map(|i| (i as f32 + 1.0) * 0.3).collect(),
            "wq" | "wk" | "wv" | "wo" => identity(n, 4),
            _ => vec![0.0; n],
        });
        let m = Model::from_bytes(&bytes, CheckpointLimits::default()).unwrap();
        let second_after = |first: u32| {
            let mut st = RunState::new(&m.config);
            m.forward(&mut st, first, 0).unwrap();
            m.forward(&mut st, 2, 1).unwrap().to_vec()
        };
        let a = second_after(0);
        let b = second_after(3);
        assert_ne!(a, b, "先行トークンが違えば出力も違う = 注意が効いている");
    }

    #[test]
    fn rejects_truncated_and_malformed_checkpoints() {
        assert!(matches!(
            Model::from_bytes(&[0u8; 3], CheckpointLimits::default()),
            Err(InferenceError::MalformedCheckpoint(_))
        ));
        let c = tiny_config(true);
        let mut bytes = build_checkpoint(c, |_, n| vec![0.0; n]);
        bytes.truncate(bytes.len() - 4); // 重みを 1 要素削る
        assert!(
            matches!(
                Model::from_bytes(&bytes, CheckpointLimits::default()),
                Err(InferenceError::MalformedCheckpoint(_))
            ),
            "宣言とファイル長の不一致を検出する"
        );
    }

    #[test]
    fn refuses_oversized_header_without_allocating() {
        // 「dim = 2^30」と主張するヘッダ。割り当てを試みたら OOM で落ちる。
        let mut bytes = Vec::new();
        for v in [1i32 << 30, 4, 1, 1, 1, 4, 4] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&[0u8; 64]);
        let r = Model::from_bytes(&bytes, CheckpointLimits::default());
        assert!(
            matches!(r, Err(InferenceError::LimitExceeded(_))),
            "{:?}",
            r
        );
    }

    #[test]
    fn rejects_out_of_range_token_and_position() {
        let c = tiny_config(true);
        let bytes = build_checkpoint(c, |name, n| match name {
            "rms_att" | "rms_ffn" | "rms_final" => vec![1.0; n],
            _ => vec![0.0; n],
        });
        let m = Model::from_bytes(&bytes, CheckpointLimits::default()).unwrap();
        let mut st = RunState::new(&m.config);
        assert!(matches!(
            m.forward(&mut st, 99, 0),
            Err(InferenceError::TokenOutOfRange { .. })
        ));
        assert!(matches!(
            m.forward(&mut st, 0, 99),
            Err(InferenceError::ContextOverflow { .. })
        ));
    }

    // ---------------------------------------------------------------------------
    // 生成の end-to-end (合成モデル + 合成トークナイザ)
    // ---------------------------------------------------------------------------

    pub(super) fn tok_bytes(pieces: &[(&str, f32)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&16i32.to_le_bytes()); // max_token_length
        for (p, s) in pieces {
            out.extend_from_slice(&s.to_le_bytes());
            out.extend_from_slice(&(p.len() as i32).to_le_bytes());
            out.extend_from_slice(p.as_bytes());
        }
        out
    }

    pub(super) const PIECES: &[(&str, f32)] = &[
        ("<unk>", 0.0),
        ("<s>", 0.0),
        ("</s>", 0.0),
        ("\u{2581}", -1.0),
        ("a", -2.0),
        ("b", -3.0),
        ("ab", 5.0),
        ("\u{2581}a", 4.0),
    ];

    /// attention/FFN をゼロにし、wcls をトークン 5 ("b") だけ立てる。
    /// → argmax は常に 5。生成は "bbbb..." になる。
    fn predictable_engine() -> CpuEngine {
        let c = Config {
            dim: 4,
            hidden_dim: 4,
            n_layers: 1,
            n_heads: 2,
            n_kv_heads: 2,
            vocab_size: 8,
            seq_len: 16,
            shared_classifier: false,
        };
        let bytes = build_checkpoint(c, |name, n| match name {
            "emb" => (0..n).map(|i| if i % 4 == 0 { 2.0 } else { 0.0 }).collect(),
            "rms_att" | "rms_ffn" | "rms_final" => vec![1.0; n],
            "wcls" => {
                let mut v = vec![0.0; n];
                v[5 * 4] = 1.0;
                v
            }
            _ => vec![0.0; n],
        });
        CpuEngine {
            model: Model::from_bytes(&bytes, CheckpointLimits::default()).unwrap(),
            tokenizer: Tokenizer::from_bytes(&tok_bytes(PIECES), 8).unwrap(),
        }
    }

    #[test]
    fn tokenizer_merges_by_score() {
        let t = Tokenizer::from_bytes(&tok_bytes(PIECES), 8).unwrap();
        // "ab" は score 5.0 で最優先マージ → 単一トークン 6 になる
        let ids = t.encode("ab", false, false);
        assert!(ids.contains(&6), "ab がマージされる: {:?}", ids);
        assert!(
            !ids.contains(&4) || !ids.contains(&5),
            "a と b が別々に残らない: {:?}",
            ids
        );
    }

    /// 実際の llama 語彙と同じ形: id 3..258 が <0x00>..<0xFF> のバイトトークン。
    fn byte_fallback_vocab() -> (Vec<u8>, u32) {
        let mut pieces: Vec<(String, f32)> = vec![
            ("<unk>".into(), 0.0),
            ("<s>".into(), 0.0),
            ("</s>".into(), 0.0),
        ];
        for b in 0u16..=255 {
            pieces.push((format!("<0x{:02X}>", b), -100.0));
        }
        pieces.push(("\u{2581}".into(), -1.0)); // id 259
        let n = pieces.len() as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&16i32.to_le_bytes());
        for (p, s) in &pieces {
            out.extend_from_slice(&s.to_le_bytes());
            out.extend_from_slice(&(p.len() as i32).to_le_bytes());
            out.extend_from_slice(p.as_bytes());
        }
        (out, n)
    }

    #[test]
    fn tokenizer_falls_back_to_bytes_for_unknown_chars() {
        let (bytes, n) = byte_fallback_vocab();
        let t = Tokenizer::from_bytes(&bytes, n).unwrap();
        let ids = t.encode("z", false, false);
        // 'z' = 0x7A = 122 -> 語彙に "z" は無いのでバイトトークン 122+3 = 125
        assert!(
            ids.contains(&125),
            "未知文字はバイトフォールバック: {:?}",
            ids
        );
        // そして decode で元のバイトに戻る
        assert_eq!(t.decode(0, 125), "z");
    }

    /// 語彙が小さくバイトフォールバック領域が無い場合、範囲外 id を作らない。
    /// (作ると forward が TokenOutOfRange で落ちる)
    #[test]
    fn tokenizer_never_emits_out_of_range_ids() {
        let t = Tokenizer::from_bytes(&tok_bytes(PIECES), 8).unwrap();
        for ids in [
            t.encode("z", true, true),
            t.encode("日本語", false, false),
            t.encode("ab", true, false),
        ] {
            for id in ids {
                assert!(
                    (id as usize) < t.vocab_size(),
                    "id {} が語彙 {} の外",
                    id,
                    t.vocab_size()
                );
            }
        }
    }

    #[test]
    fn generate_actually_produces_text_and_counts_work() {
        let e = predictable_engine();
        let limits = ExecutionLimits {
            max_prompt_tokens: 64,
            max_output_tokens: 5,
        };
        let out = e
            .generate("ab", limits, Sampler::deterministic(), 42)
            .unwrap();
        assert_eq!(
            out.text, "bbbbb",
            "argmax が常に 5 なので b が 5 個: {:?}",
            out
        );
        assert_eq!(out.output_tokens, 5);
        assert_eq!(out.stop_reason, StopReason::OutputLimit);
        assert!(
            out.forward_passes >= out.output_tokens,
            "生成トークン数以上の forward を回している = 実際に計算した: {:?}",
            out
        );
    }

    #[test]
    fn generation_is_reproducible_for_a_fixed_seed() {
        let e = predictable_engine();
        let limits = ExecutionLimits {
            max_prompt_tokens: 64,
            max_output_tokens: 4,
        };
        let s = Sampler {
            temperature: 0.8,
            top_p: 0.9,
        };
        let a = e.generate("ab", limits, s, 7).unwrap();
        let b = e.generate("ab", limits, s, 7).unwrap();
        assert_eq!(a, b, "同じ seed なら完全に同じ結果");
    }

    #[test]
    fn generation_respects_a9_limits() {
        let e = predictable_engine();
        let tight = ExecutionLimits {
            max_prompt_tokens: 1,
            max_output_tokens: 5,
        };
        let r = e.generate("ab", tight, Sampler::deterministic(), 1);
        assert!(
            matches!(r, Err(InferenceError::LimitExceeded(_))),
            "{:?}",
            r
        );

        let e2 = predictable_engine();
        let empty = e2.generate(
            "   ",
            ExecutionLimits::default(),
            Sampler::deterministic(),
            1,
        );
        assert!(
            matches!(empty, Err(InferenceError::EmptyPrompt)),
            "{:?}",
            empty
        );
    }

    #[test]
    fn generation_stops_at_context_limit_not_by_panicking() {
        let e = predictable_engine(); // seq_len = 16
        let limits = ExecutionLimits {
            max_prompt_tokens: 64,
            max_output_tokens: 10_000,
        };
        let out = e
            .generate("ab", limits, Sampler::deterministic(), 1)
            .unwrap();
        assert_eq!(out.stop_reason, StopReason::ContextLimit);
        assert!(
            out.forward_passes <= 16,
            "文脈長を超えて回さない: {:?}",
            out
        );
    }

    #[test]
    fn sampler_temperature_zero_is_argmax() {
        let mut rng = Rng::new(1);
        let logits = [0.1f32, 5.0, 0.2];
        assert_eq!(Sampler::deterministic().sample(&logits, &mut rng), 1);
    }

    #[test]
    fn top_p_never_selects_negligible_tokens() {
        let mut rng = Rng::new(99);
        // 事実上トークン 0 に全質量。top_p=0.9 なら常に 0 が返る
        let logits = [20.0f32, -20.0, -20.0, -20.0];
        let s = Sampler {
            temperature: 1.0,
            top_p: 0.9,
        };
        for _ in 0..50 {
            assert_eq!(s.sample(&logits, &mut rng), 0);
        }
    }
}

#[cfg(test)]
mod path_safety_tests {
    use super::*;

    #[test]
    fn safe_model_stem_accepts_ordinary_names() {
        assert_eq!(safe_model_stem("stories15M").unwrap(), "stories15M");
        assert_eq!(
            safe_model_stem("llama-3.2_1b.bin").unwrap(),
            "llama-3.2_1b.bin"
        );
    }

    #[test]
    fn safe_model_stem_rejects_path_traversal() {
        for bad in [
            "../../etc/passwd",
            "..",
            "a/../b",
            "/etc/passwd",
            "a/b",
            "a\\b",
            ".hidden",
            "",
            "モデル",
            "a\0b",
        ] {
            assert!(safe_model_stem(bad).is_err(), "{:?} は拒否されるべき", bad);
        }
    }
}

#[cfg(test)]
mod disk_roundtrip_tests {
    use super::tests::{build_checkpoint, tiny_config};
    use super::*;

    /// ディスク経由の経路を丸ごと通す: ファイル書き出し → `CpuEngine::load`
    /// → 生成。`from_bytes` のテストとは別に、**実際の I/O 経路**を踏む。
    #[test]
    fn loads_from_disk_and_generates() {
        let dir = std::env::temp_dir().join(format!("rope-inference-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let model_path = dir.join("m.bin");
        let tok_path = dir.join("tokenizer.bin");

        // seq_len は既定の 4 では足りない (プロンプトだけで埋まる)。
        // 3 トークン生成させるので余裕を持たせる。
        let c = Config {
            vocab_size: 8,
            seq_len: 16,
            ..tiny_config(false)
        };
        let bytes = build_checkpoint(c, |name, n| match name {
            "emb" => (0..n).map(|i| if i % 4 == 0 { 2.0 } else { 0.0 }).collect(),
            "rms_att" | "rms_ffn" | "rms_final" => vec![1.0; n],
            "wcls" => {
                let mut v = vec![0.0; n];
                v[5 * 4] = 1.0;
                v
            }
            _ => vec![0.0; n],
        });
        std::fs::write(&model_path, &bytes).unwrap();
        std::fs::write(&tok_path, super::tests::tok_bytes(super::tests::PIECES)).unwrap();

        let engine = CpuEngine::load(&model_path, &tok_path, CheckpointLimits::default())
            .expect("ディスクからロードできる");
        let out = engine
            .generate(
                "ab",
                ExecutionLimits {
                    max_prompt_tokens: 64,
                    max_output_tokens: 3,
                },
                Sampler::deterministic(),
                1,
            )
            .expect("生成できる");
        assert_eq!(out.text, "bbb");
        assert!(out.forward_passes > 0, "実際に forward を回している");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 語彙数が食い違うトークナイザは**ロード時に**落ちる
    /// (実行中に範囲外 id が出るより早く気づける)。
    #[test]
    fn mismatched_tokenizer_fails_at_load_not_at_runtime() {
        let dir = std::env::temp_dir().join(format!("rope-inference-mm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let model_path = dir.join("m.bin");
        let tok_path = dir.join("tokenizer.bin");

        let c = Config {
            vocab_size: 8,
            ..tiny_config(false)
        };
        let bytes = build_checkpoint(c, |name, n| match name {
            "rms_att" | "rms_ffn" | "rms_final" => vec![1.0; n],
            _ => vec![0.0; n],
        });
        std::fs::write(&model_path, &bytes).unwrap();
        // 語彙 3 個分しか書かない → モデルの 8 と食い違う
        std::fs::write(
            &tok_path,
            super::tests::tok_bytes(&super::tests::PIECES[..3]),
        )
        .unwrap();

        let r = CpuEngine::load(&model_path, &tok_path, CheckpointLimits::default());
        assert!(
            matches!(r, Err(InferenceError::MalformedCheckpoint(_))),
            "{:?}",
            r.map(|_| "ロードできてしまった")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

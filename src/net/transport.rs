//! ノード間のジョブ転送 — `wire` のフレームを TCP に載せる層。
//!
//! ## 何が本物で、何がまだ無いか
//!
//! - **本物**: TCP 接続、フレーミング、署名による完全性・真正性、TOFU による
//!   鍵の突き合わせ、タイムアウトとサイズ上限 (A9)
//! - **無い**: **暗号化**。中身は平文で流れる。`wire` の署名は
//!   「誰が送ったか」と「途中で書き換えられていないか」を保証するが、
//!   「誰にも読まれない」は保証しない
//!
//! ## なぜ暗号化が無いのに出せるのか / 出せないのか
//!
//! [`docs/SURPLUS_AND_GAPS.md`] §1.1 は「検証済み crate 無しに Noise を
//! 手書きするな」と明示しており、この環境では `snow` も `x25519-dalek` も
//! 追加できない (crates.io が塞がれている)。**手書きはしない。**
//!
//! 代わりに、この層は **既定で平文転送を拒否する**。使うには
//! `ROPE_ALLOW_PLAINTEXT=1` を明示的に立てる必要がある
//! ([`plaintext_allowed`])。判断を利用者に返すのであって、黙って
//! 平文で流すのではない。

use std::io::{self, Read};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use super::wire::{
    read_frame, read_frame_bytes, verify_hello, write_frame, FrameSigner, FrameVerifier, Message,
    WireError, WireProof,
};

/// 書き込みと、結果を待つ側の読み出しに使う上限
/// (A9: 相手が黙り込んでも貸し手が固まらない)。
///
/// **これは「一番遅いフェーズ」の値である。** 推論の結果は大きくなりうるし、
/// 借り手は貸し手が計算し終わるのを待たねばならない。
pub const IO_TIMEOUT: Duration = Duration::from_secs(30);

/// 握手 (`Hello`) と依頼 (`JobRequest`) を待つ時間。**本体よりずっと短くする。**
///
/// 正規の相手はこの 2 つをミリ秒で送る — 待つ理由は計算でも転送量でもなく、
/// ネットワークの往復だけだからである。
/// ここに `IO_TIMEOUT` を使っていた頃は、**接続して何も送らないだけで貸し手を
/// 30 秒占有できた** (`serve_jobs` は直列なので、その間 誰にもサービスできない)。
/// 攻撃コストはほぼゼロだった。
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// 支払いを待つ時間。**本体より短くする。**
///
/// 計算は既に終わっているので、ここで長く待つのは「払う気の無い借り手が
/// 貸し手を 30 秒縛れる」だけの意味しか持たない (A9)。払わない相手は
/// 早く切って次の接続を受ける方がよい。
pub const PAYMENT_TIMEOUT: Duration = Duration::from_secs(5);

/// 同一ピアに 1 プロセスで許す実行回数の既定値 (A9)。
///
/// **粗い上限である。** 本来は消費した時間か計算量で測るべきだが、v1 の貸し手は
/// 直列に 1 件ずつ処理するので、回数は占有時間の近似として使える。
///
/// 🔴 **Sybil には効かない** — 鍵を作り直せば回避できる。これは TOFU (A2) の
/// 限界そのもので、Sybil 耐性は [`docs/V1_SCOPE.md`] が v1 から外した項目。
/// ここで守れるのは「**1 つの鍵が貸し手の稼働時間を丸ごと食う**」ことだけ。
pub const MAX_JOBS_PER_PEER: u32 = 64;

/// 出力トークン 1 個の価格 (sats)。**v1 はプロトコル全体で固定**である。
///
/// ## なぜ「貸し手ごとの価格」ではないのか (Musk ①: 要件を疑う)
///
/// 貸し手ごとに値付けするには、借り手が**繋ぐ前にその価格を知る**手段が要る。
/// v1 にはそれが無い — mDNS の広告にも `Hello` にも価格の欄は無く、足せば
/// ワイヤ形式の版上げになる。そして A8 (設定ゼロ) は、そもそも利用者に
/// 価格を決めさせない方を選ぶ。
///
/// **だから v1 は郵便料金のように一律にする。** 両者が同じ定数を見るので
/// 交渉が要らない。以前は借り手側の `price_for` だけがこの規則を持ち、
/// **貸し手はいくら貰えるのか知らないまま計算していた** (§1.22)。
///
/// 貸し手ごとの価格は v2 の項目。入れるなら mDNS の TXT に載せるのが素直で、
/// ワイヤ形式を触らずに済む。
pub const PRICE_PER_OUTPUT_TOKEN: u64 = 1;

/// この依頼を受けるなら最低いくら必要か (sats)。
///
/// 引数は**実際に生成しうる上限**であって、借り手の希望値ではない。
/// 貸し手は自分の上限で頭打ちにするので、そこで課金しないと過大請求になる。
pub fn required_payment(effective_max_output_tokens: u32) -> u64 {
    PRICE_PER_OUTPUT_TOKEN * effective_max_output_tokens as u64
}

/// 「この鍵は何回走らせたか」を数えるだけの器 (A9)。
///
/// **A2 (TOFU) は「誰か」を見るが「どれだけか」を見ていない。** 信頼した相手が
/// 貸し手の稼働時間を丸ごと食えるなら、信頼判断は資源を守っていない。
/// これはその隙間を埋める最小のもので、判断は [`JobPolicy::accept`] から呼ぶ。
///
/// プロセスが生きている間だけ数える (貸し出しは `max_minutes` で終わる)。
/// 永続化しないのは意図的で、鍵ごとの履歴をディスクに増やす価値が
/// この粗さに見合わないため。
#[derive(Debug, Default)]
pub struct PeerQuota {
    limit: u32,
    served: std::collections::HashMap<String, u32>,
}

impl PeerQuota {
    /// 既定 ([`MAX_JOBS_PER_PEER`]) の上限で作る。
    pub fn new() -> Self {
        Self::with_limit(MAX_JOBS_PER_PEER)
    }

    /// 上限を指定して作る。`0` は「1 件も受けない」を意味する。
    pub fn with_limit(limit: u32) -> Self {
        Self {
            limit,
            served: std::collections::HashMap::new(),
        }
    }

    /// 1 件分を計上する。上限を超えるなら**数えずに** `Err(理由)` を返す。
    ///
    /// 理由の文字列はそのまま `Reject` として借り手に届く — 何が起きたか
    /// 分かるようにしておく (正直さの文化)。
    pub fn charge(&mut self, peer_pubkey: &str) -> Result<(), String> {
        let n = self.served.entry(peer_pubkey.to_string()).or_insert(0);
        if *n >= self.limit {
            return Err(format!(
                "このピアの上限 {} 件に達しています (1 鍵あたり)",
                self.limit
            ));
        }
        *n += 1;
        Ok(())
    }

    /// この鍵に対して既に走らせた件数。
    pub fn served(&self, peer_pubkey: &str) -> u32 {
        self.served.get(peer_pubkey).copied().unwrap_or(0)
    }
}

/// **フレーム 1 つを読み切るまでの合計時間**を締切で縛る `Read` ラッパ (A9)。
///
/// ## なぜ `set_read_timeout` だけでは足りないか
///
/// `set_read_timeout` は **read 1 回ごと**の上限であって、フレーム全体の
/// 締切ではない。上限が 5 秒でも、**500ms おきに 1 バイトずつ送る相手には
/// 一度も発火しない** — 各 read は常に期限内に返るからである。
/// 貸し手は無限に縛られる (`serve_jobs` は直列なので、1 本で稼働時間を
/// 丸ごと潰せる)。古典的な slow-loris である。
///
/// ここは read のたびに「締切までの残り」を計算し、**それをソケットの
/// read タイムアウトとして張り直す**。残りが尽きれば
/// [`io::ErrorKind::TimedOut`] を返す。結果として、フレーム 1 つに
/// かかる合計時間が締切そのもので抑えられる。
///
/// ## なぜ借り手側には使わないか
///
/// 借り手は**貸し手の計算が終わるのを待つ**ので、最初の 1 バイトまでの
/// 時間が原理的に読めない。全体締切を張ると遅いモデルで正当な依頼が切れる。
/// また借り手は自分から繋いだ側で、攻撃対象としての価値が低い。
/// **この非対称は意図であって漏れではない。**
struct DeadlineReader<'a> {
    stream: &'a TcpStream,
    deadline: Instant,
}

impl<'a> DeadlineReader<'a> {
    fn new(stream: &'a TcpStream, budget: Duration) -> Self {
        Self {
            stream,
            deadline: Instant::now() + budget,
        }
    }
}

impl Read for DeadlineReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "フレーム全体の締切を超えました",
            ));
        }
        // `set_read_timeout(0)` は「無制限」ではなく**無効値**として拒否される
        // プラットフォームがある。1ms 未満は 1ms に切り上げる。
        let slice = left.max(Duration::from_millis(1));
        self.stream.set_read_timeout(Some(slice))?;
        // `impl Read for &TcpStream` を使う (&mut を持たなくても読める)
        (&*self.stream).read(buf)
    }
}

#[derive(Debug)]
pub enum TransportError {
    Io(io::Error),
    Wire(WireError),
    /// 相手の名乗りが TOFU と食い違う / 未知の鍵
    UntrustedPeer(String),
    /// 期待しない順序のメッセージ
    Protocol(&'static str),
    /// 相手が依頼を断った
    Rejected(String),
    /// 平文転送が許可されていない
    PlaintextNotAllowed,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "通信エラー: {}", e),
            TransportError::Wire(e) => write!(f, "フレーム不正: {}", e),
            TransportError::UntrustedPeer(k) => write!(f, "信頼していないピア: {}", k),
            TransportError::Protocol(m) => write!(f, "プロトコル違反: {}", m),
            TransportError::Rejected(r) => write!(f, "依頼を断られた: {}", r),
            TransportError::PlaintextNotAllowed => f.write_str(
                "平文転送は既定で無効 (ROPE_ALLOW_PLAINTEXT=1 で明示的に有効化してください)",
            ),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<io::Error> for TransportError {
    fn from(e: io::Error) -> Self {
        TransportError::Io(e)
    }
}
impl From<WireError> for TransportError {
    fn from(e: WireError) -> Self {
        TransportError::Wire(e)
    }
}

/// 平文転送が明示的に許可されているか。
///
/// **既定は false。** 暗号化が無い経路にプロンプトを流すかどうかは
/// 利用者の判断であって、こちらが黙って決めることではない。
pub fn plaintext_allowed() -> bool {
    std::env::var("ROPE_ALLOW_PLAINTEXT")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// 自分の名乗り。
#[derive(Debug, Clone)]
pub struct NodeIdentity {
    pub node_id: String,
    pub pubkey: String,
}

/// 実行結果 (借り手にも貸し手にも同じ形で見える)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executed {
    pub text: String,
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    pub forward_passes: u32,
}

/// 貸し手の受け入れ方針と実行。**A9 の判断はすべてここに集まる。**
pub trait JobPolicy {
    /// この依頼を受けるか。`Err(理由)` なら `Reject` を返す。
    ///
    /// `job_id` を渡すのは、**受けると決めた側がそれを記録できるようにする**ため。
    /// 貸し手が緊急停止した時に「今どのジョブを走らせていたか」が分からないと、
    /// 借り手の escrow を返金できない (`A9→A7`,
    /// `docs/SURPLUS_AND_GAPS.md` §1.10)。
    ///
    /// `max_output_tokens` を渡すのは、**受けるかどうかの判断に必要な仕事量が
    /// それだから**である。これが無いと貸し手は「いくら貰えるのか」を
    /// 知らないまま計算を始めることになる (A9, §1.22)。
    fn accept(
        &self,
        peer_pubkey: &str,
        job_id: &str,
        model: &str,
        prompt: &str,
        max_output_tokens: u32,
        budget_sats: u64,
    ) -> Result<(), String>;

    /// 実際に走らせる。
    fn execute(
        &self,
        model: &str,
        prompt: &str,
        max_output_tokens: u32,
    ) -> Result<Executed, String>;

    /// 支払いを受け取る (A5)。返り値は受け入れた sats。
    ///
    /// **実行の後に呼ばれる。** bearer token は持っていること自体が支払いなので、
    /// 受け取り側は二重使用の記録だけ確認すればよい。
    /// 既定は「受け取らない」— 実装しなければ無償で貸すことになる。
    fn receive_payment(
        &self,
        _peer_pubkey: &str,
        _job_id: &str,
        _proofs: &[WireProof],
    ) -> Result<u64, String> {
        Ok(0)
    }
}

/// 1 件処理した記録 (貸し手側の戻り値)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedJob {
    pub peer_node_id: String,
    pub peer_pubkey: String,
    pub job_id: String,
    /// 受けて実行したか (false = Reject を返した)
    pub accepted: bool,
    pub executed: Option<Executed>,
    /// 受け取れた対価 (sats)。0 なら**無償で計算した**ということ
    pub paid_sats: u64,
}

/// 貸し手: 1 接続を最後まで処理する。
///
/// 手順は固定で、順序違反はその場で切る:
/// 1. `Hello` を受け取り、**その中の鍵で自己署名を検証**する
///    (鍵を持っている証明。信じてよいかは `make_verifier` = TOFU が決める)
/// 2. 自分の `Hello` を返す
/// 3. `JobRequest` を受け取り、`policy.accept` に諮る
/// 4. 通れば `policy.execute` して `JobResult`、駄目なら `Reject`
///
/// ## なぜタイムアウトが 3 つに分かれているか (A9)
///
/// [`HANDSHAKE_TIMEOUT`] (5 秒) が 1・3 を、[`IO_TIMEOUT`] (30 秒) が書き込みを、
/// [`PAYMENT_TIMEOUT`] (5 秒) が支払い待ちを縛る。
/// **1 つにまとめると、最も遅いフェーズに合わせた上限が最も速いフェーズの
/// DoS 窓になる** — 実際、以前は全フェーズが 30 秒で、接続して何も送らないだけで
/// 貸し手を 30 秒止められた (`serve_jobs` は直列)。
///
/// 🔴 **残る穴 (正直な開示)**: `set_read_timeout` は **read 1 回ごと**の上限で
/// あって、フレーム全体の締切ではない。**4 秒おきに 1 バイトずつ送る相手は
/// 依然として貸し手を縛れる** (slow-loris)。塞ぐには接続全体の締切を
/// [`read_frame_bytes`] に通す必要があり、v1 では行っていない
/// (`docs/SURPLUS_AND_GAPS.md` §1.21)。
pub fn serve_connection(
    stream: &mut TcpStream,
    me: &NodeIdentity,
    signer: &dyn FrameSigner,
    make_verifier: &dyn Fn(&str) -> Option<Box<dyn FrameVerifier>>,
    policy: &dyn JobPolicy,
) -> Result<ServedJob, TransportError> {
    if !plaintext_allowed() {
        return Err(TransportError::PlaintextNotAllowed);
    }
    // 読み出しの上限は**フェーズごとに違う** (A9)。ここは握手なので短い方を張る。
    // 書き込みは結果が大きくなりうるので長い方のまま。
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;

    // 1. Hello (自己署名) — 鍵はメッセージの中にある
    //
    // `DeadlineReader` で包むのは、`set_read_timeout` が **read 1 回ごと**の
    // 上限でしかないため。包まないと、1 バイトずつ滴下する相手を止められない。
    let hello_frame = {
        let mut r = DeadlineReader::new(stream, HANDSHAKE_TIMEOUT);
        read_frame_bytes(&mut r)?
    };
    let hello = verify_hello(&hello_frame, make_verifier)?;
    let (peer_node_id, peer_pubkey) = match hello {
        Message::Hello {
            node_id, pubkey, ..
        } => (node_id, pubkey),
        _ => return Err(TransportError::Protocol("最初は Hello でなければならない")),
    };

    // 2. こちらの名乗り
    write_frame(
        stream,
        &Message::Hello {
            node_id: me.node_id.clone(),
            pubkey: me.pubkey.clone(),
            nonce: String::new(),
        },
        signer,
    )?;

    // 3. 依頼。ここから先は相手の鍵で検証する。
    //    読み出しの上限は依然 `HANDSHAKE_TIMEOUT` — 依頼もまた即答すべきフェーズで、
    //    ここで待つ理由は往復の遅延しか無い。
    let verifier = make_verifier(&peer_pubkey)
        .ok_or_else(|| TransportError::UntrustedPeer(peer_pubkey.clone()))?;
    let req = {
        let mut r = DeadlineReader::new(stream, HANDSHAKE_TIMEOUT);
        read_frame(&mut r, verifier.as_ref())?
    };
    let (job_id, model, prompt, max_output_tokens, budget_sats) = match req {
        Message::JobRequest {
            job_id,
            model,
            prompt,
            max_output_tokens,
            budget_sats,
        } => (job_id, model, prompt, max_output_tokens, budget_sats),
        _ => return Err(TransportError::Protocol("Hello の次は JobRequest")),
    };

    // 4. 受けるか判断して、受けるなら走らせる
    let decision = policy
        .accept(
            &peer_pubkey,
            &job_id,
            &model,
            &prompt,
            max_output_tokens,
            budget_sats,
        )
        .and_then(|_| policy.execute(&model, &prompt, max_output_tokens));

    match decision {
        Ok(ex) => {
            write_frame(
                stream,
                &Message::JobResult {
                    job_id: job_id.clone(),
                    text: ex.text.clone(),
                    prompt_tokens: ex.prompt_tokens,
                    output_tokens: ex.output_tokens,
                    forward_passes: ex.forward_passes,
                },
                signer,
            )?;
            // 支払いを待つ。**来なくてもエラーにはしない** — 既に計算は
            // 終わっており、借り手が落ちただけかもしれない。取り損ねた事実を
            // `paid_sats = 0` として返し、判断は呼び出し元に委ねる。
            let mut pay_reader = DeadlineReader::new(stream, PAYMENT_TIMEOUT);
            let paid_sats = match read_frame(&mut pay_reader, verifier.as_ref()) {
                Ok(Message::Payment {
                    job_id: pid,
                    proofs,
                }) if pid == job_id => policy
                    .receive_payment(&peer_pubkey, &job_id, &proofs)
                    .unwrap_or(0),
                _ => 0,
            };
            Ok(ServedJob {
                peer_node_id,
                peer_pubkey,
                job_id,
                accepted: true,
                executed: Some(ex),
                paid_sats,
            })
        }
        Err(reason) => {
            write_frame(
                stream,
                &Message::Reject {
                    job_id: job_id.clone(),
                    reason,
                },
                signer,
            )?;
            Ok(ServedJob {
                peer_node_id,
                peer_pubkey,
                job_id,
                accepted: false,
                executed: None,
                paid_sats: 0,
            })
        }
    }
}

/// 支払いフレームの送達状況。
///
/// bearer token は**送る前にウォレットから消費・永続化される** (at-most-once
/// spend — クラッシュしても同じ proof を二重送信しない)。その代償として、
/// 送信に失敗した proof は**戻せない** (nullifier が既に使用済みと記録される
/// ため、再受領もできない)。だから送達の成否を握り潰さず、呼び出し元に返す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentDelivery {
    /// 支払いを試みなかった (pay が空を返した)
    NotAttempted,
    /// Payment フレームの書き込みに成功した
    Sent,
    /// 書き込みに失敗した — **消費済みの proof は失われた**
    Failed,
}

/// 借り手: 1 件の推論を依頼して結果を受け取る。
///
/// `expected_pubkey` は **mDNS 発見時に相手が広告していた公開鍵** (hex)。
/// `Some` を渡すと、接続先の `Hello` の鍵と一致しない場合に
/// [`TransportError::UntrustedPeer`] で中断する — 発見と接続の間で相手が
/// すり替わる MITM をここで検出する (TOFU のピン留め)。
/// `None` は「初対面をそのまま受け入れる」で、TOFU の初回に相当する。
#[allow(clippy::too_many_arguments)]
pub fn request_job(
    stream: &mut TcpStream,
    me: &NodeIdentity,
    signer: &dyn FrameSigner,
    make_verifier: &dyn Fn(&str) -> Option<Box<dyn FrameVerifier>>,
    expected_pubkey: Option<&str>,
    job_id: &str,
    model: &str,
    prompt: &str,
    max_output_tokens: u32,
    budget_sats: u64,
    pay: &mut dyn FnMut(&Executed) -> Vec<WireProof>,
) -> Result<(Executed, PaymentDelivery), TransportError> {
    if !plaintext_allowed() {
        return Err(TransportError::PlaintextNotAllowed);
    }
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;

    write_frame(
        stream,
        &Message::Hello {
            node_id: me.node_id.clone(),
            pubkey: me.pubkey.clone(),
            nonce: String::new(),
        },
        signer,
    )?;

    // 相手の Hello も自己署名で検証する (対称)
    let their_hello_frame = read_frame_bytes(stream)?;
    let their_hello = verify_hello(&their_hello_frame, make_verifier)?;
    let their_pubkey = match their_hello {
        Message::Hello { pubkey, .. } => pubkey,
        _ => return Err(TransportError::Protocol("相手の Hello が来ない")),
    };
    // TOFU ピン: 発見時に広告されていた鍵と違う相手なら、依頼を送る前に切る。
    // hex は大文字小文字を区別しない (相手側の符号化の癖に依存しない)。
    if let Some(expected) = expected_pubkey {
        if !their_pubkey.eq_ignore_ascii_case(expected) {
            return Err(TransportError::UntrustedPeer(their_pubkey));
        }
    }
    let verifier = make_verifier(&their_pubkey)
        .ok_or_else(|| TransportError::UntrustedPeer(their_pubkey.clone()))?;

    write_frame(
        stream,
        &Message::JobRequest {
            job_id: job_id.to_string(),
            model: model.to_string(),
            prompt: prompt.to_string(),
            max_output_tokens,
            budget_sats,
        },
        signer,
    )?;

    match read_frame(stream, verifier.as_ref())? {
        Message::JobResult {
            job_id: got,
            text,
            prompt_tokens,
            output_tokens,
            forward_passes,
        } => {
            if got != job_id {
                return Err(TransportError::Protocol("job_id が一致しない"));
            }
            let ex = Executed {
                text,
                prompt_tokens,
                output_tokens,
                forward_passes,
            };
            // A5: 結果を受け取ってから払う。**払えなくても結果は返す** —
            // 既に受け取ったものを握り潰しても誰も得をしない。未払いは
            // 貸し手側で `paid_sats = 0` として観測される。
            //
            // `pay` は proof を返す前にウォレットの消費を**永続化しておく**契約
            // (spend → persist → send)。したがってここで送信に失敗した proof は
            // 取り戻せない — 黙って握り潰さず `PaymentDelivery::Failed` で返す。
            let proofs = pay(&ex);
            let delivery = if proofs.is_empty() {
                PaymentDelivery::NotAttempted
            } else {
                match write_frame(
                    stream,
                    &Message::Payment {
                        job_id: job_id.to_string(),
                        proofs,
                    },
                    signer,
                ) {
                    Ok(()) => PaymentDelivery::Sent,
                    Err(_) => PaymentDelivery::Failed,
                }
            };
            Ok((ex, delivery))
        }
        Message::Reject { reason, .. } => Err(TransportError::Rejected(reason)),
        _ => Err(TransportError::Protocol("JobResult か Reject を期待した")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::inference::*;
    use super::super::wire::*;
    use super::*;
    use std::net::{TcpListener, TcpStream};

    pub(super) struct FakeSig(pub u8);
    impl FrameSigner for FakeSig {
        fn sign(&self, msg: &[u8]) -> Vec<u8> {
            let mut acc = [self.0; 8];
            for (i, b) in msg.iter().enumerate() {
                acc[i % 8] ^= *b;
            }
            acc.to_vec()
        }
    }
    impl FrameVerifier for FakeSig {
        fn verify(&self, msg: &[u8], sig: &[u8]) -> bool {
            self.sign(msg) == sig
        }
    }
    /// 鍵 hex "0N" → FakeSig(N) を返す TOFU 相当のルックアップ
    pub(super) fn tofu(pk: &str) -> Option<Box<dyn FrameVerifier>> {
        u8::from_str_radix(pk, 16)
            .ok()
            .map(|n| Box::new(FakeSig(n)) as Box<dyn FrameVerifier>)
    }
    fn tofu_denies_everyone(_pk: &str) -> Option<Box<dyn FrameVerifier>> {
        None
    }

    /// 本物の推論エンジンを積んだ貸し手の方針。
    pub(super) struct RealPolicy {
        pub engine: CpuEngine,
        pub max_prompt: usize,
    }
    impl JobPolicy for RealPolicy {
        fn accept(
            &self,
            _pk: &str,
            _job: &str,
            model: &str,
            prompt: &str,
            _max_out: u32,
            budget: u64,
        ) -> Result<(), String> {
            if model != "demo" {
                return Err(format!("知らないモデル: {}", model));
            }
            if prompt.len() > self.max_prompt {
                return Err("プロンプトが長すぎる".into());
            }
            if budget == 0 {
                return Err("予算 0".into());
            }
            Ok(())
        }
        fn execute(&self, _model: &str, prompt: &str, max_out: u32) -> Result<Executed, String> {
            let limits = ExecutionLimits {
                max_prompt_tokens: 512,
                max_output_tokens: max_out,
            };
            let c = self
                .engine
                .generate(prompt, limits, Sampler::deterministic(), 1)
                .map_err(|e| e.to_string())?;
            Ok(Executed {
                text: c.text,
                prompt_tokens: c.prompt_tokens,
                output_tokens: c.output_tokens,
                forward_passes: c.forward_passes,
            })
        }
    }

    /// 「b」を出し続ける小さな本物のモデル (inference.rs のテストと同じ構成)
    pub(super) fn engine() -> CpuEngine {
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
        let bytes = build_ck(c);
        let tok = tok_bytes();
        CpuEngine {
            model: Model::from_bytes(&bytes, CheckpointLimits::default()).unwrap(),
            tokenizer: Tokenizer::from_bytes(&tok, 8).unwrap(),
        }
    }
    fn build_ck(c: Config) -> Vec<u8> {
        let (dim, hid, lay, voc, seq) = (4usize, 4usize, 1usize, 8usize, 16usize);
        let hs = 2usize;
        let kv = 4usize;
        let q = 4usize;
        let mut secs: Vec<(&str, usize)> = vec![
            ("emb", voc * dim),
            ("rms_att", lay * dim),
            ("wq", lay * dim * q),
            ("wk", lay * dim * kv),
            ("wv", lay * dim * kv),
            ("wo", lay * q * dim),
            ("rms_ffn", lay * dim),
            ("w1", lay * hid * dim),
            ("w2", lay * dim * hid),
            ("w3", lay * hid * dim),
            ("rms_final", dim),
            ("fr", seq * hs / 2),
            ("fi", seq * hs / 2),
            ("wcls", voc * dim),
        ];
        let mut out = Vec::new();
        for v in [
            dim as i32,
            hid as i32,
            lay as i32,
            2i32,
            2i32,
            -(voc as i32),
            seq as i32,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for (name, n) in secs.drain(..) {
            let vals: Vec<f32> = match name {
                "emb" => (0..n).map(|i| if i % 4 == 0 { 2.0 } else { 0.0 }).collect(),
                "rms_att" | "rms_ffn" | "rms_final" => vec![1.0; n],
                "wcls" => {
                    let mut v = vec![0.0; n];
                    v[5 * 4] = 1.0;
                    v
                }
                _ => vec![0.0; n],
            };
            for v in vals {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        let _ = c;
        out
    }
    fn tok_bytes() -> Vec<u8> {
        let pieces: [(&str, f32); 8] = [
            ("<unk>", 0.0),
            ("<s>", 0.0),
            ("</s>", 0.0),
            ("\u{2581}", -1.0),
            ("a", -2.0),
            ("b", -3.0),
            ("ab", 5.0),
            ("\u{2581}a", 4.0),
        ];
        let mut out = Vec::new();
        out.extend_from_slice(&16i32.to_le_bytes());
        for (p, s) in pieces {
            out.extend_from_slice(&s.to_le_bytes());
            out.extend_from_slice(&(p.len() as i32).to_le_bytes());
            out.extend_from_slice(p.as_bytes());
        }
        out
    }

    pub(super) fn with_plaintext<T>(f: impl FnOnce() -> T) -> T {
        std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
        let r = f();
        std::env::remove_var("ROPE_ALLOW_PLAINTEXT");
        r
    }

    /// 実 TCP で、借り手 → 貸し手 → **本物の推論** → 結果 を通す。
    #[test]
    fn a_real_job_crosses_a_real_socket_and_is_really_computed() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();

            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy).unwrap()
            });

            let mut c = TcpStream::connect(addr).unwrap();
            let me = NodeIdentity {
                node_id: "borrower".into(),
                pubkey: "0b".into(),
            };
            let (out, _delivery) = request_job(
                &mut c,
                &me,
                &FakeSig(0x0b),
                &tofu,
                None,
                "job-1",
                "demo",
                "ab",
                4,
                100,
                &mut |_| vec![],
            )
            .unwrap();

            let served = server.join().unwrap();
            assert!(served.accepted);
            assert_eq!(served.peer_node_id, "borrower");
            assert_eq!(
                out.text, "bbbb",
                "貸し手側で実際に推論された結果が返る: {:?}",
                out
            );
            assert!(
                out.forward_passes >= out.output_tokens,
                "計算量が記録されている: {:?}",
                out
            );
            assert_eq!(served.executed.unwrap(), out, "両側が同じ結果を見ている");
        });
    }

    /// TOFU が知らない鍵は拒否される。
    #[test]
    fn an_unknown_peer_is_refused() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu_denies_everyone, &policy)
            });
            let mut c = TcpStream::connect(addr).unwrap();
            let me = NodeIdentity {
                node_id: "borrower".into(),
                pubkey: "0b".into(),
            };
            let _ = request_job(
                &mut c,
                &me,
                &FakeSig(0x0b),
                &tofu,
                None,
                "j",
                "demo",
                "ab",
                4,
                100,
                &mut |_| vec![],
            );
            let r = server.join().unwrap();
            assert!(r.is_err(), "未知の鍵は受け付けない");
        });
    }

    /// 方針が断れば Reject が返り、推論は走らない。
    #[test]
    fn a_rejected_job_is_never_executed() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy).unwrap()
            });
            let mut c = TcpStream::connect(addr).unwrap();
            let me = NodeIdentity {
                node_id: "borrower".into(),
                pubkey: "0b".into(),
            };
            let err = request_job(
                &mut c,
                &me,
                &FakeSig(0x0b),
                &tofu,
                None,
                "j",
                "知らないモデル",
                "ab",
                4,
                100,
                &mut |_| vec![],
            )
            .unwrap_err();
            match err {
                TransportError::Rejected(r) => assert!(r.contains("知らないモデル"), "{r}"),
                o => panic!("Reject を期待: {o}"),
            }
            let served = server.join().unwrap();
            assert!(!served.accepted);
            assert!(served.executed.is_none(), "断った依頼は実行しない");
        });
    }

    /// TOFU ピン: 発見時に広告されていた鍵と違う相手が応答したら、
    /// **依頼を送る前に**切る (発見と接続の間の MITM 検出)。
    #[test]
    fn a_peer_with_a_different_key_than_advertised_is_refused() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            // 貸し手は鍵 "0a" を名乗る
            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                // 借り手が切るのでこちらはエラーで終わる — それで正しい
                let _ = serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy);
            });
            let mut c = TcpStream::connect(addr).unwrap();
            let me = NodeIdentity {
                node_id: "borrower".into(),
                pubkey: "0b".into(),
            };
            // 発見時には "0c" を広告していた、という想定でピンを渡す
            let err = request_job(
                &mut c,
                &me,
                &FakeSig(0x0b),
                &tofu,
                Some("0c"),
                "j",
                "demo",
                "ab",
                4,
                100,
                &mut |_| vec![],
            )
            .unwrap_err();
            match err {
                TransportError::UntrustedPeer(k) => {
                    assert_eq!(k, "0a", "実際に名乗られた鍵が報告される")
                }
                o => panic!("UntrustedPeer を期待: {o}"),
            }
            // 先にソケットを閉じる — 開いたまま join すると、貸し手側が
            // JobRequest 待ちのタイムアウト (30 秒) を満了するまで返らない
            drop(c);
            let _ = server.join();
        });
    }

    /// TOFU ピン: 広告どおりの鍵なら通る (大文字小文字は区別しない)。
    #[test]
    fn a_matching_advertised_key_passes_the_pin() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy).unwrap()
            });
            let mut c = TcpStream::connect(addr).unwrap();
            let me = NodeIdentity {
                node_id: "borrower".into(),
                pubkey: "0b".into(),
            };
            let (out, _) = request_job(
                &mut c,
                &me,
                &FakeSig(0x0b),
                &tofu,
                Some("0A"), // 大文字で渡しても一致する
                "j",
                "demo",
                "ab",
                4,
                100,
                &mut |_| vec![],
            )
            .unwrap();
            assert_eq!(out.text, "bbbb");
            // 同上: 開いたままだと貸し手が支払い待ち (5 秒) を満了してしまう
            drop(c);
            let _ = server.join();
        });
    }

    /// 既定では平文転送を拒否する。
    #[test]
    fn plaintext_is_refused_by_default() {
        std::env::remove_var("ROPE_ALLOW_PLAINTEXT");
        assert!(!plaintext_allowed());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let _ = listener.accept();
        });
        let mut c = TcpStream::connect(addr).unwrap();
        let me = NodeIdentity {
            node_id: "b".into(),
            pubkey: "0b".into(),
        };
        let r = request_job(
            &mut c,
            &me,
            &FakeSig(1),
            &tofu,
            None,
            "j",
            "demo",
            "x",
            1,
            1,
            &mut |_| vec![],
        );
        assert!(
            matches!(r, Err(TransportError::PlaintextNotAllowed)),
            "既定で拒否する"
        );
    }
}

#[cfg(test)]
mod payment_tests {
    use super::tests::*;
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// 受け取った sats を記録するだけの方針。
    struct PayingPolicy {
        inner: RealPolicy,
        received: &'static AtomicU64,
    }
    impl JobPolicy for PayingPolicy {
        fn accept(
            &self,
            pk: &str,
            j: &str,
            m: &str,
            p: &str,
            n: u32,
            b: u64,
        ) -> Result<(), String> {
            self.inner.accept(pk, j, m, p, n, b)
        }
        fn execute(&self, m: &str, p: &str, n: u32) -> Result<Executed, String> {
            self.inner.execute(m, p, n)
        }
        fn receive_payment(
            &self,
            _pk: &str,
            _job: &str,
            proofs: &[WireProof],
        ) -> Result<u64, String> {
            let total: u64 = proofs.iter().map(|p| p.amount_sats).sum();
            self.received.store(total, Ordering::SeqCst);
            Ok(total)
        }
    }

    static RECEIVED: AtomicU64 = AtomicU64::new(0);

    /// **A5: 計算の対価が実際に相手へ渡る。**
    /// 借り手は結果を受け取ってから bearer token を送り、貸し手はそれを数える。
    #[test]
    fn payment_actually_crosses_the_wire_after_the_job() {
        with_plaintext(|| {
            RECEIVED.store(0, Ordering::SeqCst);
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();

            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = PayingPolicy {
                    inner: RealPolicy {
                        engine: engine(),
                        max_prompt: 4096,
                    },
                    received: &RECEIVED,
                };
                serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy).unwrap()
            });

            let mut c = TcpStream::connect(addr).unwrap();
            let me = NodeIdentity {
                node_id: "borrower".into(),
                pubkey: "0b".into(),
            };
            let mut paid = |_ex: &Executed| {
                vec![
                    WireProof {
                        id: "p1".into(),
                        amount_sats: 8,
                        mint_id: "mint1".into(),
                        keyset_id: "ks".into(),
                        secret: "s".into(),
                        c: "02".into(),
                        nullifier: "n1".into(),
                    },
                    WireProof {
                        id: "p2".into(),
                        amount_sats: 2,
                        mint_id: "mint1".into(),
                        keyset_id: "ks".into(),
                        secret: "s2".into(),
                        c: "03".into(),
                        nullifier: "n2".into(),
                    },
                ]
            };
            let (out, delivery) = request_job(
                &mut c,
                &me,
                &FakeSig(0x0b),
                &tofu,
                None,
                "job-p",
                "demo",
                "ab",
                4,
                100,
                &mut paid,
            )
            .unwrap();

            let served = server.join().unwrap();
            assert_eq!(out.text, "bbbb");
            assert_eq!(
                delivery,
                PaymentDelivery::Sent,
                "支払いは実際に送達されたと報告される"
            );
            assert_eq!(served.paid_sats, 10, "10 sats を受け取った: {:?}", served);
            assert_eq!(RECEIVED.load(Ordering::SeqCst), 10);
        });
    }

    /// **貸し手は同じモデルを 2 度読み込まない。**
    ///
    /// ソクラテス問答で見つけた実バグの回帰テスト:
    /// 「2 件目のジョブが来たとき、モデルはどこから来るのか?」に対し、
    /// 実装は**毎回ディスクから読み直していた**。7B 級なら 1 リクエストごとに
    /// 数十 GB の再読み込みで、A9 (貸し手の資源保護) に反する。
    /// テストが 1 接続しか張っていなかったので長く気づかなかった。
    ///
    /// ここでは `JobPolicy` にロード回数を数えさせ、**2 件連続で処理しても
    /// ロードは 1 回**であることを固定する。
    #[test]
    fn a_lender_loads_the_same_model_only_once_across_jobs() {
        use std::sync::atomic::{AtomicU32, Ordering};

        struct CountingPolicy {
            inner: RealPolicy,
            loaded: AtomicU32,
            current: std::cell::RefCell<Option<String>>,
        }
        impl JobPolicy for CountingPolicy {
            fn accept(
                &self,
                pk: &str,
                j: &str,
                m: &str,
                p: &str,
                n: u32,
                b: u64,
            ) -> Result<(), String> {
                self.inner.accept(pk, j, m, p, n, b)
            }
            fn execute(&self, m: &str, p: &str, n: u32) -> Result<Executed, String> {
                // 本番の `LocalPolicy::execute` と同じ判断:
                // 「保持しているモデルと違うときだけ読む」
                let mut cur = self.current.borrow_mut();
                if cur.as_deref() != Some(m) {
                    self.loaded.fetch_add(1, Ordering::SeqCst);
                    *cur = Some(m.to_string());
                }
                drop(cur);
                self.inner.execute(m, p, n)
            }
        }

        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();

            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let policy = CountingPolicy {
                    inner: RealPolicy {
                        engine: engine(),
                        max_prompt: 4096,
                    },
                    loaded: AtomicU32::new(0),
                    current: std::cell::RefCell::new(None),
                };
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                // **2 接続を直列に処理する** (本番の serve ループと同じ形)
                for _ in 0..2 {
                    let (mut s, _) = listener.accept().unwrap();
                    serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy).unwrap();
                }
                policy.loaded.load(Ordering::SeqCst)
            });

            for i in 0..2 {
                let mut c = TcpStream::connect(addr).unwrap();
                let me = NodeIdentity {
                    node_id: "borrower".into(),
                    pubkey: "0b".into(),
                };
                let (out, _) = request_job(
                    &mut c,
                    &me,
                    &FakeSig(0x0b),
                    &tofu,
                    None,
                    &format!("job-{i}"),
                    "demo",
                    "ab",
                    4,
                    100,
                    &mut |_| vec![],
                )
                .unwrap();
                assert_eq!(out.text, "bbbb", "{i} 件目も計算される");
                drop(c);
            }

            let loads = server.join().unwrap();
            assert_eq!(
                loads, 1,
                "同じモデルの 2 ジョブでロードは 1 回であるべき (実際 {loads} 回)"
            );
        });
    }

    /// 払わない借り手でも、貸し手は落ちずに `paid_sats = 0` として記録する。
    #[test]
    fn an_unpaid_job_is_recorded_not_fatal() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy).unwrap()
            });
            {
                let mut c = TcpStream::connect(addr).unwrap();
                let me = NodeIdentity {
                    node_id: "borrower".into(),
                    pubkey: "0b".into(),
                };
                let (_ex, delivery) = request_job(
                    &mut c,
                    &me,
                    &FakeSig(0x0b),
                    &tofu,
                    None,
                    "j",
                    "demo",
                    "ab",
                    4,
                    100,
                    &mut |_| vec![],
                )
                .unwrap();
                assert_eq!(
                    delivery,
                    PaymentDelivery::NotAttempted,
                    "空の支払いは「試みなかった」として報告される"
                );
                // ここで c が drop され、貸し手側の読み出しは EOF で即座に返る
            }
            let served = server.join().unwrap();
            assert!(served.accepted, "計算はした");
            assert_eq!(served.paid_sats, 0, "対価は取れなかったと記録する");
        });
    }
}

/// 貸し手の資源そのものを守れているか (A9) — 計算でも金銭でもなく、
/// **時間**を食う攻撃に対する上限のテスト。
#[cfg(test)]
mod resource_tests {
    use super::tests::*;
    use super::*;
    use std::net::{TcpListener, TcpStream};

    /// **黙って繋ぐだけの借り手は、貸し手を長く縛れない** (A9)。
    ///
    /// 以前は握手にも `IO_TIMEOUT` (30 秒) を使っていたので、接続して何も
    /// 送らないだけで貸し手を 30 秒占有できた。`serve_jobs` は直列なので、
    /// これを繰り返すと貸し手は誰にもサービスできなくなる。
    ///
    /// 上限を `HANDSHAKE_TIMEOUT` (5 秒) にしたことを**実測で**確かめる。
    /// 判定を 10 秒に置くのは、5 秒ちょうどを要求するとタイマの粒度や
    /// 遅いマシンで揺れるため — ここで見たいのは「30 秒待たない」ことである。
    #[test]
    fn a_silent_borrower_cannot_hold_the_lender_for_thirty_seconds() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                let started = std::time::Instant::now();
                let r = serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy);
                (r.is_err(), started.elapsed())
            });

            // 繋ぐだけ。1 バイトも送らず、切りもしない。
            let _victim = TcpStream::connect(addr).unwrap();
            let (errored, waited) = server.join().unwrap();

            assert!(errored, "何も送らない相手はエラーとして切られる");
            assert!(
                waited < Duration::from_secs(10),
                "握手の上限は HANDSHAKE_TIMEOUT ({:?}) の側であるべきだが {:?} 待った",
                HANDSHAKE_TIMEOUT,
                waited
            );
            // `_victim` はここで drop される
        });
    }

    /// 同一の鍵が貸し手の稼働時間を丸ごと食えないこと (A9)。
    ///
    /// **鍵ごとに独立**であることも同時に見る — 1 人が使い切っても
    /// 他のピアは影響を受けない。
    #[test]
    fn one_peer_cannot_consume_the_whole_lender() {
        let mut q = PeerQuota::with_limit(2);
        assert_eq!(q.served("0a"), 0);
        assert!(q.charge("0a").is_ok(), "1 件目は通る");
        assert!(q.charge("0a").is_ok(), "2 件目も通る");
        assert_eq!(q.served("0a"), 2);

        let refused = q.charge("0a").unwrap_err();
        assert!(
            refused.contains("上限"),
            "理由が借り手に伝わる形になっている: {}",
            refused
        );
        assert_eq!(q.served("0a"), 2, "拒否した分は数えない");

        assert!(q.charge("0b").is_ok(), "別の鍵は独立に数える");
        assert_eq!(q.served("0b"), 1);

        // 上限 0 は「1 件も受けない」— 既定値を 0 にすれば貸出を止められる
        let mut none = PeerQuota::with_limit(0);
        assert!(none.charge("0a").is_err());
    }

    /// **払えない依頼は、計算する前に断る** (A9, §1.22)。
    ///
    /// 以前は貸し手が「いくら貰えるのか」を知らないまま計算していた —
    /// `accept` に仕事量 (`max_output_tokens`) が渡っていなかったため。
    /// 予算 0 でも 1 sat でも、貸し手はまず計算し、その後で来なかった支払いを
    /// `paid_sats = 0` と記録するだけだった。
    #[test]
    fn an_underfunded_job_is_refused_before_any_computation() {
        /// 価格だけを見る方針 (`main.rs::LocalPolicy` と同じ判断を、
        /// エンジン無しで再現する)。**実行されたら記録に残る。**
        struct PricedPolicy {
            max_output_tokens: u32,
            executed: std::cell::Cell<bool>,
        }
        impl JobPolicy for PricedPolicy {
            fn accept(
                &self,
                _pk: &str,
                _j: &str,
                _m: &str,
                _p: &str,
                max_out: u32,
                budget: u64,
            ) -> Result<(), String> {
                let capped = max_out.min(self.max_output_tokens);
                let required = required_payment(capped);
                if budget < required {
                    return Err(format!("予算が足りません: {} sats 必要", required));
                }
                Ok(())
            }
            fn execute(&self, _m: &str, _p: &str, n: u32) -> Result<Executed, String> {
                self.executed.set(true);
                Ok(Executed {
                    text: "x".into(),
                    prompt_tokens: 1,
                    output_tokens: n,
                    forward_passes: n,
                })
            }
        }

        // 貸し手は最大 32 トークン出す → 32 sats 要る。借り手は 5 sats しか出さない。
        assert_eq!(required_payment(32), 32 * PRICE_PER_OUTPUT_TOKEN);

        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = PricedPolicy {
                    max_output_tokens: 32,
                    executed: std::cell::Cell::new(false),
                };
                let served = serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy).unwrap();
                (served, policy.executed.get())
            });

            let mut c = TcpStream::connect(addr).unwrap();
            let me = NodeIdentity {
                node_id: "borrower".into(),
                pubkey: "0b".into(),
            };
            let err = request_job(
                &mut c,
                &me,
                &FakeSig(0x0b),
                &tofu,
                None,
                "j",
                "demo",
                "ab",
                100, // 100 欲しいが貸し手は 32 で頭打ちにする
                5,   // 5 sats しか出さない
                &mut |_| vec![],
            )
            .unwrap_err();

            match err {
                // 断る理由に**必要額**が入っている — 借り手は次に正しい額を出せる。
                // これで価格交渉は成立する。**新しいメッセージ型は要らなかった。**
                TransportError::Rejected(r) => {
                    assert!(r.contains("32"), "必要額が理由に入る: {r}")
                }
                o => panic!("Reject を期待: {o}"),
            }

            let (served, executed) = server.join().unwrap();
            assert!(!served.accepted);
            assert!(
                !executed,
                "**1 度も計算していない** — これが守りたかったこと"
            );
        });
    }
    /// **1 バイトずつ滴下する借り手も、貸し手を縛れない** (A9, §1.21)。
    ///
    /// これは `a_silent_borrower_...` では捕まらない攻撃である。黙る相手は
    /// `set_read_timeout` で切れるが、**500ms おきに 1 バイト送る相手は各 read が
    /// 常に期限内に返るので、1 回ごとの上限は一度も発火しない** —
    /// 貸し手は無限に縛られる (古典的な slow-loris)。
    ///
    /// `DeadlineReader` がフレーム全体の締切を張ることで塞いだ。
    ///
    /// **下限も見る**のが肝心 — 3 秒未満で返ったなら「締切が効いた」のではなく
    /// 「別の理由で即失敗した」だけで、このテストは何も確かめていないことになる。
    #[test]
    fn a_byte_dripping_borrower_cannot_hold_the_lender_open() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                let started = std::time::Instant::now();
                let r = serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy);
                (r.is_err(), started.elapsed())
            });

            // 滴下する借り手。**本物の Hello フレームを 1 バイトずつ送る。**
            //
            // 🔴 ここは最初「魔法バイト + 0 の羅列」にしていた。それだと
            // ヘッダ 11 バイトが揃った時点で `UnsupportedVersion` になって
            // 5.5 秒で返り、**締切が効いていなくてもテストが通ってしまう**。
            // 実際そうなっていたのを変異検査で見つけた (§1.21)。
            // 正しいフレームなら貸し手は最後まで読もうとするので、
            // **止まる理由は締切しか無くなる。**
            let drip = std::thread::spawn(move || {
                use std::io::Write;
                let frame = Message::Hello {
                    node_id: "borrower".into(),
                    pubkey: "0b".into(),
                    nonce: String::new(),
                }
                .encode(&FakeSig(0x0b))
                .expect("encode");
                // 500ms × フレーム長 ≫ HANDSHAKE_TIMEOUT。締切が無ければ
                // 貸し手は下の 15 秒上限まで付き合わされる。
                assert!(
                    frame.len() as u64 * 500 > 15_000,
                    "フレームが短すぎて滴下の意味が無い ({} バイト)",
                    frame.len()
                );
                let mut c = TcpStream::connect(addr).unwrap();
                let began = std::time::Instant::now();
                for b in frame {
                    if began.elapsed() >= Duration::from_secs(15) {
                        break; // テストが暴走しないための保険
                    }
                    if c.write_all(&[b]).is_err() || c.flush().is_err() {
                        break; // 貸し手が切った = 締切が効いた
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
            });

            let (errored, waited) = server.join().unwrap();
            let _ = drip.join();

            assert!(errored, "滴下する相手はエラーとして切られる");
            assert!(
                waited < Duration::from_secs(10),
                "フレーム全体の締切が効いていない — {:?} 待った \
                 (1 回ごとの上限しか無いと、ここは 15 秒でも終わらない)",
                waited
            );
            // 締切 (5 秒) の付近で返っているはず。ヘッダ拒否や EOF で
            // 返っているなら、この幅から外れる。
            assert!(
                waited < HANDSHAKE_TIMEOUT + Duration::from_secs(2),
                "締切 {:?} の付近で返るべきだが {:?} 待った",
                HANDSHAKE_TIMEOUT,
                waited
            );
            assert!(
                waited > Duration::from_secs(3),
                "3 秒未満で返ったのは締切ではなく別の理由 — テストが何も \
                 確かめていない ({:?})",
                waited
            );
        });
    }
}

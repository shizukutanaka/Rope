//! A1 — LAN のピアを実際に見つける (mDNS / DNS-SD)。
//!
//! **これは実際に UDP マルチキャストを送受信するコードであって、
//! 状態機械のモデルではない** (`docs/FIRST_PRINCIPLES_AUDIT.md` の A1)。
//!
//! ## なぜ依存ゼロなのか
//!
//! [`docs/P2P_IMPLEMENTATION_READINESS.md`] は Iroh / libp2p を検討していた。
//! しかし **LAN 発見に必要なのは DNS-SD のワイヤ形式と UDP マルチキャストだけ**で、
//! どちらも `std::net` で足りる。NAT 越え・リレー・QUIC が要るなら Iroh だが、
//! [`docs/V1_SCOPE.md`] は **v1 を LAN のみに縮めた** — そこは要らない。
//!
//! ## 何をして、何をしないか
//!
//! - **する**: `_rope._tcp.local` の広告と探索、ピアの ID・鍵指紋・ポートの取得
//! - **しない**: **暗号化も認証もしない**。mDNS は平文のブロードキャストであり、
//!   それは AirDrop / Chromecast / ネットワークプリンタと同じ性質のもの。
//!   **ジョブやプロンプトはここを通らない** — 発見の後に来る Noise ハンドシェイク
//!   (未実装) が担当する。同一 LAN の第三者に分かるのは
//!   「このホストが rope を動かしている」という事実と公開鍵指紋だけ。
//!
//! ## 信用しない入力
//!
//! 受信パケットは **LAN の誰でも送れる**。パーサは以下を守る:
//! - 全ての読み出しが境界検査付き (`panic` しない)
//! - 名前圧縮ポインタのループを回数上限で打ち切る
//! - レコード数・名前長・文字列長に上限
//! - 解釈できないレコードは**捨てるだけ**でエラーにしない (相互運用のため)

use std::net::Ipv4Addr;

/// mDNS のマルチキャストアドレスとポート (RFC 6762)
pub const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const MDNS_PORT: u16 = 5353;

/// Rope の DNS-SD サービス種別
pub const SERVICE: &str = "_rope._tcp.local";

/// 圧縮ポインタを辿る上限。悪意あるパケットの無限ループを防ぐ。
const MAX_POINTER_JUMPS: usize = 16;
/// 1 パケットで受理するレコード数の上限。
const MAX_RECORDS: usize = 64;
/// ラベル 1 個の最大長 (RFC 1035)
const MAX_LABEL: usize = 63;
/// 名前全体の最大長 (RFC 1035)
const MAX_NAME: usize = 255;

// ============================================================================
// エラー
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdnsError {
    /// パケットが途中で終わっている
    Truncated,
    /// 名前が長すぎる / ラベルが長すぎる
    NameTooLong,
    /// 圧縮ポインタのループ
    PointerLoop,
    /// レコード数が上限超過
    TooManyRecords,
}

impl std::fmt::Display for MdnsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            MdnsError::Truncated => "パケットが途中で終わっている",
            MdnsError::NameTooLong => "名前が長すぎる",
            MdnsError::PointerLoop => "名前圧縮ポインタがループしている",
            MdnsError::TooManyRecords => "レコード数が多すぎる",
        };
        f.write_str(s)
    }
}

impl std::error::Error for MdnsError {}

// ============================================================================
// ワイヤ形式
// ============================================================================

pub const TYPE_A: u16 = 1;
pub const TYPE_PTR: u16 = 12;
pub const TYPE_TXT: u16 = 16;
pub const TYPE_SRV: u16 = 33;
pub const CLASS_IN: u16 = 1;

/// 境界検査付きの読み出しカーソル。**`panic` しない**のが唯一の設計目標。
struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Cursor<'a> {
        Cursor { buf, pos: 0 }
    }
    fn u8(&mut self) -> Result<u8, MdnsError> {
        let v = *self.buf.get(self.pos).ok_or(MdnsError::Truncated)?;
        self.pos += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, MdnsError> {
        let hi = self.u8()?;
        let lo = self.u8()?;
        Ok(u16::from_be_bytes([hi, lo]))
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], MdnsError> {
        let end = self.pos.checked_add(n).ok_or(MdnsError::Truncated)?;
        let s = self.buf.get(self.pos..end).ok_or(MdnsError::Truncated)?;
        self.pos = end;
        Ok(s)
    }
}

/// 名前をデコードする。圧縮ポインタ (0xC0) に対応し、ループを打ち切る。
fn decode_name(buf: &[u8], start: usize) -> Result<(String, usize), MdnsError> {
    let mut labels: Vec<String> = Vec::new();
    let mut pos = start;
    let mut jumps = 0usize;
    // ポインタを踏む前の位置 (呼び出し元が次に読むべき位置)
    let mut after: Option<usize> = None;
    let mut total = 0usize;

    loop {
        let len = *buf.get(pos).ok_or(MdnsError::Truncated)? as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        if len & 0xC0 == 0xC0 {
            // 圧縮ポインタ: 上位 2 ビットを落とした 14 ビットオフセット
            let b2 = *buf.get(pos + 1).ok_or(MdnsError::Truncated)? as usize;
            let target = ((len & 0x3F) << 8) | b2;
            if after.is_none() {
                after = Some(pos + 2);
            }
            jumps += 1;
            if jumps > MAX_POINTER_JUMPS {
                return Err(MdnsError::PointerLoop);
            }
            if target >= buf.len() {
                return Err(MdnsError::Truncated);
            }
            pos = target;
            continue;
        }
        if len > MAX_LABEL {
            return Err(MdnsError::NameTooLong);
        }
        total += len + 1;
        if total > MAX_NAME {
            return Err(MdnsError::NameTooLong);
        }
        let s = buf
            .get(pos + 1..pos + 1 + len)
            .ok_or(MdnsError::Truncated)?;
        labels.push(String::from_utf8_lossy(s).into_owned());
        pos += 1 + len;
    }

    Ok((labels.join("."), after.unwrap_or(pos)))
}

/// 名前をエンコードする (圧縮しない — 相手は必ず解ける)。
fn encode_name(out: &mut Vec<u8>, name: &str) {
    for label in name.split('.') {
        if label.is_empty() {
            continue;
        }
        let b = label.as_bytes();
        let n = b.len().min(MAX_LABEL);
        out.push(n as u8);
        out.extend_from_slice(&b[..n]);
    }
    out.push(0);
}

/// 受信したレコードのうち、Rope が解釈するもの。
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Ptr {
        name: String,
        target: String,
    },
    Srv {
        name: String,
        port: u16,
        target: String,
    },
    Txt {
        name: String,
        pairs: Vec<(String, String)>,
    },
    A {
        name: String,
        addr: Ipv4Addr,
    },
    /// 解釈しないレコード (捨てるが、パースは通ったという記録)
    Other {
        name: String,
        rtype: u16,
    },
}

/// 受信した問い合わせ 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub name: String,
    pub qtype: u16,
    /// QU ビット (RFC 6762 §5.4): ユニキャストでの応答を要求している
    pub unicast: bool,
}

/// パース済み mDNS メッセージ。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Message {
    pub is_response: bool,
    pub questions: Vec<Question>,
    pub records: Vec<Record>,
}

impl Message {
    /// 受信バイト列をパースする。**LAN の誰でも送れる入力**として扱う。
    pub fn decode(buf: &[u8]) -> Result<Message, MdnsError> {
        let mut c = Cursor::new(buf);
        let _id = c.u16()?;
        let flags = c.u16()?;
        let qd = c.u16()? as usize;
        let an = c.u16()? as usize;
        let ns = c.u16()? as usize;
        let ar = c.u16()? as usize;
        let total = an.saturating_add(ns).saturating_add(ar);
        if qd > MAX_RECORDS || total > MAX_RECORDS {
            return Err(MdnsError::TooManyRecords);
        }

        let mut msg = Message {
            is_response: flags & 0x8000 != 0,
            ..Default::default()
        };

        for _ in 0..qd {
            let (name, next) = decode_name(buf, c.pos)?;
            c.pos = next;
            let qtype = c.u16()?;
            let qclass = c.u16()?;
            msg.questions.push(Question {
                name,
                qtype,
                unicast: qclass & 0x8000 != 0,
            });
        }

        for _ in 0..total {
            let (name, next) = decode_name(buf, c.pos)?;
            c.pos = next;
            let rtype = c.u16()?;
            let _class = c.u16()?;
            let _ttl = c.u16()?;
            let _ttl2 = c.u16()?;
            let rdlen = c.u16()? as usize;
            let rdstart = c.pos;
            let rdata = c.bytes(rdlen)?;

            let rec = match rtype {
                TYPE_PTR => {
                    let (target, _) = decode_name(buf, rdstart)?;
                    Record::Ptr { name, target }
                }
                TYPE_SRV => {
                    if rdlen < 7 {
                        Record::Other { name, rtype }
                    } else {
                        let port = u16::from_be_bytes([rdata[4], rdata[5]]);
                        let (target, _) = decode_name(buf, rdstart + 6)?;
                        Record::Srv { name, port, target }
                    }
                }
                TYPE_TXT => Record::Txt {
                    name,
                    pairs: parse_txt(rdata),
                },
                TYPE_A => {
                    if rdlen != 4 {
                        Record::Other { name, rtype }
                    } else {
                        Record::A {
                            name,
                            addr: Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]),
                        }
                    }
                }
                _ => Record::Other { name, rtype },
            };
            msg.records.push(rec);
        }

        Ok(msg)
    }
}

fn parse_txt(rdata: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < rdata.len() {
        let len = rdata[i] as usize;
        i += 1;
        if len == 0 || i + len > rdata.len() {
            break;
        }
        let s = String::from_utf8_lossy(&rdata[i..i + len]).into_owned();
        i += len;
        match s.split_once('=') {
            Some((k, v)) => out.push((k.to_string(), v.to_string())),
            None => out.push((s, String::new())),
        }
    }
    out
}

// ============================================================================
// Rope が広告する内容
// ============================================================================

/// LAN に出す自分の情報。**秘密は入れない** — 平文で飛ぶ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advertisement {
    /// インスタンス名 (ホスト名相当)。DNS-SD のラベルとして使う
    pub instance: String,
    /// 待ち受けポート
    pub port: u16,
    /// ノード ID (公開情報)
    pub node_id: String,
    /// 公開鍵指紋 (TOFU の照合用。公開情報)
    pub fingerprint: String,
    /// 静的公開鍵の hex (公開情報)。TOFU はこれを鍵として突き合わせる
    pub pubkey: String,
}

impl Advertisement {
    /// PTR + SRV + TXT を 1 つの応答パケットに組み立てる。
    pub fn to_response(&self) -> Vec<u8> {
        let instance_fqdn = format!("{}.{}", sanitize_label(&self.instance), SERVICE);
        let host = format!("{}.local", sanitize_label(&self.instance));

        let mut out = Vec::new();
        out.extend_from_slice(&0u16.to_be_bytes()); // id
        out.extend_from_slice(&0x8400u16.to_be_bytes()); // response + authoritative
        out.extend_from_slice(&0u16.to_be_bytes()); // qdcount
        out.extend_from_slice(&3u16.to_be_bytes()); // ancount (PTR + SRV + TXT)
        out.extend_from_slice(&0u16.to_be_bytes()); // nscount
        out.extend_from_slice(&0u16.to_be_bytes()); // arcount

        // PTR: _rope._tcp.local -> <instance>._rope._tcp.local
        push_record(&mut out, SERVICE, TYPE_PTR, |rd| {
            encode_name(rd, &instance_fqdn)
        });
        // SRV: <instance>._rope._tcp.local -> port, host
        push_record(&mut out, &instance_fqdn, TYPE_SRV, |rd| {
            rd.extend_from_slice(&0u16.to_be_bytes()); // priority
            rd.extend_from_slice(&0u16.to_be_bytes()); // weight
            rd.extend_from_slice(&self.port.to_be_bytes());
            encode_name(rd, &host);
        });
        // TXT: id と指紋
        push_record(&mut out, &instance_fqdn, TYPE_TXT, |rd| {
            push_txt(rd, &format!("id={}", self.node_id));
            push_txt(rd, &format!("fp={}", self.fingerprint));
            push_txt(rd, &format!("pk={}", self.pubkey));
        });
        out
    }

    /// `_rope._tcp.local` の PTR 問い合わせパケット。
    pub fn query() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0u16.to_be_bytes()); // id
        out.extend_from_slice(&0u16.to_be_bytes()); // flags: standard query
        out.extend_from_slice(&1u16.to_be_bytes()); // qdcount
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        encode_name(&mut out, SERVICE);
        out.extend_from_slice(&TYPE_PTR.to_be_bytes());
        // QU ビット (RFC 6762 §5.4): ユニキャストでの応答を要求する。
        // これにより **ポート 5353 を専有せずに** 一発問い合わせができる —
        // avahi / mDNSResponder が既に 5353 を握っていても共存できる。
        out.extend_from_slice(&(CLASS_IN | 0x8000).to_be_bytes());
        out
    }
}

fn push_txt(rd: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(255);
    rd.push(n as u8);
    rd.extend_from_slice(&b[..n]);
}

fn push_record(out: &mut Vec<u8>, name: &str, rtype: u16, rdata: impl FnOnce(&mut Vec<u8>)) {
    encode_name(out, name);
    out.extend_from_slice(&rtype.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    out.extend_from_slice(&120u32.to_be_bytes()); // TTL 120s
    let mut rd = Vec::new();
    rdata(&mut rd);
    out.extend_from_slice(&(rd.len() as u16).to_be_bytes());
    out.extend_from_slice(&rd);
}

/// DNS ラベルに使えない文字を落とす。
///
/// ホスト名は環境由来なので、そのまま流すとパケットを壊しうる。
fn sanitize_label(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(MAX_LABEL)
        .collect();
    if cleaned.is_empty() {
        "rope".to_string()
    } else {
        cleaned
    }
}

// ============================================================================
// 発見結果
// ============================================================================

/// 1 台のピア (mDNS 由来)。**この情報は検証されていない** — LAN の誰でも名乗れる。
/// TOFU の照合は `core::pair` 側の仕事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdnsPeer {
    pub instance: String,
    pub node_id: String,
    pub fingerprint: String,
    /// 相手が名乗った静的公開鍵。**検証されていない** — TOFU の突き合わせ用
    pub pubkey: String,
    pub port: u16,
    pub addr: Option<Ipv4Addr>,
}

/// 応答パケット群からピアを組み立てる。
///
/// PTR/SRV/TXT/A は別レコードなので名前で突き合わせる。
/// **揃っていないものは捨てる** — 半端な情報でピアを名乗らせない。
pub fn peers_from_message(msg: &Message, from: Option<Ipv4Addr>) -> Vec<MdnsPeer> {
    let mut out = Vec::new();
    for rec in &msg.records {
        let (instance_fqdn, port) = match rec {
            Record::Srv { name, port, .. } => (name.clone(), *port),
            _ => continue,
        };
        let mut node_id = String::new();
        let mut fingerprint = String::new();
        let mut pubkey = String::new();
        for r in &msg.records {
            if let Record::Txt { name, pairs } = r {
                if *name == instance_fqdn {
                    for (k, v) in pairs {
                        match k.as_str() {
                            "id" => node_id = v.clone(),
                            "fp" => fingerprint = v.clone(),
                            "pk" => pubkey = v.clone(),
                            _ => {}
                        }
                    }
                }
            }
        }
        if node_id.is_empty() {
            continue; // 名乗りの無いものは採らない
        }
        let addr = msg
            .records
            .iter()
            .find_map(|r| match r {
                Record::A { addr, .. } => Some(*addr),
                _ => None,
            })
            .or(from);
        let instance = instance_fqdn
            .strip_suffix(&format!(".{}", SERVICE))
            .unwrap_or(&instance_fqdn)
            .to_string();
        out.push(MdnsPeer {
            instance,
            node_id,
            fingerprint,
            pubkey,
            port,
            addr,
        });
    }
    out
}

// ============================================================================
// 実際のソケット — ここから先が本物の I/O
// ============================================================================

use std::io;
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

/// LAN 発見のクライアント。
///
/// **エフェメラルポートに bind する** — 5353 を専有しない。RFC 6762 §5.1 の
/// 「one-shot query」で、QU ビットを立ててユニキャスト応答を求める。
/// これにより avahi / mDNSResponder が動いているホストでも共存できる
/// (5353 を bind しようとすると `EADDRINUSE` で失敗する環境が多い)。
pub struct Mdns {
    sock: UdpSocket,
    /// 5353 を確保できたか。できていれば**他ピアの問い合わせに応答できる**。
    /// できなければ一発問い合わせ (QU) 専用で、応答役は務まらない。
    responder: bool,
}

impl Mdns {
    /// 5353 を確保できたか (= 他ピアから見つけてもらえるか)。
    pub fn is_responder(&self) -> bool {
        self.responder
    }
}

impl Mdns {
    /// ソケットを開き、マルチキャストグループに参加する。
    ///
    /// 参加するのは、他のピアが自発的に流す告知
    /// ([`Mdns::announce`]) も拾えるようにするため。
    pub fn open() -> io::Result<Mdns> {
        // まず 5353 を狙う。取れれば応答役も務まり、他ピアのマルチキャスト告知も
        // 受け取れる (マルチキャストの配送は**宛先ポート**で決まるため、
        // エフェメラルポートでは :5353 宛の告知は届かない)。
        // avahi / mDNSResponder が既に握っていれば失敗するので、その時は
        // エフェメラルポートに退避して「一発問い合わせ専用」で動く。
        let (sock, responder) = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, MDNS_PORT)) {
            Ok(s) => (s, true),
            Err(_) => (UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?, false),
        };
        // 参加できない環境 (コンテナ・マルチキャスト無効 NIC) でも
        // 問い合わせ自体は送れるので、失敗しても致命的にしない。
        let _ = sock.join_multicast_v4(&MDNS_ADDR, &Ipv4Addr::UNSPECIFIED);
        let _ = sock.set_multicast_loop_v4(true);
        Ok(Mdns { sock, responder })
    }

    /// `timeout` の間、他ピアの `_rope._tcp.local` 問い合わせに応答する。
    ///
    /// QU ビットが立っていれば問い合わせ元へユニキャストで返す — これにより
    /// **5353 を取れなかった相手 (エフェメラルポートの一発問い合わせ) にも
    /// 応答が届く**。立っていなければマルチキャストで返す。
    ///
    /// 5353 を確保できていない場合は問い合わせが届かないので、何もせず返る。
    pub fn respond_to_queries(&self, adv: &Advertisement, timeout: Duration) -> io::Result<u32> {
        if !self.responder {
            return Ok(0);
        }
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 4096];
        let mut answered = 0u32;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            self.sock.set_read_timeout(Some(remaining))?;
            let (n, from) = match self.sock.recv_from(&mut buf) {
                Ok(v) => v,
                Err(_) => break,
            };
            let msg = match Message::decode(&buf[..n]) {
                Ok(m) => m,
                Err(_) => continue, // LAN には無関係な mDNS が飛び交う
            };
            for q in &msg.questions {
                if q.name != SERVICE || q.qtype != TYPE_PTR {
                    continue;
                }
                let pkt = adv.to_response();
                let dest: SocketAddr = if q.unicast {
                    from
                } else {
                    SocketAddr::V4(SocketAddrV4::new(MDNS_ADDR, MDNS_PORT))
                };
                let _ = self.sock.send_to(&pkt, dest);
                answered += 1;
            }
        }
        Ok(answered)
    }

    /// 自分の存在を LAN に告知する (unsolicited announcement)。
    pub fn announce(&self, adv: &Advertisement) -> io::Result<()> {
        let pkt = adv.to_response();
        self.sock
            .send_to(&pkt, SocketAddrV4::new(MDNS_ADDR, MDNS_PORT))?;
        Ok(())
    }

    /// **探索と応答を同時に行う** — mDNS は対称なプロトコルなので、
    /// 1 本のソケットで「聞く」と「答える」を 1 つのループに入れるのが自然。
    /// スレッドも非同期ランタイムも要らない。
    ///
    /// - 開始時に `_rope._tcp.local` の PTR 問い合わせを投げる
    /// - `timeout` の間、届いたパケットを仕分ける:
    ///   - **応答** → ピアとして拾う (自分の告知は `self_node_id` で除外)
    ///   - **問い合わせ** → `adv` があれば答える (QU ならユニキャストで)
    ///
    /// パースできないパケットは黙って捨てる。LAN には Rope と無関係な
    /// mDNS トラフィックが常時流れている。
    pub fn discover(
        &self,
        adv: Option<&Advertisement>,
        timeout: Duration,
        self_node_id: &str,
    ) -> io::Result<Discovery> {
        self.sock.send_to(
            &Advertisement::query(),
            SocketAddrV4::new(MDNS_ADDR, MDNS_PORT),
        )?;

        let deadline = Instant::now() + timeout;
        let mut out = Discovery {
            peers: Vec::new(),
            answered: 0,
            responder: self.responder,
        };
        let mut buf = [0u8; 4096];

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            self.sock.set_read_timeout(Some(remaining))?;
            let (n, from) = match self.sock.recv_from(&mut buf) {
                Ok(v) => v,
                // タイムアウトも受信エラーも探索を止める理由にしかならない
                Err(_) => break,
            };
            let msg = match Message::decode(&buf[..n]) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let from_v4 = match from {
                SocketAddr::V4(a) => Some(*a.ip()),
                SocketAddr::V6(_) => None,
            };
            for p in peers_from_message(&msg, from_v4) {
                if p.node_id == self_node_id {
                    continue; // 自分の告知が返ってきただけ
                }
                if !out.peers.iter().any(|q| q.node_id == p.node_id) {
                    out.peers.push(p);
                }
            }

            if let Some(adv) = adv {
                for q in &msg.questions {
                    if q.name != SERVICE || q.qtype != TYPE_PTR {
                        continue;
                    }
                    let dest: SocketAddr = if q.unicast {
                        from
                    } else {
                        SocketAddr::V4(SocketAddrV4::new(MDNS_ADDR, MDNS_PORT))
                    };
                    let _ = self.sock.send_to(&adv.to_response(), dest);
                    out.answered += 1;
                }
            }
        }
        Ok(out)
    }
}

/// 1 回の探索の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    /// 見つかった他ピア (未検証 — TOFU の照合は `core::pair` の仕事)
    pub peers: Vec<MdnsPeer>,
    /// 他ピアの問い合わせに答えた回数
    pub answered: u32,
    /// 5353 を確保できたか。**false なら他ピアから見つけてもらえない**
    /// (問い合わせは投げられるので、片方向の発見はできる)
    pub responder: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adv() -> Advertisement {
        Advertisement {
            instance: "alice-box".into(),
            port: 7777,
            node_id: "node-abc".into(),
            fingerprint: "AA:BB:CC:DD".into(),
            pubkey: "deadbeef".into(),
        }
    }

    #[test]
    fn response_roundtrips_through_the_parser() {
        let bytes = adv().to_response();
        let msg = Message::decode(&bytes).expect("自分の出したパケットは自分で読める");
        assert!(msg.is_response);
        assert_eq!(msg.records.len(), 3);
        let peers = peers_from_message(&msg, None);
        assert_eq!(peers.len(), 1, "{:?}", msg);
        let p = &peers[0];
        assert_eq!(p.instance, "alice-box");
        assert_eq!(p.node_id, "node-abc");
        assert_eq!(p.fingerprint, "AA:BB:CC:DD");
        assert_eq!(p.pubkey, "deadbeef");
        assert_eq!(p.port, 7777);
    }

    #[test]
    fn query_is_a_well_formed_ptr_question() {
        let q = Advertisement::query();
        let msg = Message::decode(&q).unwrap();
        assert!(!msg.is_response);
        assert_eq!(msg.questions.len(), 1);
        assert_eq!(msg.questions[0].name, SERVICE);
        assert_eq!(msg.questions[0].qtype, TYPE_PTR);
        assert!(
            msg.questions[0].unicast,
            "QU ビットを立てて 5353 を専有せずに済ませる"
        );
    }

    #[test]
    fn hostile_labels_are_sanitized_not_propagated() {
        let a = Advertisement {
            instance: "../../etc\0 evil.name".into(),
            ..adv()
        };
        let bytes = a.to_response();
        let msg = Message::decode(&bytes).expect("細工した名前でもパケットは壊れない");
        let peers = peers_from_message(&msg, None);
        assert_eq!(peers.len(), 1);
        assert!(
            peers[0]
                .instance
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "危険な文字が残っている: {:?}",
            peers[0].instance
        );
    }

    #[test]
    fn empty_instance_name_still_produces_valid_packet() {
        let a = Advertisement {
            instance: "日本語だけ".into(),
            ..adv()
        };
        let msg = Message::decode(&a.to_response()).unwrap();
        assert_eq!(peers_from_message(&msg, None).len(), 1);
    }

    // --- 敵対的な入力: どれも panic せず Err か「捨てる」で終わること ---

    #[test]
    fn truncated_packets_never_panic() {
        let full = adv().to_response();
        for n in 0..full.len() {
            let _ = Message::decode(&full[..n]); // panic しなければ合格
        }
    }

    #[test]
    fn compression_pointer_loop_is_rejected() {
        // ヘッダ(12B) + 自分自身を指すポインタ
        let mut p = vec![0u8; 12];
        p[4] = 0;
        p[5] = 1; // qdcount = 1
        p.push(0xC0);
        p.push(12); // オフセット 12 = このポインタ自身
        p.extend_from_slice(&[0, 12, 0, 1]);
        assert_eq!(Message::decode(&p), Err(MdnsError::PointerLoop));
    }

    #[test]
    fn absurd_record_counts_are_rejected_before_allocating() {
        let mut p = vec![0u8; 12];
        p[6] = 0xFF;
        p[7] = 0xFF; // ancount = 65535
        assert_eq!(Message::decode(&p), Err(MdnsError::TooManyRecords));
    }

    #[test]
    fn random_garbage_never_panics() {
        let mut seed: u64 = 0xDEADBEEF;
        for _ in 0..2000 {
            let mut buf = Vec::new();
            let n = {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed % 200) as usize
            };
            for _ in 0..n {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                buf.push((seed >> 24) as u8);
            }
            let _ = Message::decode(&buf);
        }
    }

    #[test]
    fn peer_without_txt_identity_is_discarded() {
        // SRV だけあって TXT の id が無いピアは採らない
        let mut out = Vec::new();
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&0x8400u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes()); // ancount = 1 (SRV のみ)
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        // 手書き SRV
        for label in ["ghost", "_rope", "_tcp", "local"] {
            out.push(label.len() as u8);
            out.extend_from_slice(label.as_bytes());
        }
        out.push(0);
        out.extend_from_slice(&TYPE_SRV.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        out.extend_from_slice(&120u32.to_be_bytes());
        let mut rd = Vec::new();
        rd.extend_from_slice(&0u16.to_be_bytes());
        rd.extend_from_slice(&0u16.to_be_bytes());
        rd.extend_from_slice(&9999u16.to_be_bytes());
        for label in ["ghost", "local"] {
            rd.push(label.len() as u8);
            rd.extend_from_slice(label.as_bytes());
        }
        rd.push(0);
        out.extend_from_slice(&(rd.len() as u16).to_be_bytes());
        out.extend_from_slice(&rd);

        let msg = Message::decode(&out).unwrap();
        assert!(
            peers_from_message(&msg, None).is_empty(),
            "名乗りの無いピアは採らない"
        );
    }
    // ------------------------------------------------------------------
    // 実ソケットのテスト
    // ------------------------------------------------------------------

    /// **実際に UDP マルチキャストで発見できることを確かめる。**
    ///
    /// alice が 5353 を確保して応答役になり、bob はエフェメラルポートから
    /// QU 付きで問い合わせる (= 1 ホストに rope が 2 つ居る場合の実際の形)。
    ///
    /// ⚠️ マルチキャストが使えない環境 (5353 が塞がっている / NIC が
    /// マルチキャスト非対応のコンテナ) では alice が応答役になれない。
    /// その場合は**黙って通すのではなく**、何を確かめられなかったかを
    /// 出力して終える。
    #[test]
    fn two_sockets_discover_each_other_over_real_multicast() {
        use std::time::Duration;

        let alice_adv = Advertisement {
            instance: "alice".into(),
            port: 7777,
            node_id: "node-alice".into(),
            fingerprint: "AA:BB".into(),
            pubkey: "a11ce".into(),
        };
        let alice = match Mdns::open() {
            Ok(m) => m,
            Err(e) => {
                println!("スキップ: ソケットを開けない ({e})");
                return;
            }
        };
        if !alice.is_responder() {
            println!("スキップ: 5353 を確保できず応答役になれない (別の mDNS が動作中?)");
            return;
        }
        let bob = match Mdns::open() {
            Ok(m) => m,
            Err(e) => {
                println!("スキップ: 2 本目のソケットを開けない ({e})");
                return;
            }
        };

        let h = std::thread::spawn(move || {
            alice
                .discover(Some(&alice_adv), Duration::from_millis(1500), "node-alice")
                .map(|d| d.answered)
                .unwrap_or(0)
        });
        std::thread::sleep(Duration::from_millis(150));

        let d = bob
            .discover(None, Duration::from_millis(1200), "node-bob")
            .expect("browse");
        let answered = h.join().unwrap_or(0);

        assert!(answered > 0, "alice は bob の問い合わせに応答したはず");
        assert_eq!(
            d.peers.len(),
            1,
            "bob は alice を 1 台だけ見つける: {:?}",
            d
        );
        let p = &d.peers[0];
        assert_eq!(p.node_id, "node-alice");
        assert_eq!(p.fingerprint, "AA:BB");
        assert_eq!(p.pubkey, "a11ce");
        assert_eq!(p.port, 7777);
        assert!(p.addr.is_some(), "送信元アドレスが取れている");
    }

    /// 自分の告知は自分の発見結果に入らない。
    #[test]
    fn own_announcement_is_not_reported_as_a_peer() {
        use std::time::Duration;
        let adv = Advertisement {
            instance: "solo".into(),
            port: 1,
            node_id: "node-solo".into(),
            fingerprint: "FF".into(),
            pubkey: "5010".into(),
        };
        let m = match Mdns::open() {
            Ok(m) => m,
            Err(_) => return,
        };
        let _ = m.announce(&adv);
        let d = m
            .discover(Some(&adv), Duration::from_millis(300), "node-solo")
            .expect("discover");
        assert!(
            d.peers.iter().all(|p| p.node_id != "node-solo"),
            "自分自身をピアとして数えない: {:?}",
            d.peers
        );
    }
}

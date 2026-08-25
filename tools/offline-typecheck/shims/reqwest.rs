//! `reqwest` の型検査専用スタブ。
//!
//! 🔴 **ネットワークに一切触らない。** `send()` は必ずエラーを返す。
//! 目的は `--features http` の経路を**型検査とテストの対象に入れる**こと。
//! この feature の下にあるのは Cashu mint との実 HTTP 通信であり、
//! そこはどのみち実ネットワークが要るので CI でも実接続はしない。
//!
//! 使う面は `src/net/cashu_mint.rs` が実際に呼ぶ 9 メソッドだけ:
//! `Client::builder` / `timeout` / `user_agent` / `build` / `get` / `post` /
//! `json` / `send` / `status` / `text` と `Url::parse`。

use std::time::Duration;

#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "reqwest error: {}", self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

// ============================================================================
// Url
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    scheme: String,
    host: String,
    raw: String,
}

impl Url {
    /// スキームとホストだけ取り出す最小のパーサ。
    /// `cashu_mint` はこの 2 つしか見ない。
    pub fn parse(s: &str) -> Result<Url> {
        let (scheme, rest) = s
            .split_once("://")
            .ok_or_else(|| Error(format!("スキームが無い: {}", s)))?;
        if scheme.is_empty() {
            return Err(Error("スキームが空".to_string()));
        }
        let host = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .split('@')
            .next_back()
            .unwrap_or("");
        if host.is_empty() {
            return Err(Error(format!("ホストが無い: {}", s)));
        }
        Ok(Url {
            scheme: scheme.to_ascii_lowercase(),
            host: host.to_string(),
            raw: s.to_string(),
        })
    }
    pub fn scheme(&self) -> &str {
        &self.scheme
    }
    pub fn host_str(&self) -> Option<&str> {
        Some(&self.host)
    }
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.raw)
    }
}

// ============================================================================
// StatusCode / Response
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCode(u16);

impl StatusCode {
    pub fn as_u16(&self) -> u16 {
        self.0
    }
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.0)
    }
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.0)
    }
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.0)
    }
}

impl std::fmt::Display for StatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct Response {
    status: StatusCode,
}

impl Response {
    pub fn status(&self) -> StatusCode {
        self.status
    }
    /// 🔴 常にエラー。ネットワークが無いので本文は存在しない。
    pub async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        Err(Error("スタブは本文を持たない".to_string()))
    }
    pub async fn text(self) -> Result<String> {
        Err(Error("スタブは本文を持たない".to_string()))
    }
    pub async fn bytes(self) -> Result<Vec<u8>> {
        Err(Error("スタブは本文を持たない".to_string()))
    }
}

// ============================================================================
// Client
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct ClientBuilder {
    timeout: Option<Duration>,
    user_agent: Option<String>,
}

impl ClientBuilder {
    pub fn timeout(mut self, d: Duration) -> ClientBuilder {
        self.timeout = Some(d);
        self
    }
    pub fn user_agent<S: ToString>(mut self, ua: S) -> ClientBuilder {
        self.user_agent = Some(ua.to_string());
        self
    }
    pub fn build(self) -> Result<Client> {
        Ok(Client {
            _timeout: self.timeout,
            _user_agent: self.user_agent,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Client {
    _timeout: Option<Duration>,
    _user_agent: Option<String>,
}

impl Client {
    pub fn new() -> Client {
        Client::default()
    }
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }
    pub fn get<S: ToString>(&self, url: S) -> RequestBuilder {
        RequestBuilder::new(url)
    }
    pub fn post<S: ToString>(&self, url: S) -> RequestBuilder {
        RequestBuilder::new(url)
    }
    pub fn put<S: ToString>(&self, url: S) -> RequestBuilder {
        RequestBuilder::new(url)
    }
    pub fn delete<S: ToString>(&self, url: S) -> RequestBuilder {
        RequestBuilder::new(url)
    }
}

pub struct RequestBuilder {
    url: String,
}

impl RequestBuilder {
    fn new<S: ToString>(url: S) -> RequestBuilder {
        RequestBuilder {
            url: url.to_string(),
        }
    }
    pub fn json<T: serde::Serialize + ?Sized>(self, _body: &T) -> RequestBuilder {
        self
    }
    pub fn body<T: Into<Vec<u8>>>(self, _body: T) -> RequestBuilder {
        self
    }
    pub fn timeout(self, _d: Duration) -> RequestBuilder {
        self
    }
    pub fn header<K: ToString, V: ToString>(self, _k: K, _v: V) -> RequestBuilder {
        self
    }
    pub fn query<T: serde::Serialize + ?Sized>(self, _q: &T) -> RequestBuilder {
        self
    }
    /// 🔴 **必ず失敗する。** ネットワークに触らないことを型ではなく挙動で示す。
    pub async fn send(self) -> Result<Response> {
        Err(Error(format!(
            "スタブはネットワークに接続しない: {}",
            self.url
        )))
    }
}

/// 参照先を持たない `Response` を作りたいテスト向け (現状未使用)。
impl Response {
    pub fn stub(status: u16) -> Response {
        Response {
            status: StatusCode(status),
        }
    }
}

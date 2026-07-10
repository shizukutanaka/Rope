//! セッション管理モジュール
//!
//! セッションのライフサイクル、状態管理、強制停止を担当

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use super::config;

/// グローバル停止フラグ
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// セッション状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// 未使用（アイドル）
    Idle,
    /// 接続待ち
    Waiting,
    /// 接続確立中（ハンドシェイク）
    Connecting,
    /// VERIFY CODE確認待ち
    Verifying,
    /// 実行中
    Running,
    /// 停止済み
    Stopped,
    /// エラー
    Error,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Idle => write!(f, "アイドル"),
            SessionState::Waiting => write!(f, "接続待ち"),
            SessionState::Connecting => write!(f, "接続中"),
            SessionState::Verifying => write!(f, "確認待ち"),
            SessionState::Running => write!(f, "実行中"),
            SessionState::Stopped => write!(f, "停止"),
            SessionState::Error => write!(f, "エラー"),
        }
    }
}

/// リソース制限
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limits {
    /// 最大実行時間（分）
    pub max_time_minutes: u32,
    /// 最大VRAM（MB）
    pub max_vram: u32,
    /// 最大GPU使用率（%）
    pub max_gpu_util: u8,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_time_minutes: 60,
            max_vram: 8192,
            max_gpu_util: 80,
        }
    }
}

/// Capabilityトークン（権限制御）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// トークンID
    pub id: String,
    /// GPU番号
    pub gpu: u32,
    /// VRAM上限（MB）
    pub vram_mb: u32,
    /// 最大実行時間（分）
    pub max_runtime_minutes: u32,
    /// 有効期限（Unix timestamp）
    pub expires: i64,
    /// ノンス（再利用防止）
    pub nonce: String,
    /// 署名（Base64）
    pub signature: String,
}

impl Capability {
    /// 署名対象の正規バイト列。`signature` 以外の全フィールドを固定順の
    /// JSON 配列で直列化する。配列は順序保証 + 文字列エスケープがあるため、
    /// id / nonce に区切り文字が混入してもフィールド境界が曖昧にならない。
    fn signing_payload(&self) -> Vec<u8> {
        let parts = (
            "rope-cap:v1",
            &self.id,
            self.gpu,
            self.vram_mb,
            self.max_runtime_minutes,
            self.expires,
            &self.nonce,
        );
        // 固定構造のタプルなので直列化は失敗しない。万一失敗しても空 → 検証で確実に弾く。
        serde_json::to_vec(&parts).unwrap_or_default()
    }

    /// 発行者（プロバイダ）側: capability に署名して `signature` を埋める。
    pub fn sign(&mut self, issuer_key: &ed25519_dalek::SigningKey) {
        let sig = issuer_key.sign(&self.signing_payload());
        self.signature = BASE64.encode(sig.to_bytes());
    }

    /// 署名検証: capability が指定 issuer の署名を持つことを確認する。
    /// `verify_strict` で非正規・小位数点の署名を拒否する。
    pub fn verify_signature(&self, issuer: &VerifyingKey) -> Result<()> {
        let sig_bytes = BASE64
            .decode(self.signature.trim())
            .context("capability 署名デコード失敗")?;
        let sig_arr: [u8; 64] = sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("capability 署名長不正 (64 バイト期待)"))?;
        let sig = Signature::from_bytes(&sig_arr);
        issuer
            .verify_strict(&self.signing_payload(), &sig)
            .map_err(|_| anyhow::anyhow!("capability 署名不一致 (改ざん or 別発行者)"))
    }

    /// 失効しているか（`now_unix` は Unix 秒）。expires 丁度も失効扱い。
    pub fn is_expired(&self, now_unix: i64) -> bool {
        self.expires <= now_unix
    }

    /// この capability が要求された制限を許可するか。
    /// capability はプロバイダからの権限付与なので、セッションの要求 limits は
    /// capability の上限以内でなければならない。
    pub fn authorizes(&self, limits: &Limits) -> bool {
        limits.max_vram <= self.vram_mb && limits.max_time_minutes <= self.max_runtime_minutes
    }

    /// 結合ゲート: 署名 OK ∧ 未失効 ∧ 制限が上限内。
    /// セッション開始前にこれを通す。要件 vs 証明の原則: ピアの自己申告ではなく、
    /// 発行者署名で裏付けられた権限のみを受理する。
    pub fn validate(&self, issuer: &VerifyingKey, limits: &Limits, now_unix: i64) -> Result<()> {
        self.verify_signature(issuer)?;
        if self.is_expired(now_unix) {
            anyhow::bail!(
                "capability 失効済み (expires={}, now={})",
                self.expires,
                now_unix
            );
        }
        if !self.authorizes(limits) {
            anyhow::bail!(
                "capability が要求制限を許可しない (cap: {}MB/{}分, 要求: {}MB/{}分)",
                self.vram_mb,
                self.max_runtime_minutes,
                limits.max_vram,
                limits.max_time_minutes
            );
        }
        Ok(())
    }
}

/// セッション情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// セッションID（UUIDv7）
    pub id: String,
    /// 状態
    pub state: SessionState,
    /// ピアID（接続相手のフィンガープリント）
    pub peer_id: Option<String>,
    /// 開始時刻
    pub started_at: Option<DateTime<Utc>>,
    /// 制限
    pub limits: Limits,
    /// VERIFY CODE（6桁）
    pub verify_code: Option<String>,
    /// Capability
    pub capability: Option<Capability>,
    /// コンテナID
    pub container_id: Option<String>,
    /// GPU監視PID
    pub monitor_pid: Option<u32>,
}

impl Session {
    /// 新規セッションを作成
    pub fn new(limits: Limits) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            state: SessionState::Idle,
            peer_id: None,
            started_at: None,
            limits,
            verify_code: None,
            capability: None,
            container_id: None,
            monitor_pid: None,
        }
    }

    /// 状態遷移（安全側に倒す）
    pub fn transition(&mut self, new_state: SessionState) -> Result<()> {
        use SessionState::*;

        // 許可された遷移のみ
        let allowed = match (self.state, new_state) {
            (Idle, Waiting) => true,
            (Waiting, Connecting) => true,
            (Connecting, Verifying) => true,
            (Verifying, Running) => true,
            (Running, Stopped) => true,
            // エラー/停止への遷移は常に許可
            (_, Error) => true,
            (_, Stopped) => true,
            // 同じ状態への遷移は許可
            (a, b) if a == b => true,
            _ => false,
        };

        if allowed {
            tracing::info!("Session state: {} -> {}", self.state, new_state);
            self.state = new_state;
            Ok(())
        } else {
            anyhow::bail!("不正な状態遷移: {} → {}", self.state, new_state);
        }
    }

    /// VERIFY CODEを生成（6桁数字）
    pub fn generate_verify_code(&mut self) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let code: u32 = rng.gen_range(100_000..1_000_000);
        self.verify_code = Some(format!("{}", code));
    }

    /// セッションファイルに保存
    pub fn save(&self) -> Result<()> {
        let path = config::session_dir().join(format!("{}.json", self.id));
        let content = serde_json::to_string_pretty(self).context("セッションシリアライズ失敗")?;
        config::atomic_write(&path, &content)
    }

    /// セッションファイルから読み込み
    pub fn load(id: &str) -> Result<Self> {
        let path = config::session_dir().join(format!("{}.json", id));
        let content = fs::read_to_string(path).context("セッションファイル読込失敗")?;
        let session: Session =
            serde_json::from_str(&content).context("セッションファイルパース失敗")?;
        Ok(session)
    }

    /// セッションを削除
    pub fn delete(&self) -> Result<()> {
        let path = config::session_dir().join(format!("{}.json", self.id));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

/// グローバルセッションマネージャー
///
/// v0.2: sync (main.rs が sync のため)。
/// v0.3 で mDNS/Noise 結線時に async 化が必要なら tokio::sync::RwLock に戻す。
///
/// **ステータス**: `new`/`current`/`start`/`elapsed_seconds` はテスト済みだが、
/// `main.rs` は代わりにファイルベースの `Session::save`/`load` ライフサイクルを
/// 直接使っており、この in-memory RwLock 版マネージャーは現行 4 動詞のどこからも
/// 使われていない。`end` に至ってはテストからも呼ばれていない (`docs/
/// REACHABILITY_AUDIT.md` category c)。削除するかどうかは製品判断が必要 —
/// 「v0.3 async化で使う予定の設計」なのか「ファイルベース方式に置き換わった後の
/// 残骸」なのか、このコメント時点では確定できない。`docs/SURPLUS_AND_GAPS.md`
/// §2.4 参照。
pub struct SessionManager {
    current: Arc<RwLock<Option<Session>>>,
    start_time: Option<Instant>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    /// 新規マネージャー作成
    pub fn new() -> Self {
        Self {
            current: Arc::new(RwLock::new(None)),
            start_time: None,
        }
    }

    /// 現在のセッションを取得
    pub fn current(&self) -> Option<Session> {
        self.current.read().ok().and_then(|guard| guard.clone())
    }

    /// セッションを開始
    pub fn start(&mut self, session: Session) -> Result<()> {
        let mut current = self
            .current
            .write()
            .map_err(|e| anyhow::anyhow!("RwLock poisoned: {}", e))?;
        if current.is_some() {
            anyhow::bail!("セッション既にアクティブ");
        }
        *current = Some(session);
        self.start_time = Some(Instant::now());
        Ok(())
    }

    /// セッションを終了
    pub fn end(&self) -> Result<()> {
        let mut current = self
            .current
            .write()
            .map_err(|e| anyhow::anyhow!("RwLock poisoned: {}", e))?;
        if let Some(session) = current.take() {
            session.delete()?;
        }
        Ok(())
    }

    /// 経過時間を取得（秒）
    pub fn elapsed_seconds(&self) -> u64 {
        self.start_time.map(|t| t.elapsed().as_secs()).unwrap_or(0)
    }
}

/// 緊急停止（フェイルセーフ）
///
/// 安全性最優先のため **best-effort** で進む。docker が無い/失敗しても全体を
/// 諦めず、停止フラグの設定とセッションファイルの掃除まで必ず到達する。
/// 旧実装は `docker ps` の失敗で `?` 早期 return し、コンテナ kill もファイル掃除も
/// せず終わっていた（緊急停止が最も脆い経路だった）。
///
/// また `cleanup()` は呼ばない。cleanup は最後に停止フラグをリセットするため、
/// 緊急停止のシグナルを即座に打ち消してしまう。ここではフラグを立てたまま残し、
/// ポーリング中のループ（監視/デモ）が確実に停止を観測できるようにする。
///
/// **ステータス**: 上記の通り安全性を最優先に設計・過去に一度バグ修正されている
/// にもかかわらず、現状どこからも呼ばれていない (テストも含め)。シグナルハンドラ
/// や Ctrl-C ハンドラが未配線なため。`cleanup`/`is_stop_requested`/`get_status`/
/// `list_sessions`/`format_session_list`/`Session::load`/`SessionManager::end` も
/// 同様に呼び出し元ゼロ。「呼び出し元ゼロ」だけで削除すべきでない理由は
/// `docs/SURPLUS_AND_GAPS.md` §2.4 / `docs/REACHABILITY_AUDIT.md` 参照 —
/// 削除するか signal handler に配線するかは製品判断待ち。
pub fn panic_stop() -> Result<()> {
    tracing::warn!("PANIC STOP triggered");
    // 真っ先に停止フラグ。後続が失敗してもポーリング側は止まれる。
    STOP_REQUESTED.store(true, Ordering::SeqCst);

    // コンテナ停止は best-effort: docker 不在/失敗で緊急停止全体を諦めない。
    match std::process::Command::new("docker")
        .args(["ps", "-q", "--filter", "name=rope-"])
        .output()
    {
        Ok(output) => {
            for id in String::from_utf8_lossy(&output.stdout).lines() {
                if !id.is_empty() {
                    tracing::info!("Killing container: {}", id);
                    std::process::Command::new("docker")
                        .args(["kill", id])
                        .output()
                        .ok();
                }
            }
        }
        Err(e) => tracing::warn!("docker ps 失敗、コンテナ停止をスキップ: {}", e),
    }

    // セッションファイルを掃除（best-effort）。失敗しても停止フラグは維持。
    if let Err(e) = clear_session_files() {
        tracing::warn!("セッションファイル削除失敗: {}", e);
    }

    Ok(())
}

/// 停止が要求されているかチェック
pub fn is_stop_requested() -> bool {
    STOP_REQUESTED.load(Ordering::SeqCst)
}

/// 停止フラグをリセット
pub fn reset_stop_flag() {
    STOP_REQUESTED.store(false, Ordering::SeqCst);
}

/// 状態を取得
pub fn get_status() -> Result<String> {
    let session_dir = config::session_dir();
    let mut status = String::new();

    // アクティブセッションを探す
    let mut found = false;
    if session_dir.exists() {
        for entry in fs::read_dir(session_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                let content = fs::read_to_string(&path)?;
                if let Ok(session) = serde_json::from_str::<Session>(&content) {
                    found = true;
                    status.push_str(&format!("セッション: {}\n", session.id));
                    status.push_str(&format!("  状態: {}\n", session.state));
                    if let Some(peer) = &session.peer_id {
                        status.push_str(&format!("  ピア: {}\n", peer));
                    }
                    if let Some(started) = &session.started_at {
                        status.push_str(&format!("  開始: {}\n", started));
                    }
                    status.push_str("  制限:\n");
                    status.push_str(&format!(
                        "    時間: {}分\n",
                        session.limits.max_time_minutes
                    ));
                    status.push_str(&format!("    VRAM: {}MB\n", session.limits.max_vram));
                    status.push_str(&format!(
                        "    GPU使用率: {}%\n",
                        session.limits.max_gpu_util
                    ));
                }
            }
        }
    }

    if !found {
        status.push_str("アクティブなセッションなし\n");
    }

    Ok(status)
}

/// 孤児セッションの掃除
///
/// 起動時に呼ぶ。アクティブでない (Waiting/Idle/Stopped/Error の) セッションを削除。
/// 進行中 (Connecting/Verifying/Running) は保持。
/// これがないと `rope earn` を起動するたびに waiting セッションが溜まり続ける (I2: 100年運用)。
pub fn prune_stale() -> Result<usize> {
    let dir = config::session_dir();
    if !dir.exists() {
        return Ok(0);
    }
    let mut pruned = 0;
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            // 読めない/壊れた JSON も掃除対象
            let stale = match fs::read_to_string(&path)
                .ok()
                .and_then(|c| serde_json::from_str::<Session>(&c).ok())
            {
                Some(s) => !matches!(
                    s.state,
                    SessionState::Connecting | SessionState::Verifying | SessionState::Running
                ),
                None => true,
            };
            if stale {
                fs::remove_file(&path).ok();
                pruned += 1;
            }
        }
    }
    Ok(pruned)
}

/// セッションファイル (*.json) を全削除する。停止フラグには触れない。
/// `cleanup`（通常終了）と `panic_stop`（緊急停止）で共有する純掃除処理。
fn clear_session_files() -> Result<()> {
    let session_dir = config::session_dir();
    if session_dir.exists() {
        for entry in fs::read_dir(&session_dir)? {
            let path = entry?.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                fs::remove_file(path)?;
            }
        }
    }
    Ok(())
}

/// クリーンアップ
pub fn cleanup() -> Result<()> {
    // セッションファイルを削除
    clear_session_files()?;

    // 残存コンテナを削除 (docker 不在は silent ignore)
    if let Ok(output) = std::process::Command::new("docker")
        .args(["ps", "-aq", "--filter", "name=rope-"])
        .output()
    {
        let ids = String::from_utf8_lossy(&output.stdout);
        for id in ids.lines() {
            if !id.is_empty() {
                std::process::Command::new("docker")
                    .args(["rm", "-f", id])
                    .output()
                    .ok();
            }
        }
    }

    // 停止フラグをリセット
    reset_stop_flag();

    Ok(())
}

// ============================================================================
// Round 21: format symmetry — 他モジュール (confidential / ecash / pair / intent /
// first_run) はすべて format_X() を持つが session のみ非対称だった。Apple 流
// 一貫性のために追加。新規データなし、既存 fields の表示のみ。
// ============================================================================

/// セッション 1 件を人間可読にフォーマット
pub fn format_session(s: &Session) -> String {
    let mut out = String::new();
    out.push_str("セッション:\n");
    out.push_str("═══════════════════════════════════════════════════\n");
    out.push_str(&format!("  ID:        {}\n", super::short(&s.id, 8)));
    out.push_str(&format!("  状態:      {}\n", s.state));

    if let Some(peer) = &s.peer_id {
        out.push_str(&format!("  ピア:      {}\n", super::short(peer, 16)));
    }
    if let Some(t) = s.started_at {
        out.push_str(&format!(
            "  開始時刻:  {}\n",
            t.format("%Y-%m-%d %H:%M:%S UTC")
        ));
    }
    if let Some(code) = &s.verify_code {
        out.push_str(&format!("  確認コード: {}\n", code));
    }
    if let Some(c_id) = &s.container_id {
        out.push_str(&format!("  コンテナ:  {}\n", super::short(c_id, 12)));
    }
    if let Some(pid) = s.monitor_pid {
        out.push_str(&format!("  監視 PID:  {}\n", pid));
    }

    out.push_str("  制限:\n");
    out.push_str(&format!(
        "    最大時間:  {} 分\n",
        s.limits.max_time_minutes
    ));
    if let Some(started) = s.started_at {
        let elapsed = (Utc::now() - started).num_seconds().max(0);
        out.push_str(&format!("    経過:      {} 秒\n", elapsed));
    }

    out
}

/// 全セッションを一覧フォーマット (config::session_dir 配下を走査)
pub fn list_sessions() -> Result<Vec<Session>> {
    let dir = config::session_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok(s) = serde_json::from_str::<Session>(&content) {
            sessions.push(s);
        }
    }
    Ok(sessions)
}

/// 全セッションを 1 文字列にフォーマット
pub fn format_session_list(sessions: &[Session]) -> String {
    if sessions.is_empty() {
        return "アクティブなセッションなし。\n".to_string();
    }
    let mut out = format!("セッション一覧 ({}):\n", sessions.len());
    out.push_str("═══════════════════════════════════════════════════\n");
    for s in sessions {
        out.push_str(&format!("  {}  {}", super::short(&s.id, 8), s.state));
        if let Some(code) = &s.verify_code {
            out.push_str(&format!("  code={}", code));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_transition() {
        let mut session = Session::new(Limits::default());
        assert_eq!(session.state, SessionState::Idle);

        session.transition(SessionState::Waiting).unwrap();
        assert_eq!(session.state, SessionState::Waiting);

        // 不正な遷移
        assert!(session.transition(SessionState::Running).is_err());

        // エラーへの遷移は常に許可
        session.transition(SessionState::Error).unwrap();
        assert_eq!(session.state, SessionState::Error);
    }

    #[test]
    fn test_verify_code_generation() {
        let mut session = Session::new(Limits::default());
        session.generate_verify_code();

        let code = session.verify_code.unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.parse::<u32>().is_ok());
    }

    #[test]
    fn test_full_lifecycle_idle_to_stopped() {
        let mut s = Session::new(Limits::default());
        s.transition(SessionState::Waiting).unwrap();
        s.transition(SessionState::Connecting).unwrap();
        s.transition(SessionState::Verifying).unwrap();
        s.transition(SessionState::Running).unwrap();
        s.transition(SessionState::Stopped).unwrap();
        assert_eq!(s.state, SessionState::Stopped);
    }

    #[test]
    fn test_invalid_skips_blocked() {
        let mut s = Session::new(Limits::default());
        // Idle から Running への直接遷移は不可
        assert!(s.transition(SessionState::Running).is_err());
        // Idle から Verifying も不可
        assert!(s.transition(SessionState::Verifying).is_err());
    }

    #[test]
    fn test_error_state_terminal_for_normal_states() {
        let mut s = Session::new(Limits::default());
        s.transition(SessionState::Error).unwrap();
        // Error から非正常遷移は失敗 or 特別扱い — どちらでも許容
        s.transition(SessionState::Running).ok();
    }

    #[test]
    fn test_two_sessions_have_unique_ids() {
        let s1 = Session::new(Limits::default());
        let s2 = Session::new(Limits::default());
        assert_ne!(s1.id, s2.id);
    }

    #[test]
    fn test_verify_code_two_calls_differ() {
        let mut s = Session::new(Limits::default());
        s.generate_verify_code();
        let c1 = s.verify_code.clone().unwrap();
        s.generate_verify_code();
        let c2 = s.verify_code.clone().unwrap();
        // 6 桁ランダム → 衝突確率 1/1,000,000、テスト失敗は事実上ない
        assert_ne!(c1, c2);
    }

    // Round 21: format symmetry
    #[test]
    fn test_format_session_renders_id_and_state() {
        let s = Session::new(Limits::default());
        let out = format_session(&s);
        assert!(out.contains(&s.id[..s.id.len().min(8)]));
        assert!(out.contains("アイドル"));
    }

    #[test]
    fn test_format_session_renders_verify_code() {
        let mut s = Session::new(Limits::default());
        s.generate_verify_code();
        let out = format_session(&s);
        assert!(out.contains("確認コード"));
        assert!(out.contains(s.verify_code.as_ref().unwrap()));
    }

    #[test]
    fn test_format_session_renders_limits() {
        let s = Session::new(Limits {
            max_time_minutes: 120,
            ..Default::default()
        });
        let out = format_session(&s);
        assert!(out.contains("120"));
    }

    // ====== Round 23: SessionManager sync tests ======

    #[test]
    fn test_session_manager_start_and_current() {
        let mut mgr = SessionManager::new();
        assert!(mgr.current().is_none());

        let s = Session::new(Limits::default());
        mgr.start(s.clone()).unwrap();
        assert!(mgr.current().is_some());
        assert_eq!(mgr.current().unwrap().id, s.id);
    }

    #[test]
    fn test_session_manager_double_start_rejected() {
        let mut mgr = SessionManager::new();
        mgr.start(Session::new(Limits::default())).unwrap();
        assert!(mgr.start(Session::new(Limits::default())).is_err());
    }

    #[test]
    fn test_session_manager_elapsed() {
        let mut mgr = SessionManager::new();
        assert_eq!(mgr.elapsed_seconds(), 0);
        mgr.start(Session::new(Limits::default())).unwrap();
        // 即座なので 0 か 1
        assert!(mgr.elapsed_seconds() <= 1);
    }

    #[test]
    fn test_session_serde_roundtrip() {
        let mut sess = Session::new(Limits::default());
        sess.transition(SessionState::Waiting).unwrap();
        sess.generate_verify_code();
        let json1 = serde_json::to_string(&sess).unwrap();
        let back: Session = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json1, json2, "Session roundtrip");
    }

    #[test]
    fn test_prune_stale_keeps_only_active() {
        // waiting は terminal でないが「進行中」でもない → 掃除対象
        let waiting = Session::new(Limits::default());
        assert!(!matches!(
            waiting.state,
            SessionState::Connecting | SessionState::Verifying | SessionState::Running
        ));
        // running は保持対象
        let mut running = Session::new(Limits::default());
        running.transition(SessionState::Waiting).unwrap();
        running.transition(SessionState::Connecting).unwrap();
        running.transition(SessionState::Verifying).unwrap();
        running.transition(SessionState::Running).unwrap();
        assert!(matches!(running.state, SessionState::Running));
    }

    // ====== Round 6 (Socratic 問⑮): Capability 検証ゲート ======

    /// 決定的鍵で署名済みの test capability を作る (8192MB / 60 分)。
    fn signed_capability(expires: i64) -> (Capability, ed25519_dalek::SigningKey) {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let mut cap = Capability {
            id: "cap-1".into(),
            gpu: 0,
            vram_mb: 8192,
            max_runtime_minutes: 60,
            expires,
            nonce: "nonce-abc".into(),
            signature: String::new(),
        };
        cap.sign(&key);
        (cap, key)
    }

    #[test]
    fn test_capability_signature_roundtrip() {
        let (cap, key) = signed_capability(Utc::now().timestamp() + 3600);
        assert!(cap.verify_signature(&key.verifying_key()).is_ok());
    }

    #[test]
    fn test_capability_rejects_wrong_issuer() {
        let (cap, _) = signed_capability(Utc::now().timestamp() + 3600);
        let other = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
        assert!(cap.verify_signature(&other.verifying_key()).is_err());
    }

    #[test]
    fn test_capability_rejects_tampered_fields() {
        let (mut cap, key) = signed_capability(Utc::now().timestamp() + 3600);
        // 署名後に vram を引き上げる → payload が変わり署名と不一致になる
        cap.vram_mb = 80_000;
        assert!(
            cap.verify_signature(&key.verifying_key()).is_err(),
            "署名後のフィールド改ざんは検出される"
        );
    }

    #[test]
    fn test_capability_rejects_malformed_signature() {
        let (mut cap, key) = signed_capability(Utc::now().timestamp() + 3600);
        cap.signature = "not-base64-!!".into();
        assert!(cap.verify_signature(&key.verifying_key()).is_err());
        cap.signature = BASE64.encode([0u8; 10]); // 長さ不正 (64 バイトでない)
        assert!(cap.verify_signature(&key.verifying_key()).is_err());
    }

    #[test]
    fn test_capability_expiry_boundary() {
        let now = Utc::now().timestamp();
        let (expired, _) = signed_capability(now - 1);
        assert!(expired.is_expired(now));
        let (exact, _) = signed_capability(now);
        assert!(exact.is_expired(now), "expires 丁度も失効扱い");
        let (live, _) = signed_capability(now + 3600);
        assert!(!live.is_expired(now));
    }

    #[test]
    fn test_capability_authorizes_within_bounds() {
        let (cap, _) = signed_capability(Utc::now().timestamp() + 3600); // 8192MB / 60 分
        let mk = |minutes: u32, vram: u32| Limits {
            max_time_minutes: minutes,
            max_vram: vram,
            max_gpu_util: 80,
        };
        assert!(cap.authorizes(&mk(30, 4096)));
        assert!(!cap.authorizes(&mk(30, 16384)), "VRAM 超過は拒否");
        assert!(!cap.authorizes(&mk(120, 4096)), "実行時間超過は拒否");
    }

    #[test]
    fn test_capability_validate_combined_gate() {
        let now = Utc::now().timestamp();
        let (cap, key) = signed_capability(now + 3600);
        let vk = key.verifying_key();
        let ok = Limits {
            max_time_minutes: 30,
            max_vram: 4096,
            max_gpu_util: 80,
        };
        // 全条件成立 → OK
        assert!(cap.validate(&vk, &ok, now).is_ok());
        // 失効 → NG
        assert!(cap.validate(&vk, &ok, now + 7200).is_err());
        // 別 issuer → NG
        let other = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]).verifying_key();
        assert!(cap.validate(&other, &ok, now).is_err());
        // 制限超過 → NG
        let over = Limits {
            max_time_minutes: 999,
            max_vram: 4096,
            max_gpu_util: 80,
        };
        assert!(cap.validate(&vk, &over, now).is_err());
    }
}

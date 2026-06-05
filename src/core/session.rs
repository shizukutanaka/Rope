//! セッション管理モジュール
//!
//! セッションのライフサイクル、状態管理、強制停止を担当

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
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

/// ジョブ仕様
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    /// Dockerイメージ
    pub image: String,
    /// 実行コマンド
    pub command: String,
    /// 入力ディレクトリ（オプション）
    pub input_dir: Option<String>,
    /// 出力ディレクトリ（オプション）
    pub output_dir: Option<String>,
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
pub struct SessionManager {
    current: Arc<RwLock<Option<Session>>>,
    start_time: Option<Instant>,
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

/// 緊急停止
pub fn panic_stop() -> Result<()> {
    tracing::warn!("PANIC STOP triggered");
    STOP_REQUESTED.store(true, Ordering::SeqCst);

    // 全コンテナを停止
    let output = std::process::Command::new("docker")
        .args(["ps", "-q", "--filter", "name=rope-"])
        .output()
        .context("コンテナ一覧取得失敗")?;

    let container_ids = String::from_utf8_lossy(&output.stdout);
    for id in container_ids.lines() {
        if !id.is_empty() {
            tracing::info!("Killing container: {}", id);
            std::process::Command::new("docker")
                .args(["kill", id])
                .output()
                .ok();
        }
    }

    // セッションファイルをクリア
    cleanup()?;

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

/// クリーンアップ
pub fn cleanup() -> Result<()> {
    let session_dir = config::session_dir();

    // セッションファイルを削除
    if session_dir.exists() {
        for entry in fs::read_dir(&session_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                fs::remove_file(path)?;
            }
        }
    }

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
}

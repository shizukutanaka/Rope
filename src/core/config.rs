//! 設定管理モジュール
//!
//! 初期化、鍵生成、設定ファイルの読み書きを担当

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 設定構造体
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// ノード設定
    pub node: NodeConfig,
    /// セキュリティ設定
    pub security: SecurityConfig,
    /// デフォルト制限
    pub limits: DefaultLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeConfig {
    /// ノード識別子（UUIDv7）
    pub id: String,
    /// 公開鍵フィンガープリント
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// ネットワークアクセス許可（デフォルト: none）
    pub allow_network: NetworkMode,
    /// 信頼済みイメージのみ許可
    pub trusted_images_only: bool,
    /// 最大温度上限（℃）
    pub max_temperature: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    None,
    Limited,
    Full,
}

impl std::fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkMode::None => write!(f, "無効"),
            NetworkMode::Limited => write!(f, "制限"),
            NetworkMode::Full => write!(f, "全開"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefaultLimits {
    /// デフォルト最大実行時間（分）
    pub default_time: u32,
    /// デフォルトVRAM上限（MB）
    pub default_vram_mb: u32,
    /// デフォルトGPU使用率上限（%）
    pub default_gpu_util: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            node: NodeConfig {
                id: uuid::Uuid::now_v7().to_string(),
                fingerprint: String::new(),
            },
            security: SecurityConfig {
                allow_network: NetworkMode::None,
                trusted_images_only: true,
                max_temperature: 85,
            },
            limits: DefaultLimits {
                default_time: 60,
                default_vram_mb: 8192,
                default_gpu_util: 80,
            },
        }
    }
}

/// 設定ディレクトリのパスを取得
/// 設定ディレクトリ — `~/.rope`
///
/// Apple 流: panic しない。home directory が取得できない環境
/// (コンテナ、CI、制限環境) では CWD にフォールバック。
/// macOS が /tmp にフォールバックするのと同じ。
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| {
            // ホームディレクトリ不在 (docker scratch, CI 等)
            // CWD/.rope にフォールバック
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        })
        .join(".rope")
}

/// 設定ファイルのパスを取得
pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// アトミックなファイル書込み (一時ファイル → rename)
///
/// `fs::write` は途中中断 (kill / ディスク満杯) で半端な内容を残し、
/// 次回読込で破損扱いになる。tmp に書いて rename すれば、rename は
/// 多くの OS で atomic なので「全部書けた」か「元のまま」のどちらかになる。
/// 並行書込みでも JSON が中途半端に混ざらない (G5: 並行安全性の最低保証)。
pub fn atomic_write(path: &std::path::Path, content: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    fs::write(&tmp, content).context("一時ファイル書込失敗")?;
    fs::rename(&tmp, path).context("rename 失敗")?;
    Ok(())
}

/// プロセス間アドバイザリロック (RAII)
///
/// `load → 変更 → save` を跨いだ排他に使う。複数の `rope` プロセスが
/// 同じ JSON を read-modify-write して互いの変更を失う競合を防ぐ (G5)。
///
/// 実装: ロックファイルの atomic create (`create_new` = O_EXCL)。
/// 既存ロックがあれば PID 死活を確認し、死んでいれば奪取 (stale lock 回収)。
/// 外部依存ゼロ (I6) — flock crate を使わず std のみ。
/// Drop でロックファイルを削除する。
pub struct LockGuard {
    path: PathBuf,
}

impl LockGuard {
    /// 名前付きロックを取得。最大 ~2 秒待って取れなければ Err。
    pub fn acquire(name: &str) -> Result<Self> {
        let dir = config_dir().join("lock");
        fs::create_dir_all(&dir).ok();
        let path = dir.join(format!("{}.lock", name));

        for _ in 0..20 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    use std::io::Write;
                    write!(f, "{}", std::process::id()).ok();
                    return Ok(LockGuard { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // stale lock 判定: PID が死んでいれば奪取。
                    // 問⑳: 旧実装は attempt==0 のみ判定していたため、保持プロセスが
                    // 取得待ちの途中 (~2 秒) でクラッシュすると stale が永久に検出されず、
                    // 全タイムアウトを待った末に「別プロセスが保持中」で失敗していた。
                    // 毎試行で判定して dead holder を即座に奪取する。
                    if Self::try_reclaim_stale(&path) {
                        continue; // 奪取成功 → 即座に再 create_new
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => return Err(e.into()),
            }
        }
        anyhow::bail!("ロック取得失敗 ({}): 別プロセスが保持中", name)
    }

    /// stale (保持プロセス死亡) なら奪取を試みる。奪取できたら true。
    ///
    /// 削除前に PID を再読込し、最初に観測した dead PID と一致する場合のみ削除する。
    /// これにより「A が stale 判定 → B が奪取して live lock 作成 → A が B の live lock を
    /// 削除」という二重奪取レースの窓を狭める (std のみでは完全排除は不可)。
    fn try_reclaim_stale(path: &std::path::Path) -> bool {
        let pid = match fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
        {
            Some(p) => p,
            None => {
                // PID 読めない = 壊れたロック。そのまま消して奪取を試みる。
                return fs::remove_file(path).is_ok();
            }
        };
        if !Self::is_process_dead(pid, path) {
            return false; // 保持プロセスは生存中 — 奪取しない
        }
        // 削除直前に再読込し、同じ dead PID のままか確認する (レース緩和)。
        match fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
        {
            Some(p) if p == pid => fs::remove_file(path).is_ok(),
            _ => false, // 別プロセスが既に奪取/書換え済み — 触らない
        }
    }

    /// PID が死亡しているか。`/proc` がある Linux では PID を直接確認する。
    #[cfg(target_os = "linux")]
    fn is_process_dead(pid: u32, _path: &std::path::Path) -> bool {
        // /proc/<pid> 存在チェック。無ければプロセス死亡とみなす。
        !std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }

    /// 非 Linux (macOS / Windows 等) には `/proc` が無く PID 死活を移植性高く
    /// 確認できない。生存プロセスは Drop でロックを消すため、十分古いロック
    /// ファイルは異常終了の残骸とみなす保守的フォールバックを使う。
    ///
    /// これにより、旧実装が非 Linux で「常に stale」と誤判定して相互排他を
    /// 壊していた問題を防ぐ (生存ロックを奪わない)。
    #[cfg(not(target_os = "linux"))]
    fn is_process_dead(_pid: u32, path: &std::path::Path) -> bool {
        const STALE_AFTER_SECS: u64 = 120;
        match fs::metadata(path).and_then(|m| m.modified()) {
            Ok(mtime) => mtime
                .elapsed()
                .map(|age| age.as_secs() >= STALE_AFTER_SECS)
                .unwrap_or(false),
            Err(_) => true, // メタデータ読めない = 壊れている = stale
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        fs::remove_file(&self.path).ok();
    }
}

/// 破損 JSON 耐性のある永続データ読込
///
/// ファイル不在 → デフォルト。破損 (JSON parse 失敗) → 壊れたファイルを
/// `.corrupt-<timestamp>` に退避し、明確な日本語メッセージを出してデフォルトで続行。
/// 生の serde エラーをユーザーに見せない (G7.3: 異常値で破綻しない)。
/// run/pair/earn で挙動が割れていたのを 1 箇所に統一。
pub fn load_or_recover<T>(path: &std::path::Path, label: &str) -> T
where
    T: serde::de::DeserializeOwned + Default,
{
    if !path.exists() {
        return T::default();
    }
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("⚠️  {} の読込失敗 ({}). 初期状態で続行.", label, e);
            return T::default();
        }
    };
    match serde_json::from_str::<T>(&content) {
        Ok(v) => v,
        Err(_) => {
            // 壊れたファイルを退避 (上書き消失を防ぐ)
            let backup = path.with_extension(format!("corrupt-{}", chrono::Utc::now().timestamp()));
            fs::rename(path, &backup).ok();
            eprintln!(
                "⚠️  {} が破損. {} に退避し初期状態で続行.",
                label,
                backup
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("backup")
            );
            T::default()
        }
    }
}

/// 秘密鍵ファイルのパスを取得
pub fn private_key_path() -> PathBuf {
    config_dir().join("identity.key")
}

/// 公開鍵ファイルのパスを取得
pub fn public_key_path() -> PathBuf {
    config_dir().join("identity.pub")
}

/// セッションディレクトリのパスを取得
pub fn session_dir() -> PathBuf {
    config_dir().join("sessions")
}

/// ログディレクトリのパスを取得
pub fn log_dir() -> PathBuf {
    config_dir().join("logs")
}

/// 初期化処理
///
/// - 設定ディレクトリ作成
/// - Ed25519鍵ペア生成
/// - 設定ファイル作成
pub fn init() -> Result<()> {
    let dir = config_dir();

    // ディレクトリ作成
    fs::create_dir_all(dir).context("設定ディレクトリ作成失敗")?;
    fs::create_dir_all(session_dir()).context("セッションディレクトリ作成失敗")?;
    fs::create_dir_all(log_dir()).context("ログディレクトリ作成失敗")?;

    // 鍵が存在しない場合のみ生成
    if !private_key_path().exists() {
        generate_keypair()?;
    }

    // 設定ファイルが存在しない場合のみ作成
    if !config_path().exists() {
        let mut config = Config::default();

        // フィンガープリントを設定
        let pubkey = load_public_key()?;
        config.node.fingerprint = compute_fingerprint(&pubkey);

        save_config(&config)?;
    }

    Ok(())
}

/// Ed25519鍵ペアを生成
fn generate_keypair() -> Result<()> {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();

    // 秘密鍵を保存（Base64エンコード）
    let private_bytes = signing_key.to_bytes();
    let private_b64 = BASE64.encode(private_bytes);

    // 作成時点で 0o600。fs::write → set_mode の二段だと、作成〜権限変更の
    // 窓で他ユーザーが鍵を読める TOCTOU が生じる。O_CREAT のモードで最初から絞る。
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(private_key_path())
            .context("秘密鍵書込失敗")?;
        f.write_all(private_b64.as_bytes())
            .context("秘密鍵書込失敗")?;
        // 既存ファイルが緩い権限だった場合に備え明示的に再設定
        use std::os::unix::fs::PermissionsExt;
        let mut perms = f.metadata()?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(private_key_path(), perms)?;
    }
    #[cfg(not(unix))]
    {
        fs::write(private_key_path(), &private_b64).context("秘密鍵書込失敗")?;
    }

    // 公開鍵を保存（Base64エンコード）
    let public_bytes = verifying_key.to_bytes();
    let public_b64 = BASE64.encode(public_bytes);
    fs::write(public_key_path(), public_b64).context("公開鍵書込失敗")?;

    tracing::info!("Generated new Ed25519 keypair");

    Ok(())
}

/// 公開鍵をロード
pub fn load_public_key() -> Result<VerifyingKey> {
    let b64 = fs::read_to_string(public_key_path()).context("公開鍵読込失敗")?;
    let bytes = BASE64.decode(b64.trim()).context("公開鍵デコード失敗")?;
    let key_bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("公開鍵長不正"))?;
    VerifyingKey::from_bytes(&key_bytes).context("公開鍵パース失敗")
}

/// 公開鍵からフィンガープリントを計算（短縮表示用）
pub fn compute_fingerprint(pubkey: &VerifyingKey) -> String {
    let hash = blake3::hash(pubkey.as_bytes());
    let bytes = hash.as_bytes();
    // 最初の4バイトをHEX表示（例: AB:7F:2C:91）
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

/// 設定を保存
pub fn save_config(config: &Config) -> Result<()> {
    let content = serde_json::to_string_pretty(config).context("設定シリアライズ失敗")?;
    atomic_write(&config_path(), &content)
}

/// 初期化済みかチェック
pub fn is_initialized() -> bool {
    config_path().exists() && private_key_path().exists()
}

/// 設定状態を 1 画面フォーマット
///
/// 他 6 モジュールと同じ format_* パターンに揃える。
/// Apple の System Settings は全ペイン同じレイアウト — Rope も同じ。
pub fn format_config(config: &Config) -> String {
    let mut out = String::new();
    out.push_str("設定:\n");
    out.push_str("═══════════════════════════════════════════════════\n");
    out.push_str(&format!("  ノード ID: {}\n", config.node.id));
    out.push_str(&format!(
        "  フィンガープリント: {}\n",
        config.node.fingerprint
    ));
    out.push_str(&format!(
        "  ネットワーク: {}\n",
        config.security.allow_network
    ));
    out.push_str(&format!(
        "  信頼済イメージのみ: {}\n",
        if config.security.trusted_images_only {
            "はい"
        } else {
            "いいえ"
        }
    ));
    out.push_str(&format!(
        "  最大温度: {}℃\n",
        config.security.max_temperature
    ));
    out.push_str(&format!(
        "  セッション上限: {} 分\n",
        config.limits.default_time
    ));
    out.push_str(&format!(
        "  VRAM 上限: {} MB\n",
        config.limits.default_vram_mb
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_format() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        let fp = compute_fingerprint(&verifying_key);
        assert_eq!(fp.len(), 11); // XX:XX:XX:XX
        assert!(fp.contains(':'));
    }

    #[test]
    fn test_fingerprint_deterministic() {
        let mut csprng = OsRng;
        let key = SigningKey::generate(&mut csprng).verifying_key();
        let fp1 = compute_fingerprint(&key);
        let fp2 = compute_fingerprint(&key);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_fingerprints_differ_for_different_keys() {
        let mut csprng = OsRng;
        let k1 = SigningKey::generate(&mut csprng).verifying_key();
        let k2 = SigningKey::generate(&mut csprng).verifying_key();
        assert_ne!(compute_fingerprint(&k1), compute_fingerprint(&k2));
    }

    #[test]
    fn test_paths_are_under_config_dir() {
        let base = config_dir();
        assert!(config_path().starts_with(&base));
        assert!(private_key_path().starts_with(&base));
        assert!(public_key_path().starts_with(&base));
        assert!(session_dir().starts_with(&base));
        assert!(log_dir().starts_with(&base));
    }

    #[test]
    fn test_paths_are_distinct() {
        // 重要なパスは互いに重ならない
        assert_ne!(config_path(), private_key_path());
        assert_ne!(private_key_path(), public_key_path());
        assert_ne!(session_dir(), log_dir());
    }

    #[test]
    fn test_format_config_contains_key_fields() {
        let config = Config {
            node: NodeConfig {
                id: "test-node-id".to_string(),
                fingerprint: "fp:aa:bb:cc".to_string(),
            },
            security: SecurityConfig {
                allow_network: NetworkMode::Limited,
                trusted_images_only: true,
                max_temperature: 85,
            },
            limits: DefaultLimits {
                default_time: 120,
                default_vram_mb: 8192,
                default_gpu_util: 80,
            },
        };
        let out = format_config(&config);
        assert!(out.contains('═'), "ヘッダ罫線");
        assert!(out.contains("test-node-id"), "ノード ID");
        assert!(out.contains("fp:aa:bb:cc"), "フィンガープリント");
        assert!(out.contains("制限"), "ネットワークモード");
        assert!(out.contains("はい"), "信頼済イメージ");
        assert!(out.contains("85℃"), "温度");
        assert!(out.contains("120"), "セッション上限");
    }

    #[test]
    fn test_format_config_trusted_images_no() {
        let config = Config {
            node: NodeConfig {
                id: "n".into(),
                fingerprint: "f".into(),
            },
            security: SecurityConfig {
                allow_network: NetworkMode::None,
                trusted_images_only: false,
                max_temperature: 90,
            },
            limits: DefaultLimits {
                default_time: 60,
                default_vram_mb: 4096,
                default_gpu_util: 80,
            },
        };
        let out = format_config(&config);
        assert!(out.contains("いいえ"));
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let config = Config::default();
        let json1 = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json1, json2, "Config roundtrip");
    }

    #[test]
    fn test_load_or_recover_corrupt_backs_up_and_defaults() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("rope_recover_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.json");
        // 破損 JSON を書く
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"{not valid json").unwrap();
        drop(f);

        // 破損 → デフォルトに回復
        let recovered: Config = load_or_recover(&path, "test");
        assert_eq!(
            recovered.node.fingerprint,
            Config::default().node.fingerprint
        );

        // 破損ファイルが退避されている (元のパスは消えている)
        assert!(!path.exists(), "破損ファイルは退避されるべき");
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("corrupt"))
            .collect();
        assert_eq!(backups.len(), 1, "退避ファイルが 1 つあるべき");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_or_recover_missing_returns_default() {
        let path = std::env::temp_dir().join("rope_nonexistent_xyz.json");
        std::fs::remove_file(&path).ok();
        let v: Config = load_or_recover(&path, "test");
        assert_eq!(v.node.fingerprint, Config::default().node.fingerprint);
    }

    #[test]
    fn test_atomic_write_no_temp_residue() {
        let dir = std::env::temp_dir().join(format!("rope_atomic_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.json");
        atomic_write(&path, "{\"x\":1}").unwrap();
        // 書けた内容が正しい
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"x\":1}");
        // tmp ファイルが残っていない
        let tmp_residue = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tmp-"))
            .count();
        assert_eq!(tmp_residue, 0, "tmp ファイルは rename 後消えるべき");
        // 上書きも atomic
        atomic_write(&path, "{\"x\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"x\":2}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_lock_acquire_release_reacquire() {
        // 一意な名前で衝突回避
        let name = format!("test_lock_{}", std::process::id());
        // 取得 → Drop で解放 → 再取得できる
        {
            let _g = LockGuard::acquire(&name).expect("初回取得");
            // 保持中はロックファイルが存在
            let lock_path = config_dir().join("lock").join(format!("{}.lock", name));
            assert!(lock_path.exists(), "保持中はロックファイルが存在");
        }
        // Drop 後は再取得可能
        let g2 = LockGuard::acquire(&name).expect("解放後の再取得");
        drop(g2);
        // クリーンアップ確認
        let lock_path = config_dir().join("lock").join(format!("{}.lock", name));
        assert!(!lock_path.exists(), "Drop 後はロックファイルが消える");
    }

    #[test]
    fn test_lock_stale_reclaim() {
        let name = format!("test_stale_{}", std::process::id());
        let dir = config_dir().join("lock");
        std::fs::create_dir_all(&dir).ok();
        let lock_path = dir.join(format!("{}.lock", name));
        // 死んだ PID を書いた stale lock を作る
        std::fs::write(&lock_path, "999999999").unwrap();
        // stale なので奪取できる
        let g = LockGuard::acquire(&name).expect("stale lock を奪取");
        drop(g);
        std::fs::remove_file(&lock_path).ok();
    }

    /// 問⑳: try_reclaim_stale は dead PID を奪取し、live PID (= 自プロセス) は保護する。
    #[test]
    fn test_try_reclaim_stale_protects_live_holder() {
        let dir = config_dir().join("lock");
        std::fs::create_dir_all(&dir).ok();

        // dead PID → 奪取できる (ファイル削除)
        let dead_path = dir.join(format!("reclaim_dead_{}.lock", std::process::id()));
        std::fs::write(&dead_path, "999999999").unwrap();
        assert!(
            LockGuard::try_reclaim_stale(&dead_path),
            "dead holder は奪取できる"
        );
        assert!(!dead_path.exists(), "奪取後はロックファイルが消える");

        // live PID (自分自身) → 奪取しない (保護)
        let live_path = dir.join(format!("reclaim_live_{}.lock", std::process::id()));
        std::fs::write(&live_path, format!("{}", std::process::id())).unwrap();
        assert!(
            !LockGuard::try_reclaim_stale(&live_path),
            "生存プロセスのロックは奪取してはならない"
        );
        assert!(live_path.exists(), "生存ロックは消されない");
        std::fs::remove_file(&live_path).ok();
    }

    /// 問⑳: 壊れた (PID 読めない) ロックは奪取して消す。
    #[test]
    fn test_try_reclaim_stale_removes_corrupt_lock() {
        let dir = config_dir().join("lock");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join(format!("reclaim_corrupt_{}.lock", std::process::id()));
        std::fs::write(&path, "not-a-pid").unwrap();
        assert!(
            LockGuard::try_reclaim_stale(&path),
            "壊れたロックは奪取できる"
        );
        assert!(!path.exists());
    }
}

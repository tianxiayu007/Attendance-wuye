//! 界面配置项的持久化。
//!
//! 设计目标：
//! - 程序在**没有任何配置文件**时也能正常使用（全部使用内置默认值）；
//! - 用户在界面上修改了默认项（自动生成 / 上午打卡点 / 下午打卡点）后，
//!   把这些值写入程序同目录的 `settings.json`，下次启动自动读取；
//! - 与 `baobiao.json`（人员名单，属于真实数据、不随仓库分发）彻底解耦，
//!   各自独立读写，避免界面改配置时误动人员名单。

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::config::{self, parse_clock};

/// 界面配置文件名称（仅保存界面上的可配置项）。
pub const SETTINGS_FILE_NAME: &str = "settings.json";

/// 界面上的可配置项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserSettings {
    /// 拖入文件后是否自动生成报表
    pub auto_run: bool,
    /// 上午打卡点（HH:MM）
    pub morning_time: String,
    /// 下午打卡点（HH:MM）
    pub afternoon_time: String,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            auto_run: true,
            morning_time: "08:00".to_string(),
            afternoon_time: "18:00".to_string(),
        }
    }
}

impl UserSettings {
    /// 校验并归一化打卡点，防止把非法值写进配置文件。
    pub fn validate(&mut self) -> Result<()> {
        self.morning_time = normalize(&self.morning_time, "上午打卡点")?;
        self.afternoon_time = normalize(&self.afternoon_time, "下午打卡点")?;
        Ok(())
    }
}

fn normalize(value: &str, label: &str) -> Result<String> {
    let text = value.trim();
    if text.is_empty() {
        return Err(anyhow!("{}不能为空", label));
    }
    parse_clock(text)
        .map(|time| time.format("%H:%M").to_string())
        .ok_or_else(|| anyhow!("{}格式不正确（应为 HH:MM，如 08:00）：{}", label, text))
}

/// 界面配置的读写位置：与 `baobiao.json` 相同的查找目录，优先可执行文件目录。
///
/// - 若某个目录里**已经存在** `settings.json`，就继续用它（保证升级后仍读旧位置）；
/// - 否则取第一个可用目录（可执行文件目录 / macOS 的 .app 所在目录 / 当前工作目录）。
pub fn settings_path() -> PathBuf {
    if let Some(existing) = config::config_search_dirs()
        .into_iter()
        .map(|dir| dir.join(SETTINGS_FILE_NAME))
        .find(|path| path.is_file())
    {
        return existing;
    }

    config::preferred_config_dir().join(SETTINGS_FILE_NAME)
}

/// 读取界面配置；文件不存在时返回内置默认值（不报错，保证无配置也能运行）。
pub fn load() -> UserSettings {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text, &path),
        Err(_) => UserSettings::default(),
    }
}

/// 读取指定文件（供测试与显式路径场景使用）。
pub fn load_from(path: &Path) -> UserSettings {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text, path),
        Err(_) => UserSettings::default(),
    }
}

/// 解析界面配置文本：
/// - 缺失字段用内置默认值补齐；
/// - 非法的时间值单独回退为默认值，**不影响其它已保存的配置**。
fn parse(text: &str, path: &Path) -> UserSettings {
    let mut settings: UserSettings = match serde_json::from_str(text) {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("解析界面配置 {} 失败，使用默认值：{}", path.display(), e);
            return UserSettings::default();
        }
    };

    let defaults = UserSettings::default();
    if parse_clock(&settings.morning_time).is_none() {
        settings.morning_time = defaults.morning_time;
    }
    if parse_clock(&settings.afternoon_time).is_none() {
        settings.afternoon_time = defaults.afternoon_time;
    }

    // 统一成 HH:MM
    let _ = settings.validate();
    settings
}

/// 写入界面配置（首次修改默认项时创建该文件）。
pub fn save(settings: &UserSettings) -> Result<PathBuf> {
    let mut settings = settings.clone();
    settings.validate()?;

    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录 {} 失败", parent.display()))?;
    }

    let text = serde_json::to_string_pretty(&settings)?;
    std::fs::write(&path, format!("{}\n", text))
        .with_context(|| format!("写入 {} 失败（目录是否只读？）", path.display()))?;

    Ok(path)
}

/// 写入指定文件（供测试使用）。
pub fn save_to(path: &Path, settings: &UserSettings) -> Result<()> {
    let mut settings = settings.clone();
    settings.validate()?;
    let text = serde_json::to_string_pretty(&settings)?;
    std::fs::write(path, format!("{}\n", text))
        .with_context(|| format!("写入 {} 失败", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_documented_values() {
        let settings = UserSettings::default();
        assert!(settings.auto_run);
        assert_eq!(settings.morning_time, "08:00");
        assert_eq!(settings.afternoon_time, "18:00");
    }

    #[test]
    fn missing_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join("attendance-wuye-test-missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SETTINGS_FILE_NAME);

        assert_eq!(load_from(&path), UserSettings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = std::env::temp_dir().join("attendance-wuye-test-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SETTINGS_FILE_NAME);

        let settings = UserSettings {
            auto_run: false,
            morning_time: "8:30".to_string(),
            afternoon_time: "17:30:00".to_string(),
        };
        save_to(&path, &settings).unwrap();

        // 归一化为 HH:MM 后原样读回
        let loaded = load_from(&path);
        assert!(!loaded.auto_run);
        assert_eq!(loaded.morning_time, "08:30");
        assert_eq!(loaded.afternoon_time, "17:30");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_times_are_rejected_on_save_and_recovered_on_load() {
        let mut settings = UserSettings::default();
        settings.morning_time = "25:00".to_string();
        assert!(settings.validate().is_err());

        let dir = std::env::temp_dir().join("attendance-wuye-test-invalid");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SETTINGS_FILE_NAME);
        std::fs::write(
            &path,
            r#"{ "auto_run": false, "morning_time": "oops", "afternoon_time": "18:00" }"#,
        )
        .unwrap();

        let loaded = load_from(&path);
        assert_eq!(loaded.morning_time, "08:00"); // 非法值回退默认
        assert_eq!(loaded.afternoon_time, "18:00"); // 合法值保留
        assert!(!loaded.auto_run);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn settings_path_points_next_to_executable() {
        let path = settings_path();
        let exe_dir = std::env::current_exe()
            .expect("test 可执行文件路径可用")
            .parent()
            .expect("有父目录")
            .to_path_buf();

        assert_eq!(path.file_name().unwrap(), SETTINGS_FILE_NAME);
        // 可执行文件目录或（开发时的）源码目录，二者都属于“程序所在位置”
        let parent = path.parent().unwrap();
        let ok = parent == exe_dir
            || parent.ends_with("src-tauri")
            || parent == std::env::current_dir().unwrap();
        assert!(ok, "settings.json 位置异常: {}", path.display());
    }
}

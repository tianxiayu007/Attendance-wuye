pub mod cli;
pub mod config;
pub mod report;
pub mod settings;

use std::path::{Path, PathBuf};
use tauri::Manager;

/// 界面启动时读取界面配置（自动生成开关、上/下午打卡点）。
///
/// 返回 `{ path, exists, settings }`：
/// - `exists=false` 表示还没有 `settings.json`，界面用内置默认值即可；
/// - 不影响 `baobiao.json`（人员名单）的读取。
#[tauri::command]
fn load_settings() -> serde_json::Value {
    let path = settings::settings_path();
    let exists = path.is_file();
    let current = settings::load();

    serde_json::json!({
        "path": path.display().to_string(),
        "exists": exists,
        "settings": current,
    })
}

/// 把界面上的配置项写入 `settings.json`（文件不存在则创建）。
///
/// 返回写入路径；失败时返回错误文案，界面会提示但不会中断使用。
#[tauri::command]
fn save_settings(auto_run: bool, morning_time: &str, afternoon_time: &str) -> Result<String, String> {
    let payload = settings::UserSettings {
        auto_run,
        morning_time: morning_time.to_string(),
        afternoon_time: afternoon_time.to_string(),
    };

    match settings::save(&payload) {
        Ok(path) => Ok(path.display().to_string()),
        Err(e) => Err(format!("保存界面配置失败：{}", e)),
    }
}

/// 当前生效的配置文件路径（用于界面提示，不要求文件必须存在）。
#[tauri::command]
fn config_file_path() -> String {
    config::active_config_path().display().to_string()
}

/// 生成报表：读取考勤机导出的 report.xls，生成最终考勤报表。
///
/// - `config`：可选的 `baobiao.json` 路径；缺省时自动查找，
///   找不到且界面已传入人员名单时，不依赖配置文件也能生成；
/// - `morning_time` / `afternoon_time`：界面上的打卡点，优先于配置文件；
/// - `roster`：可选的“部门 → 人员列表”JSON（界面将来可从设置文件读取），
///   传入时优先于 `baobiao.json`。
#[tauri::command]
fn attendance_report(
    file: &str,
    config: Option<&str>,
    morning_time: Option<&str>,
    afternoon_time: Option<&str>,
    roster: Option<&str>,
) -> String {
    let src_path = Path::new(file);
    let config_path: Option<PathBuf> = config
        .filter(|c| !c.trim().is_empty())
        .map(PathBuf::from);

    let result = resolve_config(config_path.as_deref(), roster).and_then(|mut cfg| {
        // 界面上的打卡点优先于配置文件
        if let Some(value) = morning_time.filter(|v| !v.trim().is_empty()) {
            cfg.report.set_morning_time(value)?;
        }
        if let Some(value) = afternoon_time.filter(|v| !v.trim().is_empty()) {
            cfg.report.set_afternoon_time(value)?;
        }
        report::handle_report(src_path, &cfg)
    });

    match result {
        Ok(summary) => summary.message(),
        Err(e) => format!("生成报表失败：{}", e),
    }
}

/// 组装生成报表所需的配置：
/// 1. 显式指定或自动查找到 `baobiao.json` → 用它（人员名单 + 报表规则）；
/// 2. 找不到配置文件但界面传入了人员名单 → 只用名单 + 默认报表规则；
/// 3. 都没有 → 给出明确提示。
fn resolve_config(explicit: Option<&Path>, roster: Option<&str>) -> anyhow::Result<config::Config> {
    match config::read_config(explicit) {
        Ok(cfg) => Ok(cfg),
        Err(config_error) => match roster.filter(|r| !r.trim().is_empty()) {
            Some(json) => config::config_from_roster(json).map_err(|e| {
                anyhow::anyhow!("读取配置文件失败（{}），且界面人员名单解析失败：{}", config_error, e)
            }),
            None => Err(config_error),
        },
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 记录内置资源目录，便于查找随安装包分发的默认 baobiao.json
            if let Ok(dir) = app.path().resource_dir() {
                config::set_resource_dir(dir);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            attendance_report,
            load_settings,
            save_settings,
            config_file_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

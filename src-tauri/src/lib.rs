pub mod cli;
pub mod config;
pub mod report;

use std::path::{Path, PathBuf};
use tauri::Manager;

/// 生成报表：直接读取考勤机导出的 report.xls，生成最终考勤报表。
///
/// * `file` - 考勤机导出的 .xls/.xlsx 文件路径（可由文件选择框或拖拽获得）
/// * `config` - 可选的 baobiao.json 路径，缺省时自动查找
#[tauri::command]
fn attendance_report(file: &str, config: Option<&str>) -> String {
    let src_path = Path::new(file);
    let config_path: Option<PathBuf> = config
        .filter(|c| !c.trim().is_empty())
        .map(PathBuf::from);

    let result = config::read_config(config_path.as_deref())
        .and_then(|cfg| report::handle_report(src_path, &cfg));

    match result {
        Ok(summary) => summary.message(),
        Err(e) => format!("生成报表失败：{}", e),
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
        .invoke_handler(tauri::generate_handler![attendance_report])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

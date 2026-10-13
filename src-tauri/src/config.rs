use anyhow::{Result, anyhow};
use indexmap::IndexMap;
use serde::Deserialize;
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 配置文件名称，与可执行文件/项目根目录同级存放。
pub const CONFIG_FILE_NAME: &str = "baobiao.json";

/// 安装包内置资源目录（由 Tauri 在启动时注入）。
static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 记录安装包内置资源目录，便于查找随包分发的默认配置。
pub fn set_resource_dir(dir: PathBuf) {
    let _ = RESOURCE_DIR.set(dir);
}


/// 报表生成规则（可选配置项，缺省时使用结构体默认值）。
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ReportOptions {
    /// 当天打卡人数占总人数的比例达到多少才算合法日期（默认 0.5，即一半）。
    pub valid_ratio: f64,
    /// 比例相等时是否算合法（默认 false：必须“超过”总人数的一半）。
    pub valid_ratio_inclusive: bool,
    /// 下午时段的有效打卡起始时间（HH:MM），早于该时间的末次打卡视为上午缺卡。
    pub afternoon_start: String,
    /// 上午打卡点（HH:MM，默认 08:00）：早于等于该时间的上午打卡视为正常。
    pub morning_time: String,
    /// 下午打卡点（HH:MM，默认 18:00）：晚于等于该时间的下午打卡视为正常。
    pub afternoon_time: String,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            valid_ratio: 0.5,
            valid_ratio_inclusive: false,
            afternoon_start: "12:00".to_string(),
            morning_time: "08:00".to_string(),
            afternoon_time: "18:00".to_string(),
        }
    }
}

impl ReportOptions {
    /// 判断当天打卡人数是否达到合法阈值。
    pub fn is_valid(&self, punched_people: usize, total_people: usize) -> bool {
        if total_people == 0 {
            return false;
        }
        let required = self.valid_ratio * total_people as f64;
        let got = punched_people as f64;
        if self.valid_ratio_inclusive {
            got >= required
        } else {
            got > required
        }
    }

    /// 判断某个时段的打卡时间是否“正常”（时间本身体现为黑色字体）：
    /// - 上午：早于等于 `morning_time`（默认 08:00）算正常；
    /// - 下午：晚于等于 `afternoon_time`（默认 18:00）算正常。
    ///
    /// 缺卡、请假、出差等非时间文本一律视为异常（红色字体）。
    pub fn is_normal_time(&self, period: Period, value: &str) -> bool {
        let Some(time) = parse_hhmm(value) else {
            return false;
        };

        let threshold = match period {
            Period::Morning => parse_hhmm(&self.morning_time),
            Period::Afternoon => parse_hhmm(&self.afternoon_time),
        };
        let Some(threshold) = threshold else {
            // 配置的时间格式异常时不做额外判定，避免把正常考勤全部标红
            return true;
        };

        match period {
            Period::Morning => time <= threshold,
            Period::Afternoon => time >= threshold,
        }
    }
}

/// 报表中的时段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// 上午
    Morning,
    /// 下午
    Afternoon,
}

/// 解析 `HH:MM` / `HH:MM:SS`。
fn parse_hhmm(value: &str) -> Option<chrono::NaiveTime> {
    let text = value.trim();
    chrono::NaiveTime::parse_from_str(text, "%H:%M")
        .or_else(|_| chrono::NaiveTime::parse_from_str(text, "%H:%M:%S"))
        .ok()
}

/// 考勤人员配置：顶层以部门名称分组，部门内按顺序列出人员。
///
/// 既支持旧版扁平结构：
/// ```json
/// { "办公室": { "张三": "zhangsan" } }
/// ```
/// 也支持带报表规则的结构：
/// ```json
/// { "report": { "valid_ratio": 0.5 }, "departments": { "办公室": { "张三": "zhangsan" } } }
/// ```
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub report: ReportOptions,
    pub departments: IndexMap<String, IndexMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    #[serde(default)]
    report: Option<ReportOptions>,
    #[serde(default)]
    departments: Option<IndexMap<String, IndexMap<String, String>>>,
    #[serde(flatten)]
    legacy_departments: IndexMap<String, IndexMap<String, String>>,
}

impl Config {
    /// 配置中登记的全部人员名称（按部门顺序）。
    pub fn all_people(&self) -> Vec<&str> {
        self.departments
            .values()
            .flat_map(|names| names.keys().map(|k| k.as_str()))
            .collect()
    }

    /// 总人数。
    pub fn total_people(&self) -> usize {
        self.departments.values().map(|names| names.len()).sum()
    }
}

/// 读取配置文件（优先使用显式路径，否则按候选顺序查找）。
pub fn read_config(explicit: Option<&Path>) -> Result<Config> {
    let path = match explicit {
        Some(p) => p.to_path_buf(),
        None => find_config_path().ok_or_else(|| {
            anyhow!(
                "配置文件{}不存在，请将配置文件与程序放在同一目录",
                CONFIG_FILE_NAME
            )
        })?,
    };

    let file = std::fs::File::open(&path)
        .map_err(|e| anyhow!("无法打开配置文件 {}: {}", path.display(), e))?;
    let reader = std::io::BufReader::new(file);
    let raw: RawConfig = serde_json::from_reader(reader)
        .map_err(|e| anyhow!("配置文件 {} 格式错误: {}", path.display(), e))?;

    let mut config = Config {
        report: raw.report.unwrap_or_default(),
        departments: IndexMap::new(),
    };

    if let Some(departments) = raw.departments {
        config.departments.extend(departments);
    }
    config.departments.extend(raw.legacy_departments);

    if config.departments.is_empty() {
        return Err(anyhow!(
            "配置文件 {} 中没有配置任何部门/人员",
            path.display()
        ));
    }

    Ok(config)
}

/// 按候选顺序查找配置文件。
fn find_config_path() -> Option<PathBuf> {
    for dir in config_search_dirs() {
        let candidate = dir.join(CONFIG_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// 配置文件查找目录：
/// 可执行文件目录 → macOS 应用目录 → 当前工作目录 → 开发时源码目录 → 安装包内置资源目录。
fn config_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Ok(exe_path) = env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            dirs.push(dir.to_path_buf());
        }
        // macOS: 打包成 .app 后，可执行文件在 XXX.app/Contents/MacOS 下
        if cfg!(target_os = "macos") {
            let mut path = exe_path.clone();
            while path.pop() {
                if path.extension() == Some(OsStr::new("app"))
                    && path.join("Contents/MacOS").exists()
                {
                    if let Some(parent) = path.parent() {
                        dirs.push(parent.to_path_buf());
                    }
                    break;
                }
            }
        }
    }

    if let Ok(cwd) = env::current_dir() {
        dirs.push(cwd);
    }

    // 开发调试时（target/debug/...）回退到源码目录
    if cfg!(debug_assertions)
        && let Ok(exe_path) = env::current_exe()
        && let Some(dir) = exe_path.parent()
    {
        dirs.push(dir.join("../.."));
    }

    // 安装包内置的默认配置（bundle.resources，Tauri 会把上级目录映射为 _up_）
    if let Some(dir) = RESOURCE_DIR.get() {
        dirs.push(dir.join("_up_"));
        dirs.push(dir.clone());
    }

    dirs
}

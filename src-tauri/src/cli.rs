//! 命令行模式：`attendance-wuye <report.xls> [baobiao.json] [上午打卡点] [下午打卡点]`
//!
//! 与界面共用同一套报表逻辑，方便在没有图形环境（或 CI）时验证输出。
//! 后两个参数用于覆盖配置里的打卡点（HH:MM），缺省使用配置文件 / 默认值。

use crate::{config, report};
use std::path::PathBuf;

pub fn run(args: Vec<String>) -> i32 {
    let mut rest = args.into_iter().skip(1);
    let Some(source) = rest.next() else {
        eprintln!(
            "用法: attendance-wuye <report.xls> [baobiao.json] [上午打卡点 HH:MM] [下午打卡点 HH:MM]"
        );
        return 2;
    };
    let config_path = rest.next().map(PathBuf::from);
    let morning_time = rest.next();
    let afternoon_time = rest.next();

    let mut cfg = match config::read_config(config_path.as_deref()) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("读取配置失败：{}", e);
            return 1;
        }
    };

    if let Some(value) = morning_time.filter(|v| !v.trim().is_empty())
        && let Err(e) = cfg.report.set_morning_time(&value)
    {
        eprintln!("上午打卡点无效：{}", e);
        return 2;
    }
    if let Some(value) = afternoon_time.filter(|v| !v.trim().is_empty())
        && let Err(e) = cfg.report.set_afternoon_time(&value)
    {
        eprintln!("下午打卡点无效：{}", e);
        return 2;
    }

    match report::handle_report(std::path::Path::new(&source), &cfg) {
        Ok(summary) => {
            println!("{}", summary.message());
            0
        }
        Err(e) => {
            eprintln!("生成报表失败：{}", e);
            1
        }
    }
}

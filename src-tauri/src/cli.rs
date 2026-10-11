//! 命令行模式：`attendance-wuye <report.xls> [baobiao.json]`
//!
//! 与界面共用同一套报表逻辑，方便在没有图形环境（或 CI）时验证输出。

use crate::{config, report};
use std::path::PathBuf;

pub fn run(args: Vec<String>) -> i32 {
    let mut paths = args.into_iter().skip(1);
    let Some(source) = paths.next() else {
        eprintln!("用法: attendance-wuye <report.xls> [baobiao.json]");
        return 2;
    };
    let config_path = paths.next().map(PathBuf::from);

    let cfg = match config::read_config(config_path.as_deref()) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("读取配置失败：{}", e);
            return 1;
        }
    };

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

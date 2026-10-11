// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // 带参数启动时进入命令行模式，方便脚本化/CI 校验报表输出
    if args.len() > 1 {
        std::process::exit(attendance_wuye_lib::cli::run(args));
    }

    attendance_wuye_lib::run()
}

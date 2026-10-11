# 物业考勤报表（Attendance-wuye）

海康考勤机打卡数据 → 最终考勤报表 的一键生成工具（Tauri 2 + Vue 3 + Rust）。

## 功能

| 功能 | 说明 |
| --- | --- |
| 生成报表 | 直接读取考勤机导出的 `report.xls`，按 `baobiao.json` 的部门/人员顺序生成最终考勤报表 `report_report.xlsx` |
| 文件拖拽 | 支持把 `report.xls` 直接拖进窗口，自动识别本地绝对路径并生成报表 |

> 与旧版 `Attendance` 的差异：
> 1. 取消了“导入考勤”功能，只保留“生成报表”——不再需要先把考勤机数据转换成 `report_oa.xlsx` 交给 OA；
> 2. `baobiao.json` 不再用于生成 OA 中间文件，只用于**报表中的部门归类与人员排序**；
> 3. 日期是否合法改为**当天打卡人数是否超过总人数的一半**（旧版是固定“当天记录数 > 30”）；
> 4. 支持把文件直接拖拽到窗口里，无需手动打开文件选择框。

## 报表规则

1. 解析考勤机导出的 `report.xls`（实际是 HTML 表格，表 class 为 `Punch_Report`），读取 `姓名 / 日期 / 首次签到 / 末次签退`。
2. 统计每个日期“至少有一次打卡”的人数；人数**超过总人数的 `valid_ratio`（默认 0.5，即一半）**时，该日期才作为报表列。总人数取 `baobiao.json` 中登记的人员数。
3. 每人每天两行：`上午`取首次签到时间，`下午`取末次签退时间；末次签退早于 `afternoon_start`（默认 `12:00`）视为上午临时打卡，下午记 `缺卡`。
4. 没有打卡记录的时段记 `缺卡`，异常信息用红色字体，正常时间用黑色字体。
5. 报表按 `baobiao.json` 的部门顺序生成工作表，部门内按人员配置顺序排列，与旧版报表格式保持一致。

## 配置文件 `baobiao.json`

```jsonc
{
  "report": {
    "valid_ratio": 0.5,            // 当天打卡人数占比阈值
    "valid_ratio_inclusive": false, // true 表示“达到一半”即可，false 表示必须“超过一半”
    "afternoon_start": "12:00"      // 下午时段起始时间
  },
  "departments": {
    "领导": { "张三": "zhangsan", "李四": "lisi" }
  }
}
```

- `report` 段可以省略，省略时使用上面的默认值。
- 也兼容旧版扁平结构（顶层直接是部门），此时使用默认报表规则。
- 程序按以下顺序查找配置：可执行文件目录 → macOS `.app` 所在目录 → 当前工作目录 → 开发时源码目录 → 安装包内置资源目录，
  因此把 `baobiao.json` 放在程序（或 `.app`）旁边即可覆盖内置的默认人员名单。

## 开发与构建

```bash
pnpm install
pnpm tauri dev      # 开发调试

pnpm tauri build    # 本地打包
```

命令行模式（与界面共用同一套报表逻辑，便于脚本化验证）：

```bash
cd src-tauri
cargo run -- <report.xls 路径> [baobiao.json 路径]
```

单元测试：

```bash
cd src-tauri
cargo test
```

## 图标

`app-icon.png` 为应用图标源文件，执行下面的命令重新生成各平台图标：

```bash
node create-icon.cjs
```

## 自动打包

仓库已配置 GitHub Actions（`.github/workflows/release.yml`）：
推送到 `main` 分支或手动触发后，会分别构建 Linux / macOS(arm64) / Windows 产物，
运行 Rust 单元测试，并在构建成功后创建 Release 上传压缩包。

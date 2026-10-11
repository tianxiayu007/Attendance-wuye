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

> `baobiao.json` 属于各单位的真实人员数据，**不随仓库分发**（已在 `.gitignore` 中忽略），
> 需要自行按下面的结构创建，并与程序放在同一目录。

```jsonc
{
  "report": {
    "valid_ratio": 0.5,            // 当天打卡人数占比阈值
    "valid_ratio_inclusive": false, // true 表示“达到一半”即可，false 表示必须“超过一半”
    "afternoon_start": "12:00"      // 下午时段起始时间
  },
  "departments": {
    "部门一": { "张三": "zhangsan", "李四": "lisi" },
    "部门二": { "王五": "wangwu" }
  }
}
```

- `departments` 决定报表生成哪些工作表、每个部门包含哪些人员，以及**人员在报表中的排列顺序**；
  值（账号）在本项目中仅作登记、不参与计算，留空字符串也可以。
- `report` 段可以省略，省略时使用上面的默认值。
- 也兼容旧版扁平结构（顶层直接是部门映射），此时使用默认报表规则。
- 程序按以下顺序查找配置：
  可执行文件目录 → macOS `.app` 所在目录 → 当前工作目录 → 开发时源码目录 → 安装包内置资源目录，
  因此把 `baobiao.json` 放在程序（或 `.app`）旁边即可。
- 未找到配置文件时会提示“配置文件 baobiao.json 不存在，请将配置文件与程序放在同一目录”。

## 开发与构建

```bash
pnpm install
pnpm tauri dev      # 开发调试（自动启动 vite 开发服务器）

# 打包：先构建前端产物，再执行打包
pnpm build
pnpm tauri build
```

> 说明：`tauri.conf.json` 中刻意没有配置 `beforeBuildCommand`，
> 打包前需要自行执行 `pnpm build`（CI 中已拆成独立的 “Build frontend” 步骤），
> 这样打包过程不依赖 pnpm 的交互式行为，在 CI/无 TTY 环境下更稳定。

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

## 安装与使用

1. 在 [Releases](https://github.com/tianxiayu007/Attendance-wuye/releases) 下载对应平台的压缩包并解压：
   - Windows：`Attendance-wuye-windows-x86_64.zip` → 解压得到 `Attendance-wuye.exe`
   - macOS：`Attendance-wuye-macos-aarch64.zip`（Apple 芯片）/ `Attendance-wuye-macos-x86_64.zip`（Intel）→ 解压得到 `物业考勤报表.app`
   - Linux：`Attendance-wuye-linux-x86_64.zip` → 解压得到 `.deb` 安装包
2. 按下面的说明创建 `baobiao.json`，放在与程序（macOS 为 `.app`）**同一目录**。
3. 运行程序，把考勤机导出的 `report.xls` 拖进窗口即可生成报表。

### macOS 首次打开提示“已损坏”或“无法验证开发者”

本项目没有 Apple 开发者证书，发布包只做了 ad-hoc 签名（签名只会因文件被篡改而失效，
不会影响功能），因此从浏览器下载后首次打开会被 Gatekeeper 拦截。任选一种方式即可：

- 图形界面：在 `.app` 上**右键 → 打开 → 仍要打开**（只需一次）；若提示“已损坏，无法打开”，
  先在「系统设置 → 隐私与安全性」里点「仍要打开」，或使用下面的命令方式。
- 终端方式（推荐，最稳）：

```bash
# 把 .app 前面的路径换成你的实际解压位置
cd ~/Downloads

# 去掉浏览器下载标记（解决“已损坏，无法打开”）
xattr -dr com.apple.quarantine "物业考勤报表.app"

# 如仍无法打开，重新做一次 ad-hoc 签名（无需开发者证书）
codesign --force --deep --sign - "物业考勤报表.app"

# 打开
open "物业考勤报表.app"
```

### macOS 提示“找不到配置文件 baobiao.json”

把 `baobiao.json` 放在 `.app` 所在目录（即解压出来的那个文件夹根目录），
而不是放进 `.app` 内部。程序的查找顺序是：
可执行文件目录 → `.app` 所在目录 → 当前工作目录 → 安装包内置资源目录。

## 图标

`app-icon.png` 为应用图标源文件，执行下面的命令重新生成各平台图标：

```bash
node create-icon.cjs
```

## 自动打包

仓库已配置 GitHub Actions（`.github/workflows/release.yml`）：
推送到 `main` 分支或手动触发后，会分别构建 Linux / macOS(arm64、x86_64) / Windows 产物，
运行 Rust 单元测试，并在构建成功后创建 Release 上传压缩包。

打包时做了两处针对 macOS 的专门处理，避免下载后提示“已损坏”：

- 用 `codesign --force --deep --sign -` 对 `.app` 做完整的 ad-hoc 签名
  （Tauri 在没有开发者证书时只保留链接器生成的临时签名，信息不完整）；
- 用 `ditto -c -k --sequesterRsrc --keepParent` 生成 macOS 压缩包，
  而不是 `zip`（`zip` 会破坏 `.app` 内的符号链接与签名信息）。

> 如果希望用户双击即可打开、不再需要上述处理，需要 Apple 开发者账号做
> Developer ID 签名 + 公证（notarization），可在 CI 中通过
> `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_ID`、
> `APPLE_PASSWORD`、`APPLE_TEAM_ID` 等 secrets 配置。

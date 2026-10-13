//! 报表生成：直接读取考勤机导出的 report.xls（HTML 表格），生成最终考勤报表。
//!
//! 与旧版 Attendance 的差异：
//! 1. 不再依赖 baobiao.json 做“导入考勤”中间转换，直接由考勤机导出文件生成报表；
//! 2. baobiao.json 仅用于部门归类与人员排序（报表行顺序）；
//! 3. 日期合法性改为“当天打卡人数超过总人数的一半”，而不是固定 30 条记录。

use crate::config::{Config, Period, ReportOptions, parse_clock};
use anyhow::{Result, anyhow};
use calamine::{Cell, Data, Range, Reader, open_workbook_auto};
use chrono::{Datelike, NaiveDate, NaiveTime, Weekday};
use regex::Regex;
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// 考勤记录工作表名（考勤机导出文件里的数据表 class）。
const PUNCH_REPORT_CLASS: &str = "Punch_Report";
/// 考勤机导出文件中的表头列数。
const PUNCH_COLUMNS: usize = 11;

/// 原始打卡记录（仅保留报表需要的字段）。
#[derive(Debug, Clone)]
struct PunchRecord {
    person: String,
    date: NaiveDate,
    /// 首次签到（上午时段）
    first: String,
    /// 末次签退（下午时段）
    last: String,
}

/// 某位员工某天的上/下午取值。
#[derive(Debug, Clone, Default)]
struct DayTimes {
    am: Option<String>,
    pm: Option<String>,
}

/// 生成结果摘要。
#[derive(Debug, Clone)]
pub struct ReportSummary {
    pub output_path: PathBuf,
    pub year: i32,
    pub month: u32,
    pub valid_dates: usize,
    pub sheets: usize,
    pub total_people: usize,
}

impl ReportSummary {
    pub fn message(&self) -> String {
        format!(
            "已生成 {}（{}个部门、{}个有效日期、总人数{}人）",
            self.output_path.display(),
            self.sheets,
            self.valid_dates,
            self.total_people
        )
    }
}

/// 生成报表入口。
pub fn handle_report(src_path: &Path, config: &Config) -> Result<ReportSummary> {
    if !src_path.is_file() {
        return Err(anyhow!("源文件不存在: {}", src_path.display()));
    }

    let ext = src_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_lowercase();

    if ext != "xls" && ext != "xlsx" {
        return Err(anyhow!("仅支持考勤机导出的 .xls/.xlsx 文件"));
    }

    let records = read_punch_records(src_path)?;
    if records.is_empty() {
        return Err(anyhow!("考勤记录为空"));
    }

    let options = &config.report;
    let total_people = config.total_people();

    let (dates, _people_per_date) = pick_valid_dates(&records, options, total_people)?;
    let first_date = *dates.first().unwrap();

    let day_map = group_by_person_and_date(&records);
    let output_path = build_output_path(src_path);

    let mut workbook = Workbook::new();
    let formats = Formats::new();
    let mut sheet_count = 0usize;

    for (dept_name, roster) in &config.departments {
        if roster.is_empty() {
            continue;
        }

        write_department_sheet(
            &mut workbook,
            dept_name,
            roster,
            &dates,
            first_date,
            &day_map,
            options,
            &formats,
        )?;
        sheet_count += 1;
    }

    if sheet_count == 0 {
        return Err(anyhow!("配置文件中没有可生成报表的部门"));
    }

    workbook.save(&output_path)?;

    Ok(ReportSummary {
        output_path,
        year: first_date.year(),
        month: first_date.month(),
        valid_dates: dates.len(),
        sheets: sheet_count,
        total_people,
    })
}

/// 读取考勤机导出的打卡记录。
fn read_punch_records(src_path: &Path) -> Result<Vec<PunchRecord>> {
    let range = read_punch_table(src_path)?;
    let mut records: Vec<PunchRecord> = Vec::new();
    let mut skipped = 0usize;

    // 注意：Punch_Report 表格里没有表头行，第一行就是数据
    //（表头在另一个名为 Detail2 的表格中）。
    for index in 0..range.height() {
        let person = cell_string(&range, index, 2);
        let date_text = cell_string(&range, index, 6);
        let first = cell_string(&range, index, 9);
        let last = cell_string(&range, index, 10);

        if person.is_empty() {
            continue;
        }

        let date = match parse_date(&date_text) {
            Some(date) => date,
            None => {
                skipped += 1;
                continue;
            }
        };

        records.push(PunchRecord {
            person,
            date,
            first: clean_time(&first),
            last: clean_time(&last),
        });
    }

    if records.is_empty() {
        return Err(anyhow!(
            "未在考勤文件中找到打卡记录，请确认是否为考勤机导出的 report.xls（或检查表头格式）"
        ));
    }

    if skipped > 0 {
        eprintln!("已跳过 {} 条日期无法解析的记录", skipped);
    }

    Ok(records)
}

/// 读取考勤机导出文件里的 Punch_Report 表格。
fn read_punch_table(src_path: &Path) -> Result<Range<Data>> {
    // 考勤机导出的 .xls 实际是 HTML 表格，UTF-8 编码。
    if let Ok(raw) = std::fs::read(src_path) {
        let (text, _, had_errors) = encoding_rs::UTF_8.decode(&raw);
        if !had_errors && text.contains(PUNCH_REPORT_CLASS) {
            let rows = parse_html_rows(&text);
            if rows.is_empty() {
                return Err(anyhow!(
                    "未找到考勤数据表，请确认考勤机导出的 .xls 文件是否完整"
                ));
            }
            let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
            let mut cells: Vec<Cell<Data>> = Vec::new();
            for (row_index, mut row) in rows.into_iter().enumerate() {
                row.resize(width, String::new());
                for (col_index, value) in row.into_iter().enumerate() {
                    cells.push(Cell::new(
                        (row_index as u32, col_index as u32),
                        Data::String(value),
                    ));
                }
            }
            return Ok(Range::from_sparse(cells));
        }
    }

    // 回退：真正的 Excel 工作簿
    let mut workbook = open_workbook_auto(src_path)?;
    let sheet_name = workbook
        .sheet_names()
        .iter()
        .find(|name| name.contains("考勤") || name.contains("Punch"))
        .cloned()
        .or_else(|| workbook.sheet_names().first().cloned())
        .ok_or_else(|| anyhow!("考勤文件中没有工作表"))?;

    workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| anyhow!("读取工作表 {} 失败: {}", sheet_name, e))
}

/// 解析 HTML 表格里的单元格文本。
///
/// 考勤机导出的表格结构并不规范（行与行之间没有 `</tr>`，单元格上方的 `<table>` 也未闭合），
/// 因此这里与旧版 Python 实现保持一致：按 `<td>…</td>` 逐个取出文本，再按固定列数切分成行。
fn parse_html_rows(html: &str) -> Vec<Vec<String>> {
    let Some(start) = html.find(PUNCH_REPORT_CLASS) else {
        return Vec::new();
    };
    let Some(end) = html[start..].find("</table>").map(|i| i + start) else {
        return Vec::new();
    };
    let body = &html[start..end];

    let cell_re = Regex::new(r"(?s)<td[^>]*>(.*?)</td>").expect("单元格正则表达式固定有效");
    let cells: Vec<String> = cell_re
        .captures_iter(body)
        .map(|caps| html_to_text(&caps[1]))
        .collect();

    // 按列数切分成行，丢弃列数不足的残行
    cells
        .chunks(PUNCH_COLUMNS)
        .filter(|chunk| chunk.len() == PUNCH_COLUMNS)
        .map(|chunk| chunk.to_vec())
        .collect()
}

/// 去掉 HTML 标签与转义字符，返回单元格纯文本。
fn html_to_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

/// 单元格取值（兼容字符串/数字/日期）。
fn cell_string(range: &Range<Data>, row: usize, index: usize) -> String {
    match range.get((row, index)) {
        Some(Data::String(s)) => s.trim().to_string(),
        Some(Data::Empty) | None => String::new(),
        Some(other) => other.to_string().trim().to_string(),
    }
}

/// 解析日期（支持 `2026-09-01`、`2026/9/1`、Excel 日期序列号）。
fn parse_date(text: &str) -> Option<NaiveDate> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    for fmt in ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(date) = NaiveDate::parse_from_str(text, fmt) {
            return Some(date);
        }
    }

    // Excel 序列号（1899-12-30 为基准）
    if let Ok(serial) = text.parse::<f64>()
        && serial > 0.0
    {
        return NaiveDate::from_num_days_from_ce_opt(serial as i32 + 693_594);
    }

    None
}

/// 考勤机用 `-` 表示未打卡。
fn clean_time(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value == "-" || value == "--" {
        String::new()
    } else {
        value.to_string()
    }
}

/// 取 `HH:MM`（保留分钟精度，与旧版报表一致）。
fn hhmm(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 5 && value.as_bytes().get(2) == Some(&b':') {
        value[..5].to_string()
    } else {
        value.to_string()
    }
}

/// 判断日期是否落在合法的打卡区间（阈值 + 下午时段起始时间）。
fn pick_valid_dates(
    records: &[PunchRecord],
    options: &ReportOptions,
    total_people: usize,
) -> Result<(Vec<NaiveDate>, HashMap<NaiveDate, usize>)> {
    let mut people_per_date: HashMap<NaiveDate, BTreeSet<&str>> = HashMap::new();
    for record in records {
        if record.first.is_empty() && record.last.is_empty() {
            // 当天两次都是缺卡，不计入打卡人数
            continue;
        }
        people_per_date
            .entry(record.date)
            .or_default()
            .insert(record.person.as_str());
    }

    let mut counts: HashMap<NaiveDate, usize> = HashMap::new();
    let mut dates: Vec<NaiveDate> = Vec::new();

    for (date, people) in &people_per_date {
        counts.insert(*date, people.len());
        if options.is_valid(people.len(), total_people) {
            dates.push(*date);
        }
    }

    dates.sort();

    if dates.is_empty() {
        return Err(anyhow!(
            "没有满足合法条件的日期（要求当天打卡人数超过总人数{}人的一半，实际单日最多{}人）",
            total_people,
            people_per_date.values().map(|p| p.len()).max().unwrap_or(0)
        ));
    }

    Ok((dates, counts))
}

/// 汇总每人每天的上午/下午取值。
fn group_by_person_and_date(
    records: &[PunchRecord],
) -> HashMap<(String, NaiveDate), DayTimes> {
    let mut map: HashMap<(String, NaiveDate), DayTimes> = HashMap::new();

    for record in records {
        let entry = map
            .entry((record.person.clone(), record.date))
            .or_default();

        if entry.am.is_none() && !record.first.is_empty() {
            entry.am = Some(hhmm(&record.first));
        }
        if !record.last.is_empty() {
            // 保留最后一条末次签退
            entry.pm = Some(hhmm(&record.last));
        }
    }

    map
}

/// 输出文件：与源文件同目录，命名为 `{源文件名}_report.xlsx`。
fn build_output_path(src_path: &Path) -> PathBuf {
    let stem = src_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "考勤报表".to_string());
    src_path.with_file_name(format!("{}_report.xlsx", stem))
}

/// 星期中文名。
fn weekday_cn(date: &NaiveDate) -> &'static str {
    match date.weekday() {
        Weekday::Mon => "星期一",
        Weekday::Tue => "星期二",
        Weekday::Wed => "星期三",
        Weekday::Thu => "星期四",
        Weekday::Fri => "星期五",
        Weekday::Sat => "星期六",
        Weekday::Sun => "星期日",
    }
}

/// 日期列标题：`9.1`
fn date_label(date: &NaiveDate) -> String {
    format!("{}.{}", date.month(), date.day())
}

/// 生成单个部门的报表工作表。
#[allow(clippy::too_many_arguments)]
fn write_department_sheet(
    workbook: &mut Workbook,
    dept_name: &str,
    roster: &indexmap::IndexMap<String, String>,
    dates: &[NaiveDate],
    first_date: NaiveDate,
    day_map: &HashMap<(String, NaiveDate), DayTimes>,
    options: &ReportOptions,
    formats: &Formats,
) -> Result<()> {
    let date_count = dates.len() as u16;
    let last_col = date_count + 2; // 签名列索引
    let sheet = workbook.add_worksheet().set_name(dept_name)?;

    sheet.set_landscape();
    sheet.set_paper_size(9);
    sheet.set_print_fit_to_pages(1, 0);
    sheet.set_margins(0.1, 0.1, 0.2, 0.2, 0.0, 0.0);
    sheet.set_print_center_horizontally(true);

    // 标题
    let title = format!(
        "{}{}年{}月考勤报表",
        dept_name, first_date.year(), first_date.month()
    );
    sheet.merge_range(0, 0, 0, last_col, &title, &formats.header)?;

    // 表头：日期
    let mut header: Vec<String> = vec![String::new(), String::new()];
    header.extend(dates.iter().map(date_label));
    header.push("签名".to_string());
    let header_refs: Vec<&str> = header.iter().map(|s| s.as_str()).collect();
    sheet.write_row_with_format(1, 0, header_refs, &formats.header)?;

    // 表头：星期
    let mut weekdays: Vec<&str> = vec!["人员名称", "时段"];
    weekdays.extend(dates.iter().map(weekday_cn));
    weekdays.push("");
    sheet.write_row_with_format(2, 0, weekdays, &formats.header)?;

    sheet.merge_range(1, 0, 2, 0, "人员名称", &formats.header)?;
    sheet.merge_range(1, 1, 2, 1, "时段", &formats.header)?;
    sheet.merge_range(1, last_col, 2, last_col, "签名", &formats.header)?;

    let mut row = 3u32;
    for name in roster.keys() {
        sheet.merge_range(row, 0, row + 1, 0, name, &formats.data)?;
        sheet.write_string_with_format(row, 1, "上午", &formats.data)?;
        sheet.write_string_with_format(row + 1, 1, "下午", &formats.data)?;

        for (index, date) in dates.iter().enumerate() {
            let col = index as u16 + 2;
            let times = day_map.get(&(name.clone(), *date)).cloned().unwrap_or_default();

            let (am, pm) = resolve_times(&times, options);
            write_cell(sheet, row, col, &am, Period::Morning, options, formats)?;
            write_cell(sheet, row + 1, col, &pm, Period::Afternoon, options, formats)?;
        }

        sheet.merge_range(row, last_col, row + 1, last_col, "", &formats.signature)?;
        row += 2;
    }

    // 列宽
    sheet.set_column_width(0, 9.0)?;
    sheet.set_column_width(1, 5.0)?;
    for col in 2..(2 + date_count) {
        sheet.set_column_width(col, 9.0)?;
    }
    sheet.set_column_width(last_col, 14.0)?;

    for r in 0..row {
        sheet.set_row_height(r, 38.0)?;
    }

    Ok(())
}

/// 计算上午/下午取值：上午取首次签到，下午取末次签退；
/// 末次签退早于下午时段起始时间时，视为上午打卡、下午缺卡。
fn resolve_times(times: &DayTimes, options: &ReportOptions) -> (String, String) {
    let am = times.am.clone().unwrap_or_else(|| MISSING.to_string());
    let pm = match &times.pm {
        Some(last) if is_afternoon(last, &options.afternoon_start) => last.clone(),
        _ => MISSING.to_string(),
    };
    (am, pm)
}

/// 缺卡占位文本。
const MISSING: &str = "缺卡";

/// 判断末次签退是否落在下午时段。
fn is_afternoon(time: &str, afternoon_start: &str) -> bool {
    let parsed = parse_clock(time);
    let start = parse_clock(afternoon_start)
        .unwrap_or_else(|| NaiveTime::from_hms_opt(12, 0, 0).expect("12:00 是合法时间"));

    match parsed {
        Some(time) => time >= start,
        // 时间格式异常时不做额外过滤
        None => true,
    }
}

/// 写入单元格：
/// - 时间落在正常区间（上午 ≤ morning_time、下午 ≥ afternoon_time）→ 黑色字体；
/// - 时间超出正常区间，或为缺卡/请假/出差等非时间文本 → 红色字体。
fn write_cell(
    sheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: &str,
    period: Period,
    options: &ReportOptions,
    formats: &Formats,
) -> Result<()> {
    let format = if options.is_normal_time(period, value) {
        &formats.data
    } else {
        &formats.red
    };
    sheet.write_string_with_format(row, col, value, format)?;
    Ok(())
}

/// 单元格格式集合。
struct Formats {
    header: Format,
    data: Format,
    red: Format,
    signature: Format,
}

impl Formats {
    fn new() -> Self {
        Self {
            header: Format::new()
                .set_bold()
                .set_align(FormatAlign::VerticalCenter)
                .set_align(FormatAlign::Center)
                .set_border(FormatBorder::Thin)
                .set_font_name("Arial")
                .set_font_size(12),
            data: Format::new()
                .set_bold()
                .set_align(FormatAlign::VerticalCenter)
                .set_align(FormatAlign::Center)
                .set_border(FormatBorder::Thin)
                .set_font_name("Arial")
                .set_font_size(11),
            red: Format::new()
                .set_bold()
                .set_align(FormatAlign::VerticalCenter)
                .set_align(FormatAlign::Center)
                .set_border(FormatBorder::Thin)
                .set_font_color(Color::Red)
                .set_font_name("Arial")
                .set_font_size(11),
            signature: Format::new()
                .set_bold()
                .set_align(FormatAlign::VerticalCenter)
                .set_align(FormatAlign::Center)
                .set_border(FormatBorder::Thin)
                .set_font_name("Arial")
                .set_font_size(11),
        }
    }
}

/// 供测试/调试：统计每天打卡人数。
#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn record(person: &str, day: NaiveDate, first: &str, last: &str) -> PunchRecord {
        PunchRecord {
            person: person.to_string(),
            date: day,
            first: first.to_string(),
            last: last.to_string(),
        }
    }

    #[test]
    fn morning_before_or_at_punch_point_is_normal() {
        let options = ReportOptions::default(); // morning_time = 08:00

        assert!(options.is_normal_time(Period::Morning, "07:59"));
        assert!(options.is_normal_time(Period::Morning, "08:00"));
        // 晚于打卡点 → 异常（红色）
        assert!(!options.is_normal_time(Period::Morning, "08:01"));
        assert!(!options.is_normal_time(Period::Morning, "09:30"));
    }

    #[test]
    fn afternoon_from_punch_point_is_normal() {
        let options = ReportOptions::default(); // afternoon_time = 18:00

        assert!(options.is_normal_time(Period::Afternoon, "18:00"));
        assert!(options.is_normal_time(Period::Afternoon, "18:01"));
        assert!(options.is_normal_time(Period::Afternoon, "23:59"));
        // 早于打卡点 → 异常（红色）
        assert!(!options.is_normal_time(Period::Afternoon, "17:59"));
        assert!(!options.is_normal_time(Period::Afternoon, "15:12"));
    }

    #[test]
    fn non_time_values_are_never_normal() {
        let options = ReportOptions::default();

        for value in ["缺卡", "请假(年休假)", "出差", "迟到", "早退", ""] {
            assert!(!options.is_normal_time(Period::Morning, value), "{value}");
            assert!(!options.is_normal_time(Period::Afternoon, value), "{value}");
        }
    }

    #[test]
    fn punch_points_are_configurable() {
        let options = ReportOptions {
            morning_time: "08:30".to_string(),
            afternoon_time: "17:30".to_string(),
            ..ReportOptions::default()
        };

        assert!(options.is_normal_time(Period::Morning, "08:30"));
        assert!(!options.is_normal_time(Period::Morning, "08:31"));
        assert!(options.is_normal_time(Period::Afternoon, "17:30"));
        assert!(!options.is_normal_time(Period::Afternoon, "17:29"));
    }

    #[test]
    fn date_label_uses_dot_format() {
        assert_eq!(date_label(&date(2026, 9, 1)), "9.1");
        assert_eq!(date_label(&date(2026, 9, 30)), "9.30");
    }

    #[test]
    fn valid_ratio_requires_more_than_half_by_default() {
        let options = ReportOptions::default();
        assert!(!options.is_valid(5, 10));
        assert!(options.is_valid(6, 10));
        assert!(!options.is_valid(54, 109));
        assert!(options.is_valid(55, 109));
    }

    #[test]
    fn valid_ratio_inclusive_allows_exactly_half() {
        let options = ReportOptions {
            valid_ratio_inclusive: true,
            ..ReportOptions::default()
        };
        assert!(options.is_valid(5, 10));
    }

    #[test]
    fn picks_dates_above_threshold() {
        let options = ReportOptions::default();
        let day1 = date(2026, 9, 1);
        let day2 = date(2026, 9, 5);
        let mut records = Vec::new();
        for i in 0..3 {
            records.push(record(&format!("甲{}", i), day1, "08:10:00", "17:40:00"));
        }
        // 周末只有 1 人打卡，不合法
        records.push(record("乙", day2, "09:00:00", ""));

        let (dates, counts) = pick_valid_dates(&records, &options, 4).unwrap();
        assert_eq!(dates, vec![day1]);
        assert_eq!(counts.get(&day1), Some(&3));
        assert_eq!(counts.get(&day2), Some(&1));
    }

    #[test]
    fn afternoon_missing_when_last_punch_is_morning() {
        let options = ReportOptions::default();
        let times = DayTimes {
            am: Some("08:07".to_string()),
            pm: Some("08:07".to_string()),
        };
        let (am, pm) = resolve_times(&times, &options);
        assert_eq!(am, "08:07");
        assert_eq!(pm, MISSING);
    }

    #[test]
    fn afternoon_kept_when_last_punch_after_start() {
        let options = ReportOptions::default();
        let times = DayTimes {
            am: Some("08:07".to_string()),
            pm: Some("17:43".to_string()),
        };
        let (am, pm) = resolve_times(&times, &options);
        assert_eq!(am, "08:07");
        assert_eq!(pm, "17:43");
    }

    #[test]
    fn parses_html_table_rows() {
        let html = r#"<table border="1" class="Punch_Report">
<tr><td>1</td><td>0001</td><td>张三</td><td>分中心</td><td>-</td><td>男</td><td>2026-09-01</td><td>星期二</td><td>(08:45:00-17:30:00)</td><td>08:10:37</td><td>17:40:12</td></tr>
<tr><td>2</td><td>0001</td><td>张三</td><td>分中心</td><td>-</td><td>男</td><td>2026-09-02</td><td>星期三</td><td>(08:45:00-17:30:00)</td><td>-</td><td>-</td></tr>
</table>"#;
        let rows = parse_html_rows(html);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][2], "张三");
        assert_eq!(rows[0][9], "08:10:37");
        assert_eq!(rows[1][10], "-");
    }

    #[test]
    fn output_path_keeps_source_stem() {
        let path = Path::new("/tmp/report.xls");
        assert_eq!(
            build_output_path(path),
            PathBuf::from("/tmp/report_report.xlsx")
        );
    }
}

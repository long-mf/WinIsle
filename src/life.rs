// 生活模块：生日倒计时 / 纪念日 / 出差补贴
// 独立子模块：数据 + 纯函数（日期/农历/补贴） + egui 渲染。
// 由 main.rs 通过 `mod life;` 引入，IslandApp 持有 LifeState，draw_expanded_panel 调度 life::draw。

use eframe::egui;
use eframe::egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};
use std::path::PathBuf;

// ===================== 农历核心（已用 2024/2025/2023 新年 + 2024 中秋验证） =====================
const LUNAR_INFO: [u32; 201] = [
19416,19168,42352,21717,53856,55632,91476,22176,39632,21970,19168,42422,42192,53840,119381,46400,54944,44450,38320,84343,18800,42160,46261,27216,27968,109396,11104,38256,21234,18800,25958,54432,59984,28309,23248,11104,100067,37600,116951,51536,54432,120998,46416,22176,107956,9680,37584,53938,43344,46423,27808,46416,86869,19872,42416,83315,21168,43432,59728,27296,44710,43856,19296,43748,42352,21088,62051,55632,23383,22176,38608,19925,19152,42192,54484,53840,54616,46400,46752,103846,38320,18864,43380,42160,45690,27216,27968,44870,43872,38256,19189,18800,25776,29859,59984,27480,23232,43872,38613,37600,51552,55636,54432,55888,30034,22176,43959,9680,37584,51893,43344,46240,47780,44368,21977,19360,42416,86390,21168,43312,31060,27296,44368,23378,19296,42726,42208,53856,60005,54576,23200,30371,38608,19195,19152,42192,118966,53840,54560,56645,46496,22224,21938,18864,42359,42160,43600,111189,27936,44448,84835,37744,18936,18800,25776,92326,59984,27424,108228,43744,37600,53987,51552,54615,54432,55888,23893,22176,42704,21972,21200,43448,43344,46240,46758,44368,21920,43940,42416,21168,45683,26928,29495,27296,44368,84821,19296,42352,21732,53600,59752,54560,55968,92838,22224,19168,43476,42192,53584,62034,54560,
];

fn leap_month(year: i32) -> i32 { (LUNAR_INFO[(year - 1900) as usize] & 0xF) as i32 }
fn leap_days(year: i32) -> i32 {
    if leap_month(year) != 0 { if LUNAR_INFO[(year - 1900) as usize] & 0x10000 != 0 { 30 } else { 29 } } else { 0 }
}
fn month_days_l(year: i32, month: u32) -> i32 {
    if month > 12 || month < 1 { return -1; }
    if LUNAR_INFO[(year - 1900) as usize] & (0x10000u32 >> month) != 0 { 30 } else { 29 }
}
fn l_year_days(year: i32) -> i32 {
    let mut e = 348i32;
    let info = LUNAR_INFO[(year - 1900) as usize];
    let mut f: u32 = 32768;
    while f > 8 { if info & f != 0 { e += 1; } f >>= 1; }
    e + leap_days(year)
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = (y - if m <= 2 { 1 } else { 0 }) as i64;
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = ((m as i64) + if m > 2 { -3 } else { 9 }) as i64;
    let doy = (153 * mp + 2) / 5 + (d as i64) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
fn days_to_civil(z0: i64) -> (i32, u32, u32) {
    let z = z0 + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (mp + if mp < 10 { 3 } else { -9 }) as u32;
    let y = y + if m <= 2 { 1 } else { 0 };
    (y as i32, m, d)
}

/// 农历 (年,月,日[,闰]) -> 公历 (年,月,日)。越界/该年无此闰月返回 None。
pub fn lunar_to_solar(ly: i32, lm: u32, ld: u32, is_leap: bool) -> Option<(i32, u32, u32)> {
    if ly < 1900 || ly > 2100 { return None; }
    let t = leap_month(ly);
    if is_leap && t != lm as i32 { return None; }
    if (ly == 2100 && lm == 12 && ld > 1) || (ly == 1900 && lm == 1 && ld < 31) { return None; }
    let r = month_days_l(ly, lm);
    if (ld as i32) > r { return None; }
    let mut a: i64 = 0;
    for n in 1900..ly { a += l_year_days(n) as i64; }
    let mut u = false;
    for n in 1..lm {
        let dm = leap_month(ly);
        if !u && dm <= n as i32 && dm > 0 { a += leap_days(ly) as i64; u = true; }
        a += month_days_l(ly, n) as i64;
    }
    if is_leap { a += r as i64; }
    let base = days_from_civil(1900, 2, 30);
    Some(days_to_civil(base + a + (ld as i64) - 31))
}

// ===================== 日期/统计工具 =====================
fn is_leap_year(y: i32) -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 }
fn mdays(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(y) { 29 } else { 28 },
        _ => 30,
    }
}
fn to_days(d: (i32, u32, u32)) -> i64 { days_from_civil(d.0, d.1, d.2) }
fn today() -> (i32, u32, u32) {
    let st = crate::platform::local_time();
    (st.wYear as i32, st.wMonth as u32, st.wDay as u32)
}
fn parse_date(s: &str) -> Option<(i32, u32, u32)> {
    let s = s.trim();
    if s.is_empty() { return None; }
    let parts: Vec<&str> = s.split(|c| c == '-' || c == '/' || c == '.').collect();
    if parts.len() < 3 { return None; }
    let y: i32 = parts[0].trim().parse().ok()?;
    let m: u32 = parts[1].trim().parse().ok()?;
    let d: u32 = parts[2].trim().parse().ok()?;
    if !(1900..=2200).contains(&y) || m < 1 || m > 12 || d < 1 || d > 31 { return None; }
    Some((y, m, d))
}
fn fmt_ymd(d: (i32, u32, u32)) -> String { format!("{:04}-{:02}-{:02}", d.0, d.1, d.2) }
fn money(v: f64) -> String {
    let n = v.round() as i64;
    let neg = n < 0;
    let s = n.abs().to_string();
    let mut out = String::new();
    let len = s.len();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 { out.push(','); }
        out.push(ch);
    }
    if neg { format!("-¥{}", out) } else { format!("¥{}", out) }
}

const ANIMALS: [&str; 12] = ["鼠", "牛", "虎", "兔", "龙", "蛇", "马", "羊", "猴", "鸡", "狗", "猪"];
fn zodiac(year: i32) -> &'static str { ANIMALS[((year - 4).rem_euclid(12)) as usize] }
fn constellation(m: u32, d: u32) -> &'static str {
    let md = m * 100 + d;
    if md >= 120 && md <= 218 { "水瓶座" }
    else if md >= 219 && md <= 320 { "双鱼座" }
    else if md >= 321 && md <= 419 { "白羊座" }
    else if md >= 420 && md <= 520 { "金牛座" }
    else if md >= 521 && md <= 621 { "双子座" }
    else if md >= 622 && md <= 722 { "巨蟹座" }
    else if md >= 723 && md <= 822 { "狮子座" }
    else if md >= 823 && md <= 922 { "处女座" }
    else if md >= 923 && md <= 1023 { "天秤座" }
    else if md >= 1024 && md <= 1122 { "天蝎座" }
    else if md >= 1123 && md <= 1221 { "射手座" }
    else { "摩羯座" }
}

// ===================== 数据模型 =====================
#[derive(Clone)]
pub struct Birthday {
    pub id: u64,
    pub name: String,
    pub lunar: bool,   // true=农历
    pub year: u32,     // 农历生日为农历年；公历为公历年
    pub month: u32,
    pub day: u32,
    pub leap: bool,    // 农历闰月
    pub rel: String,   // 关系
}
#[derive(Clone)]
pub struct Anniversary {
    pub id: u64,
    pub name: String,
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub yearly: bool, // 每年 / 一次性
    pub cat: String,
}
#[derive(Clone)]
pub struct Bracket {
    pub lo: String,   // 起始第 N 天
    pub hi: String,   // 结束第 N 天；空=以上（开区间）
    pub rate: String, // 每天金额
}

/// 生日/纪念日 新增·编辑表单的临时状态
#[derive(Clone)]
pub struct EditState {
    pub kind: u8,        // 0=生日 1=纪念日
    pub id: Option<u64>, // None=新增
    pub name: String,
    pub lunar: bool,
    pub leap: bool,
    pub year: String,
    pub month: String,
    pub day: String,
    pub rel: String,   // 生日关系 / 纪念日分类
    pub yearly: bool,  // 纪念日：每年/一次性
    pub pick_open: bool, // 日期选择面板是否展开
    pub pick_y: i32,     // 日历面板当前显示年
    pub pick_m: u32,     // 日历面板当前显示月
    pub pick_ymode: bool, // 年份快速选择网格
    pub pick_ypage: i32,  // 年份网格起始年
}
impl EditState {
    fn new_birthday() -> Self { Self { kind: 0, id: None, name: String::new(), lunar: false, leap: false, year: String::new(), month: String::new(), day: String::new(), rel: "家人".into(), yearly: true, pick_open: false, pick_y: 0, pick_m: 0, pick_ymode: false, pick_ypage: 0 } }
    fn from_birthday(b: &Birthday) -> Self { Self { kind: 0, id: Some(b.id), name: b.name.clone(), lunar: b.lunar, leap: b.leap, year: b.year.to_string(), month: b.month.to_string(), day: b.day.to_string(), rel: b.rel.clone(), yearly: true, pick_open: false, pick_y: 0, pick_m: 0, pick_ymode: false, pick_ypage: 0 } }
    fn new_anniv() -> Self { Self { kind: 1, id: None, name: String::new(), lunar: false, leap: false, year: String::new(), month: String::new(), day: String::new(), rel: "家庭".into(), yearly: true, pick_open: false, pick_y: 0, pick_m: 0, pick_ymode: false, pick_ypage: 0 } }
    fn from_anniv(a: &Anniversary) -> Self { Self { kind: 1, id: Some(a.id), name: a.name.clone(), lunar: false, leap: false, year: a.year.to_string(), month: a.month.to_string(), day: a.day.to_string(), rel: a.cat.clone(), yearly: a.yearly, pick_open: false, pick_y: 0, pick_m: 0, pick_ymode: false, pick_ypage: 0 } }
}

pub struct LifeState {
    pub tab: u8,
    pub birthdays: Vec<Birthday>,
    pub anniversaries: Vec<Anniversary>,
    pub brackets: Vec<Bracket>,
    pub sub_start: String,
    pub sub_end: String,
    pub next_id: u64,
    pub loaded: bool,
    pub edit: Option<EditState>,
    pub sub_pick: u8,      // 0=无；1=选开始；2=选结束
    pub sub_pick_y: i32,
    pub sub_pick_m: u32,
    pub sub_ymode: bool,   // 年份快速选择网格
    pub sub_ypage: i32,    // 年份网格起始年
    pub sub_br_first: usize, // 补贴区间列表滚动起始行
}

impl Default for LifeState {
    fn default() -> Self {
        LifeState {
            tab: 0,
            birthdays: vec![
                Birthday { id: 1, name: "妈妈".into(), lunar: true, year: 1968, month: 8, day: 15, leap: false, rel: "家人".into() },
                Birthday { id: 2, name: "小林".into(), lunar: false, year: 1996, month: 3, day: 14, leap: false, rel: "朋友".into() },
                Birthday { id: 3, name: "阿哲".into(), lunar: false, year: 1992, month: 12, day: 25, leap: false, rel: "同事".into() },
            ],
            anniversaries: vec![
                Anniversary { id: 1, name: "结婚纪念日".into(), year: 2019, month: 5, day: 20, yearly: true, cat: "家庭".into() },
                Anniversary { id: 2, name: "入职纪念日".into(), year: 2021, month: 7, day: 1, yearly: true, cat: "工作".into() },
            ],
            brackets: vec![],
            sub_start: String::new(),
            sub_end: String::new(),
            next_id: 100,
            loaded: false,
            edit: None,
            sub_pick: 0,
            sub_pick_y: 0,
            sub_pick_m: 0,
            sub_ymode: false,
            sub_ypage: 0,
            sub_br_first: 0,
        }
    }
}

fn birthdays_path() -> PathBuf { crate::paths::file("birthdays.json") }
fn anniversaries_path() -> PathBuf { crate::paths::file("anniversaries.json") }
fn subsidy_path() -> PathBuf { crate::paths::file("subsidy.json") }

impl LifeState {
    pub fn load() -> Self {
        let mut s = LifeState::default();
        // 生日
        if birthdays_path().exists() {
            let mut v: Vec<Birthday> = Vec::new();
            if let Ok(text) = std::fs::read_to_string(birthdays_path()) {
                if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&text) {
                    for it in arr {
                        v.push(Birthday {
                            id: it.get("id").and_then(|x| x.as_u64()).unwrap_or(0),
                            name: it.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            lunar: it.get("lunar").and_then(|x| x.as_bool()).unwrap_or(false),
                            year: it.get("year").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                            month: it.get("month").and_then(|x| x.as_u64()).unwrap_or(1) as u32,
                            day: it.get("day").and_then(|x| x.as_u64()).unwrap_or(1) as u32,
                            leap: it.get("leap").and_then(|x| x.as_bool()).unwrap_or(false),
                            rel: it.get("rel").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        });
                    }
                }
            }
            s.birthdays = v;
        }
        // 纪念日
        if anniversaries_path().exists() {
            let mut v: Vec<Anniversary> = Vec::new();
            if let Ok(text) = std::fs::read_to_string(anniversaries_path()) {
                if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&text) {
                    for it in arr {
                        v.push(Anniversary {
                            id: it.get("id").and_then(|x| x.as_u64()).unwrap_or(0),
                            name: it.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            year: it.get("year").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                            month: it.get("month").and_then(|x| x.as_u64()).unwrap_or(1) as u32,
                            day: it.get("day").and_then(|x| x.as_u64()).unwrap_or(1) as u32,
                            yearly: it.get("yearly").and_then(|x| x.as_bool()).unwrap_or(true),
                            cat: it.get("cat").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        });
                    }
                }
            }
            s.anniversaries = v;
        }
        // 补贴
        if subsidy_path().exists() {
            if let Ok(text) = std::fs::read_to_string(subsidy_path()) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(st) = v.get("start").and_then(|x| x.as_str()) { s.sub_start = st.to_string(); }
                    if let Some(en) = v.get("end").and_then(|x| x.as_str()) { s.sub_end = en.to_string(); }
                    if let Some(serde_json::Value::Array(arr)) = v.get("brackets") {
                        let mut brs: Vec<Bracket> = Vec::new();
                        for it in arr {
                            brs.push(Bracket {
                                lo: it.get("lo").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                                hi: it.get("hi").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                                rate: it.get("rate").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            });
                        }
                        s.brackets = brs;
                        normalize_brackets(&mut s.brackets);
                    }
                }
            }
        }
        let max_b = s.birthdays.iter().map(|b| b.id).max().unwrap_or(0);
        let max_a = s.anniversaries.iter().map(|a| a.id).max().unwrap_or(0);
        s.next_id = max_b.max(max_a).max(100) + 1;
        if let Ok(v) = std::env::var("WINISLE_LIFE_TAB") { if let Ok(n) = v.parse::<u8>() { s.tab = n.min(2); } }
        if let Ok(v) = std::env::var("WINISLE_LIFE_EDIT") {
            match v.as_str() {
                "1" => s.edit = Some(EditState::new_birthday()),
                "editb" => { if let Some(b) = s.birthdays.first() { s.edit = Some(EditState::from_birthday(b)); } }
                "newa" => s.edit = Some(EditState::new_anniv()),
                "edita" => { if let Some(a) = s.anniversaries.first() { s.edit = Some(EditState::from_anniv(a)); } }
                _ => {}
            }
        }
        // 调试：自动展开日期选择面板（WINISLE_LIFE_PICK=1）
        if std::env::var("WINISLE_LIFE_PICK").is_ok() {
            if let Some(e) = s.edit.as_mut() {
                e.pick_open = true;
                let t = today();
                e.pick_y = e.year.trim().parse().unwrap_or(t.0);
                e.pick_m = e.month.trim().parse().unwrap_or(t.1);
            }
        }
        // 调试：自动展开出差日历选择（WINISLE_SUB_PICK=1/2）
        if let Ok(v) = std::env::var("WINISLE_SUB_PICK") {
            if let Ok(n) = v.parse::<u8>() {
                s.sub_pick = n.min(2);
                let t = today();
                let d = parse_date(if n == 1 { &s.sub_start } else { &s.sub_end }).unwrap_or(t);
                s.sub_pick_y = d.0; s.sub_pick_m = d.1;
            }
        }
        s.loaded = true;
        s
    }

    pub fn save_birthdays(&self) {
        let arr: Vec<serde_json::Value> = self.birthdays.iter().map(|b| serde_json::json!({
            "id": b.id, "name": b.name, "lunar": b.lunar, "year": b.year,
            "month": b.month, "day": b.day, "leap": b.leap, "rel": b.rel,
        })).collect();
        let _ = std::fs::write(birthdays_path(), serde_json::Value::Array(arr).to_string());
    }
    pub fn save_anniversaries(&self) {
        let arr: Vec<serde_json::Value> = self.anniversaries.iter().map(|a| serde_json::json!({
            "id": a.id, "name": a.name, "year": a.year, "month": a.month,
            "day": a.day, "yearly": a.yearly, "cat": a.cat,
        })).collect();
        let _ = std::fs::write(anniversaries_path(), serde_json::Value::Array(arr).to_string());
    }
    pub fn save_subsidy(&self) {
        let brs: Vec<serde_json::Value> = self.brackets.iter().map(|b| serde_json::json!({
            "lo": b.lo, "hi": b.hi, "rate": b.rate,
        })).collect();
        let v = serde_json::json!({ "start": self.sub_start, "end": self.sub_end, "brackets": brs });
        let _ = std::fs::write(subsidy_path(), v.to_string());
    }
}

// ===================== 生日/纪念日/补贴 计算 =====================
fn next_birthday(b: &Birthday, t: (i32, u32, u32)) -> (i32, u32, u32, i32) {
    let td = to_days(t);
    if !b.lunar {
        for k in 0..4 {
            let y = t.0 + k;
            let dd = b.day.min(mdays(y, b.month));
            let cand = (y, b.month, dd);
            if to_days(cand) >= td { return (cand.0, cand.1, cand.2, y); }
        }
        (t.0 + 1, b.month, b.day, t.0 + 1)
    } else {
        for k in 0..5 {
            let ly = t.0 - 1 + k;
            if let Some((y, m, d)) = lunar_to_solar(ly, b.month, b.day, b.leap) {
                if to_days((y, m, d)) >= td { return (y, m, d, ly); }
            }
        }
        (t.0 + 1, b.month, b.day, t.0 + 1)
    }
}

fn br_lo(b: &Bracket) -> i64 { b.lo.trim().parse().unwrap_or(0) }
fn br_hi(b: &Bracket) -> i64 { let t = b.hi.trim(); if t.is_empty() { 0 } else { t.parse().unwrap_or(0) } }
fn br_rate(b: &Bracket) -> f64 { b.rate.trim().trim_start_matches('¥').trim().parse().unwrap_or(0.0) }

/// 校验阶梯区间是否合理；返回 Some(提示) 表示存在问题。
fn validate_brackets(bs: &[Bracket]) -> Option<String> {
    let mut prev_end: Option<u32> = None;
    for (i, b) in bs.iter().enumerate() {
        let lo = match b.lo.trim().parse::<u32>() {
            Ok(v) => v,
            Err(_) => return Some(format!("第{}段起始天数未填/无效", i + 1)),
        };
        if lo == 0 { return Some(format!("第{}段起始天数需 ≥ 1", i + 1)); }
        if b.rate.trim().is_empty() { return Some(format!("第{}段每天金额未填", i + 1)); }
        if br_rate(b) <= 0.0 { return Some(format!("第{}段每天金额无效", i + 1)); }
        let hi = if b.hi.trim().is_empty() { None } else {
            match b.hi.trim().parse::<u32>() {
                Ok(v) => Some(v),
                Err(_) => return Some(format!("第{}段结束天数无效", i + 1)),
            }
        };
        if let Some(h) = hi { if h < lo { return Some(format!("第{}段结束应 ≥ 起始", i + 1)); } }
        if let Some(pe) = prev_end {
            if lo != pe + 1 {
                return Some(if lo <= pe {
                    format!("第{}段与上一段重叠，应从第 {} 天开始", i + 1, pe + 1)
                } else {
                    format!("第{}段有空档，应从第 {} 天开始", i + 1, pe + 1)
                });
            }
        }
        match hi {
            Some(h) => prev_end = Some(h),
            None => {
                if i + 1 < bs.len() { return Some("只有最后一段可以是「X 天以上」".to_string()); }
                prev_end = None;
            }
        }
    }
    None
}

fn normalize_brackets(bs: &mut Vec<Bracket>) {
    if bs.is_empty() { return; }
    let mut start: i64 = 1;
    let mut cut: Option<usize> = None;
    for i in 0..bs.len() {
        bs[i].lo = start.to_string();
        let hi = br_hi(&bs[i]);
        if hi <= 0 { cut = Some(i); break; }
        start = hi + 1;
    }
    if let Some(c) = cut { bs.truncate(c + 1); }
}

fn rate_for(day: i64, brs: &[Bracket]) -> f64 {
    for b in brs {
        let lo = br_lo(b);
        let hi = br_hi(b);
        if day >= lo && (hi == 0 || day <= hi) { return br_rate(b); }
    }
    0.0
}
/// 返回 (总天数, 总补贴, 已过天数, 至今补贴)
fn subsidy_calc(start: (i32, u32, u32), end: (i32, u32, u32), brs: &[Bracket]) -> (i64, f64, i64, f64) {
    let sd = to_days(start);
    let ed = to_days(end);
    let days = (ed - sd + 1).max(0);
    let mut total = 0.0;
    for d in 1..=days { total += rate_for(d, brs); }
    let t = today();
    let tday = to_days(t);
    let elapsed = if tday < sd { 0 } else { (tday - sd + 1).min(days).max(0) };
    let mut todate = 0.0;
    for d in 1..=elapsed { todate += rate_for(d, brs); }
    (days, total, elapsed, todate)
}

// ===================== 绘制辅助 =====================
fn c(r: u8, g: u8, b: u8, a: u8) -> Color32 { Color32::from_rgba_unmultiplied(r, g, b, a) }

fn chip(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, id: egui::Id, label: &str,
        fs: f32, fg: Color32, bg: Color32, hover: Color32) -> bool {
    let hov = ui.rect_contains_pointer(rect);
    painter.rect_filled(rect, 8.0, if hov { hover } else { bg });
    painter.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(fs), fg);
    if hov { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    ui.interact(rect, id, egui::Sense::click()).clicked()
}

fn text_field(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, id: egui::Id,
              buf: &mut String, fs: f32, ink: Color32, align: u8) -> bool {
    let hovered = ui.rect_contains_pointer(rect);
    // 按下即聚焦（非激活窗口也能拿到 egui 焦点）
    if hovered && ui.input(|i| i.pointer.primary_pressed() || i.pointer.primary_down()) {
        ui.memory_mut(|m| m.request_focus(id));
        if ui.input(|i| i.pointer.primary_pressed()) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }
    let focused = ui.memory(|m| m.has_focus(id));
    let hot = focused || hovered;
    painter.rect_stroke(rect, 8.0,
        Stroke::new(1.0, c(52, 211, 153, if hot { 140 } else { 40 })),
        StrokeKind::Inside);
    if hovered { ui.ctx().set_cursor_icon(egui::CursorIcon::Text); }
    // 悬浮/常驻置顶窗口常被视为“未聚焦”，egui 因而不画插入符；临时把 input.focused 置真以强制绘制 caret
    let vp_before = ui.ctx().input(|i| i.focused);
    let force_caret = !vp_before && focused;
    if force_caret { ui.ctx().input_mut(|i| i.focused = true); }
    let h_align = match align { 0 => egui::Align::Min, 1 => egui::Align::Max, _ => egui::Align::Center };
    let fid = if align == 3 { FontId::monospace(fs) } else { FontId::proportional(fs) };
    let changed = ui.scope(|ui| {
        let v = ui.visuals_mut();
        v.selection.bg_fill = Color32::from_rgba_unmultiplied(52, 211, 153, 56);
        v.selection.stroke = Stroke::new(1.0, ink);
        v.extreme_bg_color = Color32::TRANSPARENT;
        v.widgets.inactive.bg_fill = Color32::TRANSPARENT;
        v.widgets.hovered.bg_fill = Color32::TRANSPARENT;
        v.widgets.active.bg_fill = Color32::TRANSPARENT;
        v.text_cursor = egui::style::TextCursorStyle {
            stroke: Stroke::new(1.6, Color32::from_rgb(233, 236, 242)),
            ..Default::default()
        };
        let inner = rect.shrink2(Vec2::new(8.0, 0.0));
        // 居中字段不能加水平 margin（否则控件被右侧撑宽、文字整体右偏约 7px）；左对齐字段保留左内边距
        let mx = if align >= 2 { 0 } else { 9 };
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.add_sized([inner.width(), rect.height()],
            egui::TextEdit::singleline(buf).frame(false).id(id)
                .margin(egui::Margin::symmetric(mx, 3))
                .vertical_align(egui::Align::Center)
                .horizontal_align(h_align)
                .font(fid).text_color(ink)).changed()
    }).inner;
    if force_caret { ui.ctx().input_mut(|i| i.focused = vp_before); }
    if focused { ui.ctx().request_repaint(); }
    changed
}

// ===================== 主绘制 =====================
pub fn draw(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, e: f32, st: &mut LifeState) {
    let af = ((e - 0.5) * 2.0).clamp(0.0, 1.0);
    let a = (af * 255.0) as u8;
    let ink = c(233, 236, 242, a);
    let muted = c(150, 158, 175, a);
    let muted2 = c(107, 113, 128, a);
    let accent = c(52, 211, 153, a);
    let _accent_strong = c(16, 185, 129, a);
    let danger = c(244, 114, 114, a);

    let x0 = rect.left() + crate::CONTENT_PAD_X;
    let x1 = rect.right() - crate::CONTENT_PAD_X;
    let y0 = rect.top() + crate::CONTENT_PAD_T;

    let titles = ["生日", "纪念日", "出差补贴"];
    let ti = (st.tab.min(2)) as usize;
    painter.text(Pos2::new(x0, y0), Align2::LEFT_TOP, titles[ti], crate::bold_font(16.5), ink);

    let chip_txt = match ti {
        0 => format!("{} 位", st.birthdays.len()),
        1 => format!("{} 个", st.anniversaries.len()),
        _ => "按天累计".to_string(),
    };
    let cw = if ti == 2 { 60.0 } else { 46.0 };
    let chip_rect = Rect::from_min_size(Pos2::new(x1 - cw, y0 - 2.0), Vec2::new(cw, 20.0));
    painter.text(chip_rect.right_center(), Align2::RIGHT_CENTER, chip_txt, FontId::proportional(10.5), muted2);

    // 子页 tab
    let tabs = ["生日", "纪念日", "补贴"];
    let tab_y = y0 + 30.0;
    let th = 24.0;
    let mut tx = x0;
    for (i, t) in tabs.iter().enumerate() {
        let tw = 62.0;
        let r = Rect::from_min_size(Pos2::new(tx, tab_y), Vec2::new(tw, th));
        let on = st.tab == i as u8;
        let hov = ui.rect_contains_pointer(r);
        painter.text(r.center(), Align2::CENTER_CENTER, *t, FontId::proportional(11.5),
            if on { accent } else if hov { ink } else { muted });
        if on {
            let uw = 16.0;
            let uy = r.bottom() - 2.0;
            painter.rect_filled(Rect::from_min_size(Pos2::new(r.center().x - uw * 0.5, uy), Vec2::new(uw, 2.0)), 1.0, accent);
        }
        if hov { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        if ui.interact(r, egui::Id::new(("life_tab", i)), egui::Sense::click()).clicked() {
            st.tab = i as u8;
        }
        tx += tw + 8.0;
    }

    let body_top = tab_y + th + 12.0;
    let body_bottom = rect.bottom() - crate::CONTENT_PAD_B;

    match st.tab {
        0 => draw_birthdays(ui, painter, st, x0, x1, body_top, body_bottom, a, ink, muted, muted2, accent),
        1 => draw_anniversaries(ui, painter, st, x0, x1, body_top, body_bottom, a, ink, muted, muted2, accent),
        _ => draw_subsidy(ui, painter, st, x0, x1, body_top, body_bottom, a, ink, muted, muted2, accent, danger),
    }

    if st.edit.is_some() {
        draw_editor(ui, painter, rect, a, ink, muted, muted2, accent, danger, st);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_birthdays(ui: &mut egui::Ui, painter: &egui::Painter, st: &mut LifeState,
                  x0: f32, x1: f32, top: f32, bottom: f32, a: u8,
                  ink: Color32, muted: Color32, muted2: Color32, accent: Color32) {
    let t = today();
    let td = to_days(t);
    let mut items: Vec<(usize, i64, i32, String, String)> = Vec::new();
    for (idx, b) in st.birthdays.iter().enumerate() {
        let (ny, nm, nd, occ) = next_birthday(b, t);
        let dl = to_days((ny, nm, nd)) - td;
        let age = occ - b.year as i32;
        let sub = if b.lunar {
            format!("农历 {}月{}", b.month, b.day)
        } else {
            format!("公历 {:02}-{:02} · {}", b.month, b.day, constellation(b.month, b.day))
        };
        items.push((idx, dl, age, format!("{:02}-{:02}", nm, nd), sub));
    }
    items.sort_by_key(|x| x.1);

    let row_h = 44.0;
    let mut y = top;
    let max_rows = (((bottom - top) / row_h) as usize).saturating_sub(1);
    let mut clicked: Option<usize> = None;
    let mut del_id: Option<u64> = None;
    let mut add_clicked = false;
    for (n, it) in items.iter().enumerate() {
        if n >= max_rows { break; }
        let b = &st.birthdays[it.0];
        let cy = y + row_h * 0.5;
        let is_today = it.1 == 0;
        let row_rect = Rect::from_min_max(Pos2::new(x0 - 8.0, y), Pos2::new(x1 + 8.0, y + row_h));
        let hit_rect = Rect::from_min_max(Pos2::new(x0 - 8.0, y), Pos2::new(x1 - 22.0, y + row_h));
        let row_hov = ui.rect_contains_pointer(row_rect);
        if is_today {
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0 - 8.0, y + 2.0), Pos2::new(x1 + 8.0, y + row_h - 2.0)),
                8.0, c(52, 211, 153, (a as f32 * 0.08) as u8));
        } else if row_hov {
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0 - 8.0, y + 2.0), Pos2::new(x1 + 8.0, y + row_h - 2.0)),
                8.0, c(52, 211, 153, (a as f32 * 0.05) as u8));
        }
        // 头像
        painter.circle_filled(Pos2::new(x0 + 15.0, cy), 15.0, c(52, 211, 153, 22));
        let initial: String = b.name.chars().next().map(|ch| ch.to_string()).unwrap_or_default();
        painter.text(Pos2::new(x0 + 15.0, cy), Align2::CENTER_CENTER, initial, crate::bold_font(13.0), accent);
        // 名称 + 副行
        painter.text(Pos2::new(x0 + 40.0, cy - 9.0), Align2::LEFT_CENTER, &b.name, crate::bold_font(12.5), ink);
        painter.text(Pos2::new(x0 + 40.0, cy + 9.0), Align2::LEFT_CENTER, &it.4, FontId::proportional(10.0), muted);
        // 右侧倒计时 + 满岁（给右侧删除键留出 26px）
        let countdown = if it.1 == 0 { "今天".to_string() } else if it.1 == 1 { "明天".to_string() } else { format!("{} 天后", it.1) };
        painter.text(Pos2::new(x1 - 24.0, cy), Align2::RIGHT_CENTER, countdown, crate::bold_font(13.0), if is_today { accent } else { ink });
        // 悬停点击整行进入编辑（先注册，删除键后注册以覆盖其小区域）
        if ui.interact(hit_rect, egui::Id::new(("life_b_row", b.id)), egui::Sense::click()).clicked() { clicked = Some(it.0); }
        // 右侧删除键
        let drect = Rect::from_center_size(Pos2::new(x1 - 9.0, cy), Vec2::new(16.0, 16.0));
        let dh = ui.rect_contains_pointer(drect);
        if dh { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        painter.text(drect.center(), Align2::CENTER_CENTER, "✕", FontId::proportional(10.5),
            if dh { c(244, 114, 114, a) } else { c(150, 158, 175, (a as f32 * 0.7) as u8) });
        if ui.interact(drect, egui::Id::new(("life_b_del", b.id)), egui::Sense::click()).clicked() { del_id = Some(b.id); }
        if n + 1 < items.len() && n + 1 < max_rows {
            painter.line_segment([Pos2::new(x0, y + row_h), Pos2::new(x1, y + row_h)], Stroke::new(1.0, c(52, 211, 153, (a as f32 * 0.06) as u8)));
        }
        y += row_h;
    }
    // 添加行
    let add_y = (y + 6.0).min(bottom - 22.0);
    let add_rect = Rect::from_min_max(Pos2::new(x0, add_y), Pos2::new(x1, add_y + 22.0));
    let hov = ui.rect_contains_pointer(add_rect);
    painter.text(Pos2::new(x0 + 2.0, add_y + 11.0), Align2::LEFT_CENTER, "＋ 添加生日…",
        FontId::proportional(11.5), if hov { accent } else { muted2 });
    if hov { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    if ui.interact(add_rect, egui::Id::new("life_b_add"), egui::Sense::click()).clicked() { add_clicked = true; }
    if let Some(id) = del_id {
        st.birthdays.retain(|b| b.id != id);
        st.save_birthdays();
    } else if let Some(i) = clicked {
        if let Some(b) = st.birthdays.get(i).cloned() { st.edit = Some(EditState::from_birthday(&b)); }
    } else if add_clicked {
        st.edit = Some(EditState::new_birthday());
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_anniversaries(ui: &mut egui::Ui, painter: &egui::Painter, st: &mut LifeState,
                      x0: f32, x1: f32, top: f32, bottom: f32, a: u8,
                      ink: Color32, muted: Color32, muted2: Color32, accent: Color32) {
    let t = today();
    let td = to_days(t);
    let row_h = 44.0;
    let mut y = top;
    let max_rows = (((bottom - top) / row_h) as usize).saturating_sub(1);
    let n_items = st.anniversaries.len();
    let mut clicked: Option<usize> = None;
    let mut del_id: Option<u64> = None;
    let mut add_clicked = false;
    for (n, ann) in st.anniversaries.iter().enumerate() {
        if n >= max_rows { break; }
        let cy = y + row_h * 0.5;
        let (days, nth, label) = if ann.yearly {
            let mut found = None;
            for k in 0..3 {
                let yy = t.0 + k;
                let dd = ann.day.min(mdays(yy, ann.month));
                let cand = to_days((yy, ann.month, dd));
                if cand >= td { found = Some((cand - td, yy as i32 - ann.year as i32)); break; }
            }
            let (dl, na) = found.unwrap_or((0, 0));
            (dl, na, format!("每年 · 第 {} 周年", na.max(1)))
        } else {
            let od = to_days((ann.year as i32, ann.month, ann.day));
            if od >= td { (od - td, 0, "一次性".to_string()) } else { (td - od, 0, "一次性".to_string()) }
        };
        let is_today = days == 0;
        let row_rect = Rect::from_min_max(Pos2::new(x0 - 8.0, y), Pos2::new(x1 + 8.0, y + row_h));
        let hit_rect = Rect::from_min_max(Pos2::new(x0 - 8.0, y), Pos2::new(x1 - 22.0, y + row_h));
        let row_hov = ui.rect_contains_pointer(row_rect);
        if is_today {
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0 - 8.0, y + 2.0), Pos2::new(x1 + 8.0, y + row_h - 2.0)),
                8.0, c(52, 211, 153, (a as f32 * 0.08) as u8));
        } else if row_hov {
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0 - 8.0, y + 2.0), Pos2::new(x1 + 8.0, y + row_h - 2.0)),
                8.0, c(52, 211, 153, (a as f32 * 0.05) as u8));
        }
        painter.circle_filled(Pos2::new(x0 + 15.0, cy), 15.0, c(230, 178, 60, 22));
        painter.text(Pos2::new(x0 + 15.0, cy), Align2::CENTER_CENTER, "★", crate::bold_font(12.0), c(230, 178, 60, a));
        painter.text(Pos2::new(x0 + 40.0, cy - 9.0), Align2::LEFT_CENTER, &ann.name, crate::bold_font(12.5), ink);
        let sub = label.clone();
        painter.text(Pos2::new(x0 + 40.0, cy + 9.0), Align2::LEFT_CENTER, &sub, FontId::proportional(10.0), muted);
        let right = if ann.yearly {
            if days == 0 { "今天".to_string() } else if days == 1 { "明天".to_string() } else { format!("{} 天", days) }
        } else if to_days((ann.year as i32, ann.month, ann.day)) >= td {
            if days == 1 { "明天".to_string() } else { format!("{} 天", days) }
        } else {
            format!("已过 {} 天", days)
        };
        painter.text(Pos2::new(x1 - 24.0, cy), Align2::RIGHT_CENTER, right, crate::bold_font(12.5), if is_today { accent } else { ink });
        if ui.interact(hit_rect, egui::Id::new(("life_a_row", ann.id)), egui::Sense::click()).clicked() { clicked = Some(n); }
        let drect = Rect::from_center_size(Pos2::new(x1 - 9.0, cy), Vec2::new(16.0, 16.0));
        let dh = ui.rect_contains_pointer(drect);
        if dh { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        painter.text(drect.center(), Align2::CENTER_CENTER, "✕", FontId::proportional(10.5),
            if dh { c(244, 114, 114, a) } else { c(150, 158, 175, (a as f32 * 0.7) as u8) });
        if ui.interact(drect, egui::Id::new(("life_a_del", ann.id)), egui::Sense::click()).clicked() { del_id = Some(ann.id); }
        if n + 1 < n_items && n + 1 < max_rows {
            painter.line_segment([Pos2::new(x0, y + row_h), Pos2::new(x1, y + row_h)], Stroke::new(1.0, c(52, 211, 153, (a as f32 * 0.06) as u8)));
        }
        y += row_h;
    }
    let add_y = (y + 6.0).min(bottom - 22.0);
    let add_rect = Rect::from_min_max(Pos2::new(x0, add_y), Pos2::new(x1, add_y + 22.0));
    let hov = ui.rect_contains_pointer(add_rect);
    painter.text(Pos2::new(x0 + 2.0, add_y + 11.0), Align2::LEFT_CENTER, "＋ 添加纪念日…",
        FontId::proportional(11.5), if hov { accent } else { muted2 });
    if hov { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    if ui.interact(add_rect, egui::Id::new("life_a_add"), egui::Sense::click()).clicked() { add_clicked = true; }
    if let Some(id) = del_id {
        st.anniversaries.retain(|x| x.id != id);
        st.save_anniversaries();
    } else if let Some(i) = clicked {
        if let Some(a) = st.anniversaries.get(i).cloned() { st.edit = Some(EditState::from_anniv(&a)); }
    } else if add_clicked {
        st.edit = Some(EditState::new_anniv());
    }
}

/// 分段选择控件：返回当前选中下标
fn seg(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, id: egui::Id,
       opts: &[&str], sel: usize, a: u8, muted: Color32, ink: Color32, accent: Color32) -> usize {
    // 扁平风：仅细描边分组，选中=薄荷文字+下划线，无任何填充
    painter.rect_stroke(rect, 8.0, Stroke::new(1.0, c(52, 211, 153, (a as f32 * 0.16) as u8)), StrokeKind::Inside);
    let n = opts.len().max(1);
    let ow = rect.width() / n as f32;
    let mut chosen = sel.min(n - 1);
    for (i, opt) in opts.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * ow, rect.top()), Vec2::new(ow, rect.height()));
        let on = i == chosen;
        let hov = ui.rect_contains_pointer(r);
        painter.text(r.center(), Align2::CENTER_CENTER, *opt, FontId::proportional(11.5),
            if on { accent } else if hov { ink } else { muted });
        if on {
            let uw = (ow * 0.42).min(20.0);
            let uy = r.bottom() - 5.0;
            painter.rect_filled(Rect::from_min_size(Pos2::new(r.center().x - uw * 0.5, uy), Vec2::new(uw, 2.0)), 1.0, accent);
        }
        if hov { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        if ui.interact(r, egui::Id::new((id, i)), egui::Sense::click()).clicked() { chosen = i; }
    }
    chosen
}

/// 生日 / 纪念日 新增·编辑表单（覆盖在内容区之上的圆角卡片）
#[allow(clippy::too_many_arguments)]
fn draw_editor(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, a: u8,
               ink: Color32, muted: Color32, muted2: Color32, accent: Color32, danger: Color32,
               st: &mut LifeState) {
    let Some(mut ed) = st.edit.clone() else { return; };
    let ov = Rect::from_min_max(Pos2::new(rect.left() + 6.0, rect.top() + 6.0),
                                Pos2::new(rect.right() - 6.0, rect.bottom() - 6.0));
    painter.rect_filled(ov, 14.0, c(24, 26, 33, 255));
    painter.rect_stroke(ov, 14.0, Stroke::new(1.0, c(52, 211, 153, (a as f32 * 0.10) as u8)), StrokeKind::Inside);

    let lx = ov.left() + 16.0;
    let rx = ov.right() - 16.0;
    let label_w = 50.0;
    let is_b = ed.kind == 0;
    // 调试：自动聚焦姓名框（WINISLE_LIFE_FOCUS=1），用于验证插入符
    if std::env::var("WINISLE_LIFE_FOCUS").is_ok() {
        ui.memory_mut(|m| m.request_focus(egui::Id::new(("life_ed_name", is_b))));
    }
    let title = match (is_b, ed.id.is_some()) {
        (true, false) => "新增生日", (true, true) => "编辑生日",
        (false, false) => "新增纪念日", (false, true) => "编辑纪念日",
    };
    painter.text(Pos2::new(lx, ov.top() + 15.0), Align2::LEFT_TOP, title, crate::bold_font(15.0), ink);

    let by = ov.bottom() - 44.0;
    let bh = 30.0;

    let yr = ed.year.trim().parse::<i32>().ok();
    let mo = ed.month.trim().parse::<u32>().ok();
    let dy = ed.day.trim().parse::<u32>().ok();
    let date_ok = matches!((yr, mo, dy), (Some(y), Some(m), Some(d)) if (1900..=2200).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d));

    if ed.pick_open {
        let area = Rect::from_min_max(Pos2::new(lx, ov.top() + 44.0), Pos2::new(rx, by - 8.0));
        draw_date_picker(ui, painter, area, &mut ed, a, ink, muted, muted2, accent);
    } else {
        let mut y = ov.top() + 46.0;
        let rh = 30.0;
        let gap = 12.0;
        // 名称
        painter.text(Pos2::new(lx, y + rh * 0.5), Align2::LEFT_CENTER, if is_b { "姓名" } else { "名称" }, FontId::proportional(11.0), muted);
        let nrect = Rect::from_min_size(Pos2::new(lx + label_w, y), Vec2::new(rx - (lx + label_w), rh));
        text_field(ui, painter, nrect, egui::Id::new(("life_ed_name", is_b)), &mut ed.name, 12.0, ink, 0);
        y += rh + gap;
        // 历法 / 类型
        painter.text(Pos2::new(lx, y + rh * 0.5), Align2::LEFT_CENTER, if is_b { "历法" } else { "类型" }, FontId::proportional(11.0), muted);
        let srect = Rect::from_min_size(Pos2::new(lx + label_w, y), Vec2::new(180.0, rh));
        if is_b {
            let sel = seg(ui, painter, srect, egui::Id::new("life_ed_cal"), &["公历", "农历"], if ed.lunar { 1 } else { 0 }, a, muted, ink, accent);
            ed.lunar = sel == 1;
        } else {
            let sel = seg(ui, painter, srect, egui::Id::new("life_ed_kind"), &["每年", "一次性"], if ed.yearly { 0 } else { 1 }, a, muted, ink, accent);
            ed.yearly = sel == 0;
        }
        if is_b && ed.lunar {
            let lr = Rect::from_min_size(Pos2::new(srect.right() + 10.0, y), Vec2::new(60.0, rh));
            let on = ed.leap;
            let hov = ui.rect_contains_pointer(lr);
            painter.rect_stroke(lr, 8.0, Stroke::new(1.0, c(52, 211, 153, (a as f32 * if on { 0.5 } else { 0.16 }) as u8)), StrokeKind::Inside);
            painter.text(lr.center(), Align2::CENTER_CENTER, "闰月", FontId::proportional(11.0), if on { accent } else if hov { ink } else { muted });
            if hov { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
            if ui.interact(lr, egui::Id::new("life_ed_leap"), egui::Sense::click()).clicked() { ed.leap = !on; }
        }
        y += rh + gap;
        // 日期：点击整行弹出日历 / 农历选择
        painter.text(Pos2::new(lx, y + rh * 0.5), Align2::LEFT_CENTER, "日期", FontId::proportional(11.0), muted);
        let drow = Rect::from_min_size(Pos2::new(lx + label_w, y), Vec2::new(rx - (lx + label_w), rh));
        let pick_w = 50.0;
        let disp = Rect::from_min_size(drow.min, Vec2::new(drow.width() - pick_w - 8.0, rh));
        let brow = Rect::from_min_size(Pos2::new(disp.right() + 8.0, y), Vec2::new(pick_w, rh));
        let disp_txt = match (yr, mo, dy) {
            (Some(yy), Some(mm), Some(dd)) if date_ok => {
                if ed.lunar { format!("{:04} · 农历{}{}月{}", yy, if ed.leap { "闰" } else { "" }, mm, dd) }
                else { fmt_ymd((yy, mm, dd)) }
            }
            _ => "点击选择日期".to_string(),
        };
        let dh = ui.rect_contains_pointer(disp);
        painter.rect_stroke(disp, 8.0, Stroke::new(1.0, c(52, 211, 153, (a as f32 * if dh { 0.42 } else { 0.16 }) as u8)), StrokeKind::Inside);
        painter.text(Pos2::new(disp.left() + 9.0, disp.center().y), Align2::LEFT_CENTER, disp_txt, FontId::proportional(12.0), if date_ok { ink } else { muted2 });
        let bhv = ui.rect_contains_pointer(brow);
        painter.text(brow.center(), Align2::CENTER_CENTER, "选择", FontId::proportional(11.5), if bhv { accent } else { muted });
        if dh || bhv { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        let dclick = ui.interact(disp, egui::Id::new(("life_ed_date", is_b)), egui::Sense::click()).clicked();
        let bclick = ui.interact(brow, egui::Id::new(("life_ed_pick", is_b)), egui::Sense::click()).clicked();
        if dclick || bclick {
            ed.pick_open = true;
            let t = today();
            if ed.pick_y == 0 { ed.pick_y = yr.unwrap_or(t.0); }
            if ed.pick_m == 0 { ed.pick_m = mo.unwrap_or(t.1); }
        }
    }
    if ed.id.is_some() {
        let dr = Rect::from_min_size(Pos2::new(rx - 206.0, by), Vec2::new(56.0, bh));
        let dh = ui.rect_contains_pointer(dr);
        painter.rect_stroke(dr, 8.0, Stroke::new(1.0, c(244, 114, 114, (a as f32 * if dh { 0.55 } else { 0.30 }) as u8)), StrokeKind::Inside);
        painter.text(dr.center(), Align2::CENTER_CENTER, "删除", FontId::proportional(11.5), danger);
        if dh { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        if ui.interact(dr, egui::Id::new("life_ed_del"), egui::Sense::click()).clicked() {
            let id = ed.id.unwrap();
            if is_b { st.birthdays.retain(|b| b.id != id); st.save_birthdays(); }
            else { st.anniversaries.retain(|x| x.id != id); st.save_anniversaries(); }
            st.edit = None;
            return;
        }
    }
    let cr = Rect::from_min_size(Pos2::new(rx - 142.0, by), Vec2::new(64.0, bh));
    let ch = ui.rect_contains_pointer(cr);
    painter.rect_stroke(cr, 8.0, Stroke::new(1.0, c(52, 211, 153, (a as f32 * 0.20) as u8)), StrokeKind::Inside);
    painter.text(cr.center(), Align2::CENTER_CENTER, "取消", FontId::proportional(11.5), if ch { ink } else { muted });
    if ch { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    if ui.interact(cr, egui::Id::new("life_ed_cancel"), egui::Sense::click()).clicked() { st.edit = None; return; }
    let sr = Rect::from_min_size(Pos2::new(rx - 70.0, by), Vec2::new(70.0, bh));
    let sh = ui.rect_contains_pointer(sr);
    painter.rect_stroke(sr, 8.0, Stroke::new(1.0, c(52, 211, 153, (a as f32 * if sh { 0.62 } else { 0.42 }) as u8)), StrokeKind::Inside);
    painter.text(sr.center(), Align2::CENTER_CENTER, "保存", crate::bold_font(11.5), accent);
    if sh { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    if ui.interact(sr, egui::Id::new("life_ed_save"), egui::Sense::click()).clicked() && date_ok {
        let (yy, mm, dd) = (yr.unwrap(), mo.unwrap(), dy.unwrap());
        let name = if ed.name.trim().is_empty() { if is_b { "未命名" } else { "纪念日" } } else { ed.name.trim() };
        if is_b {
            let id = ed.id.unwrap_or_else(|| { st.next_id += 1; st.next_id });
            let rec = Birthday { id, name: name.to_string(), lunar: ed.lunar, year: yy as u32, month: mm, day: dd, leap: ed.leap, rel: ed.rel.clone() };
            if let Some(pos) = st.birthdays.iter().position(|b| b.id == id) { st.birthdays[pos] = rec; } else { st.birthdays.push(rec); }
            st.save_birthdays();
        } else {
            let id = ed.id.unwrap_or_else(|| { st.next_id += 1; st.next_id });
            let rec = Anniversary { id, name: name.to_string(), year: yy as u32, month: mm, day: dd, yearly: ed.yearly, cat: ed.rel.clone() };
            if let Some(pos) = st.anniversaries.iter().position(|x| x.id == id) { st.anniversaries[pos] = rec; } else { st.anniversaries.push(rec); }
            st.save_anniversaries();
        }
        st.edit = None;
        return;
    }

    st.edit = Some(ed);
}

/// 日期选择面板：公历=日历网格（可翻月）；农历=年步进 + 月/日 网格
fn draw_date_picker(ui: &mut egui::Ui, painter: &egui::Painter, area: Rect, ed: &mut EditState,
                    a: u8, ink: Color32, muted: Color32, muted2: Color32, accent: Color32) {
    let t = today();
    if ed.pick_y == 0 { ed.pick_y = ed.year.trim().parse().unwrap_or(t.0); }
    if ed.pick_m == 0 { ed.pick_m = ed.month.trim().parse().unwrap_or(t.1); }
    if std::env::var_os("WINISLE_LIFE_YEARS").is_some() {
        ed.pick_ymode = true;
        if ed.pick_ypage == 0 {
            let cy = if ed.lunar { ed.year.trim().parse().unwrap_or(t.0) } else { ed.pick_y };
            ed.pick_ypage = (cy.clamp(1900, 2100) / 20) * 20;
        }
    }

    // 返回
    let xr = Rect::from_min_size(Pos2::new(area.right() - 42.0, area.top()), Vec2::new(42.0, 20.0));
    let xh = ui.rect_contains_pointer(xr);
    painter.text(xr.center(), Align2::CENTER_CENTER, "返回", FontId::proportional(10.5), if xh { ink } else { muted2 });
    if xh { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    if ui.interact(xr, egui::Id::new("life_pick_close"), egui::Sense::click()).clicked() { ed.pick_open = false; return; }

    // 左右切换（公历=换月；农历=换年）
    let hdr_y = area.top();
    let prev = Rect::from_min_size(Pos2::new(area.left() + 2.0, hdr_y), Vec2::new(24.0, 22.0));
    let next = Rect::from_min_size(Pos2::new(area.right() - 70.0, hdr_y), Vec2::new(24.0, 22.0));
    for (rr, g, wid) in [(prev, "<", "life_pick_prev"), (next, ">", "life_pick_next")] {
        let h = ui.rect_contains_pointer(rr);
        painter.text(rr.center(), Align2::CENTER_CENTER, g, FontId::proportional(13.0), if h { accent } else { muted });
        if h { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        if ui.interact(rr, egui::Id::new(wid), egui::Sense::click()).clicked() {
            let dir: i32 = if wid == "life_pick_prev" { -1 } else { 1 };
            if ed.pick_ymode {
                ed.pick_ypage = (ed.pick_ypage + dir * 20).clamp(1900, 2081);
            } else if ed.lunar {
                let ny = (ed.year.trim().parse::<i32>().unwrap_or(t.0) + dir).clamp(1900, 2100);
                ed.year = ny.to_string();
            } else {
                let mut m = ed.pick_m as i32 + dir;
                if m < 1 { m = 12; ed.pick_y -= 1; } else if m > 12 { m = 1; ed.pick_y += 1; }
                ed.pick_m = m as u32;
            }
        }
    }
    let cur_year = if ed.lunar { ed.year.trim().parse::<i32>().unwrap_or(t.0) } else { ed.pick_y };
    let header = if ed.pick_ymode {
        format!("{} – {} 年", ed.pick_ypage, ed.pick_ypage + 19)
    } else if ed.lunar {
        format!("{:04} 年 · 农历", cur_year)
    } else {
        format!("{} 年 {} 月", ed.pick_y, ed.pick_m)
    };
    painter.text(Pos2::new(area.center().x, hdr_y + 11.0), Align2::CENTER_CENTER, header, crate::bold_font(13.0), ink);
    let hr = Rect::from_center_size(Pos2::new(area.center().x, hdr_y + 11.0), Vec2::new(150.0, 20.0));
    let hhov = ui.rect_contains_pointer(hr);
    if hhov { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    if ui.interact(hr, egui::Id::new("life_pick_ymode"), egui::Sense::click()).clicked() {
        ed.pick_ymode = !ed.pick_ymode;
        if ed.pick_ymode { ed.pick_ypage = ((cur_year.clamp(1900, 2100)) / 20) * 20; }
    }
    if ed.pick_ymode {
        // 年份快速选择：20 年一页，4 列 × 5 行
        let base = ed.pick_ypage;
        let gy = hdr_y + 26.0;
        let gh = area.bottom() - gy - 4.0;
        let cw2 = (area.width() - 12.0) / 4.0;
        let rhh = (gh / 5.0).clamp(18.0, 30.0);
        for i in 0..20i32 {
            let yr = base + i;
            if yr < 1900 || yr > 2100 { continue; }
            let col = (i % 4) as f32;
            let row = (i / 4) as f32;
            let cell = Rect::from_min_size(Pos2::new(area.left() + 6.0 + cw2 * col, gy + rhh * row),
                Vec2::new(cw2 - 4.0, rhh - 4.0));
            let on = yr == cur_year;
            let h = ui.rect_contains_pointer(cell);
            if h { painter.rect_filled(cell, 6.0, c(52, 211, 153, (a as f32 * 0.16) as u8)); ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
            painter.text(cell.center(), Align2::CENTER_CENTER, format!("{}", yr), FontId::proportional(11.5), if on { accent } else if h { ink } else { muted });
            if ui.interact(cell, egui::Id::new(("life_pick_yr", i)), egui::Sense::click()).clicked() {
                if ed.lunar { ed.year = yr.to_string(); } else { ed.pick_y = yr; }
                ed.pick_ymode = false;
            }
        }
        return;
    }

    let cw = area.width() / 7.0;
    if ed.lunar {
        let ly = ed.year.trim().parse::<i32>().unwrap_or(t.0).clamp(1900, 2100);
        if ed.year.trim().is_empty() { ed.year = ly.to_string(); }
        let lm = leap_month(ly);
        let mut months: Vec<(u32, bool)> = Vec::new();
        for mm in 1..=12u32 { months.push((mm, false)); if lm > 0 && lm as u32 == mm { months.push((mm, true)); } }
        painter.text(Pos2::new(area.left() + 2.0, hdr_y + 21.0), Align2::LEFT_TOP, "月份", FontId::proportional(9.5), muted2);
        let mtop = hdr_y + 21.0 + 13.0;
        let mrh = 19.0;
        let mcols = 7usize;
        let mrows = (months.len() + mcols - 1) / mcols;
        let cur_m = ed.month.trim().parse::<u32>().unwrap_or(1);
        for (i, (mm, isleap)) in months.iter().enumerate() {
            let col = i % mcols; let row = i / mcols;
            let cell = Rect::from_min_size(Pos2::new(area.left() + cw * col as f32, mtop + mrh * row as f32), Vec2::new(cw, mrh));
            let on = *mm == cur_m && *isleap == ed.leap;
            let h = ui.rect_contains_pointer(cell);
            let label = if *isleap { format!("闰{}", mm) } else { format!("{}", mm) };
            painter.text(cell.center(), Align2::CENTER_CENTER, label, FontId::proportional(10.5), if on { accent } else if h { ink } else { muted });
            if h { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
            if ui.interact(cell, egui::Id::new(("life_pick_mon", i)), egui::Sense::click()).clicked() {
                ed.month = mm.to_string(); ed.leap = *isleap;
            }
        }
        let dlabel_y = mtop + mrh * mrows as f32 + 2.0;
        painter.text(Pos2::new(area.left() + 2.0, dlabel_y), Align2::LEFT_TOP, "日期", FontId::proportional(9.5), muted2);
        let dtop = dlabel_y + 12.0;
        let dim = if ed.leap && leap_month(ly) == cur_m as i32 { leap_days(ly) as usize } else { month_days_l(ly, cur_m).max(0) as usize };
        let cur_d = ed.day.trim().parse::<u32>().unwrap_or(0);
        let drh = ((area.bottom() - dtop) / 5.0).clamp(14.0, 22.0);
        for d in 1..=dim {
            let idx = d - 1;
            let col = idx % 7; let row = idx / 7;
            let cell = Rect::from_min_size(Pos2::new(area.left() + cw * col as f32, dtop + drh * row as f32), Vec2::new(cw, drh));
            let on = d as u32 == cur_d;
            let h = ui.rect_contains_pointer(cell);
            painter.text(cell.center(), Align2::CENTER_CENTER, format!("{}", d), FontId::proportional(11.0), if on { accent } else if h { accent } else { ink });
            if h { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
            if ui.interact(cell, egui::Id::new(("life_pick_day_l", d)), egui::Sense::click()).clicked() {
                ed.day = d.to_string(); ed.pick_open = false;
            }
        }
    } else {
        let wk = ["日", "一", "二", "三", "四", "五", "六"];
        let gtop = hdr_y + 24.0;
        let rh2 = ((area.bottom() - gtop) / 7.0).clamp(15.0, 26.0);
        for (i, w) in wk.iter().enumerate() {
            painter.text(Pos2::new(area.left() + cw * (i as f32 + 0.5), gtop + rh2 * 0.5), Align2::CENTER_CENTER, *w, FontId::proportional(9.5), muted2);
        }
        let first = days_from_civil(ed.pick_y, ed.pick_m, 1);
        let lead = ((first + 4).rem_euclid(7)) as i64; // 0=周日
        let dim = mdays(ed.pick_y, ed.pick_m) as i64;
        let cyy = ed.year.trim().parse::<i32>().ok();
        let cmm = ed.month.trim().parse::<u32>().ok();
        let cdd = ed.day.trim().parse::<u32>().ok();
        for idx in 0..42i64 {
            let d = idx - lead + 1;
            if d < 1 || d > dim { continue; }
            let col = idx % 7; let row = idx / 7;
            let cell = Rect::from_min_size(Pos2::new(area.left() + cw * col as f32, gtop + rh2 * (row as f32 + 1.0)), Vec2::new(cw, rh2));
            let on = cyy == Some(ed.pick_y) && cmm == Some(ed.pick_m) && cdd == Some(d as u32);
            let h = ui.rect_contains_pointer(cell);
            painter.text(cell.center(), Align2::CENTER_CENTER, format!("{}", d), FontId::proportional(11.0), if on { accent } else if h { accent } else { ink });
            if h { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
            if ui.interact(cell, egui::Id::new(("life_pick_day_s", idx)), egui::Sense::click()).clicked() {
                ed.year = ed.pick_y.to_string();
                ed.month = ed.pick_m.to_string();
                ed.day = d.to_string();
                ed.pick_open = false;
            }
        }
    }
}

/// 公历日期选择器（浮层日历）。返回被点选的日期；点 × 时置 close=true。
#[allow(clippy::too_many_arguments)]
fn draw_solar_picker(ui: &mut egui::Ui, painter: &egui::Painter, area: Rect,
                     py: &mut i32, pm: &mut u32, a: u8,
                     ink: Color32, muted: Color32, muted2: Color32, accent: Color32,
                     close: &mut bool, ymode: &mut bool, ypage: &mut i32) -> Option<(i32, u32, u32)> {
    painter.rect_filled(area, 10.0, c(24, 26, 33, 255));
    painter.rect_stroke(area, 10.0, Stroke::new(1.0, c(52, 211, 153, (a as f32 * 0.16) as u8)), StrokeKind::Inside);
    let pad = 12.0;
    let hy = area.top() + 8.0;
    let prev = Rect::from_min_size(Pos2::new(area.left() + pad, hy), Vec2::new(20.0, 20.0));
    let next = Rect::from_min_size(Pos2::new(area.left() + pad + 24.0, hy), Vec2::new(20.0, 20.0));
    let cx = Rect::from_min_size(Pos2::new(area.right() - pad - 42.0, hy), Vec2::new(42.0, 20.0));
    painter.text(prev.center(), Align2::CENTER_CENTER, "‹", FontId::proportional(14.0), muted);
    painter.text(next.center(), Align2::CENTER_CENTER, "›", FontId::proportional(14.0), muted);
    painter.text(cx.center(), Align2::CENTER_CENTER, "返回", FontId::proportional(10.5), muted);
    let title = if *ymode { format!("{} – {} 年", *ypage, *ypage + 19) } else { format!("{} 年 {} 月", *py, *pm) };
    painter.text(Pos2::new(area.center().x, hy + 10.0), Align2::CENTER_CENTER, title, FontId::proportional(12.0), ink);
    let tr = Rect::from_center_size(Pos2::new(area.center().x, hy + 10.0), Vec2::new(150.0, 20.0));
    if ui.rect_contains_pointer(prev) || ui.rect_contains_pointer(next) || ui.rect_contains_pointer(cx) || ui.rect_contains_pointer(tr) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if ui.interact(prev, egui::Id::new("sp_prev"), egui::Sense::click()).clicked() {
        if *ymode { *ypage = (*ypage - 20).clamp(1900, 2081); }
        else if *pm == 1 { *pm = 12; *py -= 1; } else { *pm -= 1; }
    }
    if ui.interact(next, egui::Id::new("sp_next"), egui::Sense::click()).clicked() {
        if *ymode { *ypage = (*ypage + 20).clamp(1900, 2081); }
        else if *pm == 12 { *pm = 1; *py += 1; } else { *pm += 1; }
    }
    if ui.interact(cx, egui::Id::new("sp_close"), egui::Sense::click()).clicked() { *close = true; }
    if ui.interact(tr, egui::Id::new("sp_ymode"), egui::Sense::click()).clicked() {
        *ymode = !*ymode;
        if *ymode { *ypage = ((*py).clamp(1900, 2100) / 20) * 20; }
    }
    if *ymode {
        let base = *ypage;
        let gy = hy + 26.0;
        let gh = area.bottom() - gy - 4.0;
        let cw2 = (area.width() - 2.0 * pad) / 4.0;
        let rhh = (gh / 5.0).clamp(18.0, 30.0);
        for i in 0..20i32 {
            let yr = base + i;
            if yr < 1900 || yr > 2100 { continue; }
            let col = (i % 4) as f32;
            let row = (i / 4) as f32;
            let cell = Rect::from_min_size(Pos2::new(area.left() + pad + cw2 * col, gy + rhh * row),
                Vec2::new(cw2 - 4.0, rhh - 4.0));
            let on = yr == *py;
            let h = ui.rect_contains_pointer(cell);
            if h { painter.rect_filled(cell, 6.0, c(52, 211, 153, (a as f32 * 0.16) as u8)); ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
            painter.text(cell.center(), Align2::CENTER_CENTER, format!("{}", yr), FontId::proportional(11.5), if on { accent } else if h { ink } else { muted });
            if ui.interact(cell, egui::Id::new(("sp_yr", i)), egui::Sense::click()).clicked() {
                *py = yr;
                *ymode = false;
            }
        }
        return None;
    }

    let cols = 7.0f32;
    let gw = area.width() - pad * 2.0;
    let cw = gw / cols;
    let wk = ["日", "一", "二", "三", "四", "五", "六"];
    let wy = hy + 26.0;
    for (i, w) in wk.iter().enumerate() {
        painter.text(Pos2::new(area.left() + pad + cw * (i as f32 + 0.5), wy), Align2::CENTER_CENTER, *w,
            FontId::proportional(9.5), muted2);
    }
    let gy = wy + 12.0;
    let avail = area.bottom() - pad - gy;
    let ch = (avail / 6.0).clamp(10.0, 26.0);
    let lead = (days_from_civil(*py, *pm, 1) + 4).rem_euclid(7);
    let dim = mdays(*py, *pm);
    let t = today();
    let mut result = None;
    for d in 1..=dim {
        let idx = lead + (d as i64 - 1);
        let row = (idx / 7) as i32;
        let col = (idx % 7) as i32;
        let cell = Rect::from_min_size(Pos2::new(area.left() + pad + cw * col as f32, gy + ch * row as f32),
            Vec2::new(cw - 2.0, ch - 2.0));
        let is_today = t.0 == *py && t.1 == *pm && t.2 == d;
        let hov = ui.rect_contains_pointer(cell);
        if hov {
            painter.rect_filled(cell, 5.0, c(52, 211, 153, (a as f32 * 0.16) as u8));
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        painter.text(cell.center(), Align2::CENTER_CENTER, format!("{}", d), FontId::proportional(10.5),
            if is_today { accent } else { ink });
        if ui.interact(cell, egui::Id::new(("sp_day", *py, *pm, d)), egui::Sense::click()).clicked() {
            result = Some((*py, *pm, d));
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn draw_subsidy(ui: &mut egui::Ui, painter: &egui::Painter, st: &mut LifeState,
                x0: f32, x1: f32, top: f32, bottom: f32, a: u8,
                ink: Color32, muted: Color32, muted2: Color32, accent: Color32, danger: Color32) {
    let mut changed = false;
    let content_w = x1 - x0;

    // 起止日期（点击弹日历选择）
    let label_y = top + 1.0;
    painter.text(Pos2::new(x0, label_y), Align2::LEFT_TOP, "开始出差", FontId::proportional(10.0), muted);
    let field_h = 24.0;
    let days_chip_w = 70.0;
    let gap = 8.0;
    let fw = (content_w - days_chip_w - gap * 2.0) / 2.0;
    let fy = label_y + 16.0;
    let r_start = Rect::from_min_size(Pos2::new(x0, fy), Vec2::new(fw, field_h));
    let r_end = Rect::from_min_size(Pos2::new(x0 + fw + gap, fy), Vec2::new(fw, field_h));
    painter.text(Pos2::new(x0 + fw + gap, label_y), Align2::LEFT_TOP, "结束出差", FontId::proportional(10.0), muted);
    let bar_h = 48.0;
    let bar_y = bottom - bar_h;
    for (which, rc) in [(1u8, r_start), (2u8, r_end)] {
        let val = if which == 1 { st.sub_start.clone() } else { st.sub_end.clone() };
        let empty = val.trim().is_empty();
        let hov = ui.rect_contains_pointer(rc);
        let active = st.sub_pick == which;
        if active {
            painter.rect_filled(rc, 8.0, c(52, 211, 153, (a as f32 * 0.10) as u8));
        }
        painter.rect_stroke(rc, 8.0, Stroke::new(1.0, c(52, 211, 153, if hov || active { 140 } else { 40 })), StrokeKind::Inside);
        painter.text(Pos2::new(rc.left() + 10.0, rc.center().y), Align2::LEFT_CENTER,
            if empty { "点击选择日期" } else { &val },
            FontId::proportional(12.0), if empty { muted2 } else { ink });
        painter.text(Pos2::new(rc.right() - 10.0, rc.center().y), Align2::RIGHT_CENTER, "▾",
            FontId::proportional(9.0), muted2);
        if hov { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        if ui.interact(rc, egui::Id::new(("life_sub_date", which)), egui::Sense::click()).clicked() {
            st.sub_pick = which;
            let d = parse_date(if which == 1 { &st.sub_start } else { &st.sub_end }).unwrap_or_else(today);
            st.sub_pick_y = d.0; st.sub_pick_m = d.1;
        }
    }
    // 天数/计算
    let sd = parse_date(&st.sub_start);
    let ed = parse_date(&st.sub_end);
    let (mut days, mut total, mut elapsed, mut todate) = if let (Some(s), Some(e)) = (sd, ed) {
        subsidy_calc(s, e, &st.brackets)
    } else { (0, 0.0, 0, 0.0) };
    // 日期区间校验：结束不能早于开始
    let mut vmsg: Option<String> = None;
    if let (Some(s), Some(e)) = (sd, ed) {
        if e < s { vmsg = Some("结束日期不能早于开始日期".to_string()); }
    }
    if vmsg.is_none() { vmsg = validate_brackets(&st.brackets); }
    let chip_rect = Rect::from_min_size(Pos2::new(x1 - days_chip_w, fy), Vec2::new(days_chip_w, field_h));
    let chip_label = if days > 0 { format!("共{}天", days) } else { "— 天".to_string() };
    painter.text(chip_rect.center(), Align2::CENTER_CENTER, chip_label, crate::bold_font(12.5), accent);

    // 区间标题（左：标题/段数 或 校验提示；右上：常驻「添加区间」）
    let n_br = st.brackets.len();
    let bh_y = fy + field_h + 6.0;
    let br_top = bh_y + 15.0;
    let br_h = 24.0;
    let pitch = br_h + 6.0;
    let list_top = br_top - 2.0;
    let list_bottom = bar_y - 8.0;
    let avail_h = (list_bottom - list_top).max(pitch);
    let visible = (((avail_h + 4.0) / pitch).floor() as usize).max(1);
    let max_first = n_br.saturating_sub(visible);
    if st.sub_br_first > max_first { st.sub_br_first = max_first; }
    if let Ok(v) = std::env::var("WINISLE_SUB_FIRST") { if let Ok(n) = v.parse::<usize>() { st.sub_br_first = n.min(max_first); } }
    let mut remove_idx: Option<usize> = None;

    painter.text(Pos2::new(x0, bh_y), Align2::LEFT_TOP, "补贴区间", FontId::proportional(10.5), muted);
    if let Some(msg) = &vmsg {
        painter.text(Pos2::new(x0 + 50.0, bh_y + 1.0), Align2::LEFT_TOP,
            format!("⚠ {}", msg), FontId::proportional(9.5), danger);
    } else if n_br > visible {
        painter.text(Pos2::new(x0 + 50.0, bh_y + 1.0), Align2::LEFT_TOP,
            format!("共{}段 · 滚轮查看更多", n_br), FontId::proportional(9.5), muted2);
    } else {
        painter.text(Pos2::new(x0 + 50.0, bh_y + 1.0), Align2::LEFT_TOP,
            "按出差天数累计 · 首尾都算", FontId::proportional(9.0), muted2);
    }
    if st.sub_pick == 0 {
        let last_hi_now = st.brackets.last().map(|b| br_hi(b)).unwrap_or(1);
        let last_rate_ok = st.brackets.last().map(|b| !b.rate.trim().is_empty() && br_rate(b) > 0.0).unwrap_or(true);
        let can_add = st.brackets.is_empty() || (last_hi_now > 0 && last_rate_ok);
        let add_rect = Rect::from_min_max(Pos2::new(x1 - 78.0, bh_y - 3.0), Pos2::new(x1, bh_y + 15.0));
        let add_hov = ui.rect_contains_pointer(add_rect);
        if add_hov { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        painter.text(Pos2::new(x1, bh_y + 6.0), Align2::RIGHT_CENTER, "＋ 添加区间",
            FontId::proportional(10.5), if can_add { accent } else { muted2 });
        if ui.interact(add_rect, egui::Id::new("life_br_add"), egui::Sense::click()).clicked() {
            if can_add {
                let last_hi = st.brackets.last().map(|b| br_hi(b)).unwrap_or(1);
                let lo = if st.brackets.is_empty() { 1 } else { last_hi + 1 };
                st.brackets.push(Bracket { lo: lo.to_string(), hi: String::new(), rate: String::new() });
                let new_idx = st.brackets.len() - 1;
                st.sub_br_first = st.brackets.len().saturating_sub(visible);
                ui.memory_mut(|m| m.request_focus(egui::Id::new(("life_br_hi", new_idx))));
                changed = true;
            } else if let Some(last) = st.brackets.len().checked_sub(1) {
                // 定位到缺失项：结束天数没填→定位结束天数；已填但金额没填→定位金额
                let hi_ok = br_hi(&st.brackets[last]) > 0;
                let id = if hi_ok { egui::Id::new(("life_br_rate", last)) } else { egui::Id::new(("life_br_hi", last)) };
                ui.memory_mut(|m| m.request_focus(id));
            }
        }
    }
    let list_rect = Rect::from_min_max(Pos2::new(x0, list_top), Pos2::new(x1, list_bottom));
    if n_br > visible && ui.rect_contains_pointer(list_rect) {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        if dy > 0.0 { st.sub_br_first = st.sub_br_first.saturating_sub(1); }
        else if dy < 0.0 { st.sub_br_first = (st.sub_br_first + 1).min(max_first); }
    }
    for i in st.sub_br_first..n_br {
        let by = list_top + 2.0 + (i - st.sub_br_first) as f32 * pitch;
        if by + br_h > list_bottom { break; }
        let is_new = i + 1 == n_br && br_hi(&st.brackets[i]) == 0;
        if is_new {
            painter.rect_filled(Rect::from_min_size(Pos2::new(x0 - 4.0, by - 1.0), Vec2::new(content_w + 8.0, br_h + 2.0)), 7.0, c(52, 211, 153, (a as f32 * 0.07) as u8));
            painter.rect_filled(Rect::from_min_size(Pos2::new(x0 - 4.0, by - 1.0), Vec2::new(3.0, br_h + 2.0)), 1.5, accent);
        }
        painter.text(Pos2::new(x0, by + br_h * 0.5), Align2::LEFT_CENTER, "第", FontId::proportional(10.0), muted);
        let lo_rect = Rect::from_min_size(Pos2::new(x0 + 18.0, by), Vec2::new(44.0, br_h));
        if i == 0 {
            if text_field(ui, painter, lo_rect, egui::Id::new(("life_br_lo", i)), &mut st.brackets[i].lo, 11.5, ink, 3) { changed = true; }
        } else {
            // 起始天数锁定为「上一段结束 + 1」，逐段自动级联；上一段未填结束则显示 —
            let ph = br_hi(&st.brackets[i - 1]);
            if ph > 0 {
                let auto_lo = ph + 1;
                st.brackets[i].lo = auto_lo.to_string();
                painter.rect_stroke(lo_rect, 8.0, Stroke::new(1.0, c(52, 211, 153, (a as f32 * 0.10) as u8)), StrokeKind::Inside);
                painter.text(lo_rect.center(), Align2::CENTER_CENTER, auto_lo.to_string(), FontId::proportional(11.5), muted);
            } else {
                st.brackets[i].lo.clear();
                painter.rect_stroke(lo_rect, 8.0, Stroke::new(1.0, c(52, 211, 153, (a as f32 * 0.10) as u8)), StrokeKind::Inside);
                painter.text(lo_rect.center(), Align2::CENTER_CENTER, "—", FontId::proportional(11.5), muted2);
            }
        }
        painter.text(Pos2::new(x0 + 66.0, by + br_h * 0.5), Align2::LEFT_CENTER, "–", FontId::proportional(11.0), muted);
        let hi_rect = Rect::from_min_size(Pos2::new(x0 + 78.0, by), Vec2::new(44.0, br_h));
        if text_field(ui, painter, hi_rect, egui::Id::new(("life_br_hi", i)), &mut st.brackets[i].hi, 11.5, ink, 3) { changed = true; }
        painter.text(Pos2::new(x0 + 126.0, by + br_h * 0.5), Align2::LEFT_CENTER, "天", FontId::proportional(10.0), muted);
        // 右侧金额
        painter.text(Pos2::new(x1 - 116.0, by + br_h * 0.5), Align2::RIGHT_CENTER, "¥", FontId::proportional(11.0), accent);
        let rate_rect = Rect::from_min_size(Pos2::new(x1 - 108.0, by), Vec2::new(58.0, br_h));
        if text_field(ui, painter, rate_rect, egui::Id::new(("life_br_rate", i)), &mut st.brackets[i].rate, 12.0, accent, 3) { changed = true; }
        painter.text(Pos2::new(x1 - 46.0, by + br_h * 0.5), Align2::LEFT_CENTER, "/天", FontId::proportional(10.0), muted);
        // 删除按钮
        let del_rect = Rect::from_center_size(Pos2::new(x1 - 9.0, by + br_h * 0.5), Vec2::new(16.0, br_h - 8.0));
        let dh = ui.rect_contains_pointer(del_rect);
        if dh { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        painter.text(del_rect.center(), Align2::CENTER_CENTER, "✕", FontId::proportional(11.0), if dh { danger } else { muted2 });
        if ui.interact(del_rect, egui::Id::new(("life_br_del", i)), egui::Sense::click()).clicked() { remove_idx = Some(i); }
    }
    if let Some(ri) = remove_idx {
        if n_br > 1 { st.brackets.remove(ri); changed = true; }
    }

    // 添加区间按钮已移至区间标题右上角（常驻可见，无需滚到底部查找）

    // 日历浮层（覆盖区间编辑区）
    if st.sub_pick > 0 {
        if std::env::var_os("WINISLE_SUB_YEARS").is_some() {
            st.sub_ymode = true;
            if st.sub_ypage == 0 { st.sub_ypage = (st.sub_pick_y.clamp(1900, 2100) / 20) * 20; }
        }
        let area = Rect::from_min_max(Pos2::new(x0, fy - 16.0), Pos2::new(x1, bar_y - 8.0));
        let mut close = false;
        if let Some((yy, mm, dd)) = draw_solar_picker(ui, painter, area, &mut st.sub_pick_y, &mut st.sub_pick_m,
                a, ink, muted, muted2, accent, &mut close, &mut st.sub_ymode, &mut st.sub_ypage) {
            let s = fmt_ymd((yy, mm, dd));
            if st.sub_pick == 1 { st.sub_start = s; } else { st.sub_end = s; }
            st.sub_pick = 0; changed = true;
        }
        if close { st.sub_pick = 0; }
        let sd2 = parse_date(&st.sub_start);
        let ed2 = parse_date(&st.sub_end);
        if let (Some(s), Some(e)) = (sd2, ed2) {
            let r = subsidy_calc(s, e, &st.brackets);
            days = r.0; total = r.1; elapsed = r.2; todate = r.3;
        }
    }

    // 区间合理性校验提示已并入“添加区间”同一行，避免与按钮重叠

    // 结果条（三栏等宽：出差天数 / 至今补贴+进度 / 总补贴）——不填充底色，仅一条顶部分隔线
    let bar_rect = Rect::from_min_max(Pos2::new(x0, bar_y), Pos2::new(x1, bottom));
    let _ = bar_rect;
    painter.line_segment([Pos2::new(x0, bar_y), Pos2::new(x1, bar_y)],
        Stroke::new(1.0, c(52, 211, 153, (a as f32 * 0.12) as u8)));
    let col_w = content_w / 3.0;
    let base_y = bar_y + 9.0;
    for k in 1..3 {
        let dx = x0 + col_w * k as f32;
        painter.line_segment([Pos2::new(dx, bar_y + 9.0), Pos2::new(dx, bar_y + bar_h - 8.0)],
            Stroke::new(1.0, c(52, 211, 153, (a as f32 * 0.10) as u8)));
    }
    // 1 出差天数
    let c1 = x0 + 16.0;
    painter.text(Pos2::new(c1, base_y), Align2::LEFT_TOP, "出差天数", FontId::proportional(9.5), muted);
    painter.text(Pos2::new(c1, base_y + 11.0), Align2::LEFT_TOP, format!("{} 天", days), crate::bold_font(14.0), ink);
    // 2 至今补贴 + 进度条
    let c2 = x0 + col_w + 16.0;
    painter.text(Pos2::new(c2, base_y), Align2::LEFT_TOP, "至今补贴", FontId::proportional(9.5), muted);
    painter.text(Pos2::new(c2, base_y + 11.0), Align2::LEFT_TOP, money(todate), crate::bold_font(14.0), ink);
    let pb = Rect::from_min_size(Pos2::new(c2, base_y + 30.0), Vec2::new(col_w - 32.0, 3.0));
    painter.rect_filled(pb, 1.5, c(52, 211, 153, (a as f32 * 0.16) as u8));
    let frac = if days > 0 { (elapsed as f32 / days as f32).clamp(0.0, 1.0) } else { 0.0 };
    painter.rect_filled(Rect::from_min_size(pb.min, Vec2::new(pb.width() * frac, 3.0)), 1.5, accent);
    // 3 总补贴
    let c3 = x0 + col_w * 2.0 + 16.0;
    painter.text(Pos2::new(c3, base_y), Align2::LEFT_TOP, "总补贴", FontId::proportional(9.5), muted);
    painter.text(Pos2::new(c3, base_y + 12.0), Align2::LEFT_TOP, money(total), crate::bold_font(18.0), accent);

    if changed { st.save_subsidy(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lunar_new_year() {
        assert_eq!(lunar_to_solar(2024, 1, 1, false), Some((2024, 2, 10)));
        assert_eq!(lunar_to_solar(2025, 1, 1, false), Some((2025, 1, 29)));
        assert_eq!(lunar_to_solar(2023, 1, 1, false), Some((2023, 1, 22)));
        assert_eq!(lunar_to_solar(2024, 8, 15, false), Some((2024, 9, 17)));
    }

    #[test]
    fn subsidy_tiers() {
        let brs = vec![
            Bracket { lo: "1".into(), hi: "60".into(), rate: "100".into() },
            Bracket { lo: "61".into(), hi: "90".into(), rate: "200".into() },
        ];
        let (days, total, _, _) = subsidy_calc((2025, 10, 1), (2025, 12, 10), &brs);
        assert_eq!(days, 71);
        assert_eq!(total, 8200.0);
    }

    #[test]
    fn money_fmt() {
        assert_eq!(money(8200.0), "¥8,200");
        assert_eq!(money(0.0), "¥0");
    }
}

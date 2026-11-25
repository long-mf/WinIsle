//! 天气数据层（零 key）。
//!
//! 设计原则：用户打开即用，永不出现注册 / Key / 数据源概念。
//! 主源用 itboy（中国天气网数据，无需 key、国内直连、中文、15 天预报 + AQI），
//! 失败时回退到本地缓存（断网时显示上次数据 + 更新时间）。
//!
//! 本模块只负责“取数 + 解析 + 缓存”，不碰 UI；由 main.rs 的后台线程调用。

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 程序化天气图标种类（与 UI 的绘制函数一一对应）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sky {
    Sun,
    Partly,
    Cloud,
    Rain,
    Thunder,
    Snow,
    Fog,
    Night,
}

/// 一天（预报项）。温度单位为摄氏度整数；文本沿用数据源中文。
#[derive(Clone, Debug, Default)]
pub struct Day {
    pub date: String,     // ymd，如 2026-11-18
    pub week: String,     // 星期三
    pub high: i32,        // 最高温 ℃
    pub low: i32,         // 最低温 ℃
    pub day_type: String, // 晴 / 多云 / 小雨 / 霾 …
    pub sunrise: String,  // 07:02
    pub sunset: String,   // 16:56
    pub aqi: i32,
    pub wind_dir: String,  // 西北风
    pub wind_level: String, // 2级
    pub notice: String,
}

/// 一个 3 小时粒度的预报点（优先中国天气网 hour3data，回退 7timer；用于展开页「未来 24 小时」横条）。
#[derive(Clone, Debug)]
pub struct Hour {
    pub label: String, // 本地时钟，如 "14:00"
    pub temp: i32,     // 摄氏度
    pub sky: Sky,
}

/// 聚合后的天气数据（一次成功拉取的完整快照）。
#[derive(Clone, Debug, Default)]
pub struct Weather {
    pub city: String,
    pub city_key: String,
    pub temp: f32,       // 实时温度 ℃
    pub humidity: String, // "29%"
    pub pm25: f32,
    pub pm10: f32,
    pub quality: String, // 优 / 良 / 轻度污染 …
    pub ganmao: String,  // 感冒提示
    pub updated: String, // 数据源更新时间 "07:32"
    pub today_type: String,
    pub days: Vec<Day>,
    /// 未来 24 小时（3 小时间隔）。可能为空（缓存命中或源不可用时）。
    pub hours: Vec<Hour>,
    pub fetched_at: u64, // 本机拉取时刻（unix 秒）
    pub from_cache: bool,
}

impl Weather {
    pub fn today(&self) -> Option<&Day> {
        self.days.first()
    }
    /// 今天的最高 / 最低（用于 hero 展示）。
    pub fn today_high_low(&self) -> (i32, i32) {
        self.days.first().map(|d| (d.high, d.low)).unwrap_or((0, 0))
    }
}

/// 城市搜索结果（中国天气网 search 接口，或 Open-Meteo 全球地名库）。
#[derive(Clone, Debug)]
pub struct CityHit {
    /// 中国天气网城市代码；全球地点为 "geo:纬度,经度"。
    pub key: String,
    pub name: String,
    pub province: String,
}

/// 实时观测（中国天气网 d1 接口，约 10 分钟更新）。
struct Realtime {
    temp: f32,
    humidity: String,
    pm25: f32,
    quality: String,
    weather: String,
    updated: String,
    wind_dir: String,
    wind_level: String,
}

/// 预置城市表。key 为中国天气网城市代码（itboy 接口路径参数）。
pub struct City {
    pub name: &'static str,
    pub key: &'static str,
}

pub const CITIES: &[City] = &[
    City { name: "北京", key: "101010100" },
    City { name: "上海", key: "101020100" },
    City { name: "广州", key: "101280101" },
    City { name: "深圳", key: "101280601" },
    City { name: "杭州", key: "101210101" },
    City { name: "南京", key: "101190101" },
    City { name: "成都", key: "101270101" },
    City { name: "重庆", key: "101040100" },
    City { name: "武汉", key: "101200101" },
    City { name: "西安", key: "101110101" },
    City { name: "哈尔滨", key: "101050101" },
    City { name: "三亚", key: "101310201" },
];

pub fn city_index(key: &str) -> Option<usize> {
    CITIES.iter().position(|c| c.key == key)
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 摄氏度 -> 展示温度（按单位换算，四舍五入）。
pub fn conv_temp(c: f32, fahrenheit: bool) -> f32 {
    if fahrenheit {
        c * 9.0 / 5.0 + 32.0
    } else {
        c
    }
}

pub fn temp_unit(fahrenheit: bool) -> &'static str {
    if fahrenheit {
        "°F"
    } else {
        "°C"
    }
}

/// 由中文天气描述推断图标种类。`is_night` 仅在“晴/多云”时用于切换夜间样式。
pub fn sky_of(desc: &str, is_night: bool) -> Sky {
    let s = desc;
    if s.contains("雷") {
        Sky::Thunder
    } else if s.contains("雪") || s.contains("冰") || s.contains("冻") {
        Sky::Snow
    } else if s.contains("雨") {
        Sky::Rain
    } else if s.contains("雾") || s.contains("霾") || s.contains("沙") || s.contains("尘") {
        Sky::Fog
    } else if s.contains("阴") {
        Sky::Cloud
    } else if s.contains("多云") || s.contains("少云") || s.contains("晴间") {
        if is_night {
            Sky::Night
        } else {
            Sky::Partly
        }
    } else if s.contains("晴") {
        if is_night {
            Sky::Night
        } else {
            Sky::Sun
        }
    } else {
        Sky::Cloud
    }
}

/// 从 “高温 9℃ / 低温 -1℃” 之类文本里抽出第一个整数温度。
fn parse_temp_text(s: &str) -> Option<i32> {
    let t: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '-').collect();
    t.parse::<i32>().ok()
}

fn as_f32(v: &serde_json::Value, k: &str) -> f32 {
    v.get(k)
        .and_then(|x| x.as_f64())
        .map(|x| x as f32)
        .unwrap_or(0.0)
}

/// 解析 itboy 返回的 JSON 根对象。
pub fn parse_itboy(root: &serde_json::Value) -> Option<Weather> {
    let city_info = root.get("cityInfo")?;
    let data = root.get("data")?;
    let mut w = Weather {
        city: city_info.get("city").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        city_key: city_info.get("citykey").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        updated: city_info.get("updateTime").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        humidity: data.get("shidu").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        temp: data
            .get("wendu")
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0),
        pm25: as_f32(data, "pm25"),
        pm10: as_f32(data, "pm10"),
        quality: data.get("quality").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        ganmao: data.get("ganmao").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        fetched_at: now_secs(),
        from_cache: false,
        ..Default::default()
    };
    if let Some(arr) = data.get("forecast").and_then(|x| x.as_array()) {
        for it in arr {
            let high = it.get("high").and_then(|x| x.as_str()).and_then(parse_temp_text).unwrap_or(0);
            let low = it.get("low").and_then(|x| x.as_str()).and_then(parse_temp_text).unwrap_or(0);
            let aqi = it.get("aqi").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
            w.days.push(Day {
                date: it.get("ymd").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                week: it.get("week").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                high,
                low,
                day_type: it.get("type").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                sunrise: it.get("sunrise").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                sunset: it.get("sunset").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                aqi,
                wind_dir: it.get("fx").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                wind_level: it.get("fl").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                notice: it.get("notice").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            });
        }
    }
    w.today_type = w.days.first().map(|d| d.day_type.clone()).unwrap_or_default();
    Some(w)
}

/// 自动定位得到的区县代码 -> 精确经纬度（供 7timer 逐小时使用，进程内缓存）。
fn coord_cache() -> &'static Mutex<HashMap<String, (f64, f64)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (f64, f64)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 城市 key -> (纬度, 经度)，供 7timer 请求使用。全球地点自带坐标。
fn city_coords(key: &str) -> Option<(f64, f64)> {
    if let Ok(cache) = coord_cache().lock() {
        if let Some(&c) = cache.get(key) {
            return Some(c);
        }
    }
    if let Some(rest) = key.strip_prefix("geo:") {
        let mut it = rest.split(',');
        let lat = it.next()?.trim().parse::<f64>().ok()?;
        let lon = it.next()?.trim().parse::<f64>().ok()?;
        return Some((lat, lon));
    }
    Some(match key {
        "101010100" => (39.90, 116.41), // 北京
        "101020100" => (31.23, 121.47), // 上海
        "101280101" => (23.13, 113.26), // 广州
        "101280601" => (22.54, 114.06), // 深圳
        "101210101" => (30.27, 120.15), // 杭州
        "101190101" => (32.06, 118.80), // 南京
        "101270101" => (30.57, 104.07), // 成都
        "101040100" => (29.56, 106.55), // 重庆
        "101200101" => (30.59, 114.31), // 武汉
        "101110101" => (34.34, 108.94), // 西安
        "101050101" => (45.80, 126.53), // 哈尔滨
        "101310201" => (18.25, 109.51), // 三亚
        _ => return None,
    })
}

/// URL 百分号编码（仅保留 RFC3986 unreserved 字符）。
fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        let c = *b;
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~') {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{:02X}", c));
        }
    }
    out
}

/// 轻量 HTTP GET（带可选 Referer），失败/超时/超长都返回 None。
fn http_get_text(url: &str, referer: Option<&str>, connect_ms: u64, read_ms: u64, cap: u64) -> Option<String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(connect_ms))
        .timeout_read(Duration::from_millis(read_ms))
        .build();
    let mut req = agent
        .get(url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) WinIsle-Weather");
    if let Some(r) = referer {
        req = req.set("Referer", r);
    }
    let resp = req.call().ok()?;
    let mut s = String::new();
    resp.into_reader().take(cap).read_to_string(&mut s).ok()?;
    Some(s)
}

/// 中国天气网城市搜索（全国城市 / 区县，无需 key）。
fn search_china(q: &str) -> Vec<CityHit> {
    let mut out = search_china_once(q);
    if out.is_empty() {
        // 该接口对部分地名（如“杭州”）偶尔返回空，补一个尾随空格可稳定命中
        out = search_china_once(&format!("{} ", q));
    }
    if out.is_empty() {
        let stripped = q.trim_end_matches(|c| c == '市' || c == '区' || c == '县');
        if stripped != q && !stripped.is_empty() {
            out = search_china_once(stripped);
            if out.is_empty() {
                out = search_china_once(&format!("{} ", stripped));
            }
        }
    }
    out
}

fn search_china_once(q: &str) -> Vec<CityHit> {
    let url = format!(
        "http://toy1.weather.com.cn/search?cityname={}",
        url_encode(q)
    );
    let Some(mut text) = http_get_text(&url, Some("http://www.weather.com.cn/"), 3000, 5000, 200 * 1024) else {
        return Vec::new();
    };
    let t = text.trim();
    text = t
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim()
        .to_string();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut out: Vec<CityHit> = Vec::new();
    if let Some(arr) = v.as_array() {
        for it in arr {
            let r = it.get("ref").and_then(|x| x.as_str()).unwrap_or("");
            let parts: Vec<&str> = r.split('~').collect();
            if parts.len() < 10 {
                continue;
            }
            let code = parts[0];
            // 只要 9 位纯数字的地级市 / 区县代码；过滤景点(A 结尾)、街道(12 位)等
            if code.len() != 9 || !code.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let name = parts[2].trim();
            if name.is_empty() {
                continue;
            }
            if out.iter().any(|h| h.key == code) {
                continue;
            }
            out.push(CityHit {
                key: code.to_string(),
                name: name.to_string(),
                province: parts[9].trim().to_string(),
            });
            if out.len() >= 8 {
                break;
            }
        }
    }
    out
}

/// 全球地名搜索（Open-Meteo geocoding，无需 key）；key 为 "geo:纬度,经度"。
fn search_geo(q: &str) -> Vec<CityHit> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=8&language=zh&format=json",
        url_encode(q)
    );
    let Some(text) = http_get_text(&url, None, 3000, 5000, 256 * 1024) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(arr) = v.get("results").and_then(|x| x.as_array()) {
        for it in arr {
            let name = it.get("name").and_then(|x| x.as_str()).unwrap_or("").trim();
            let (Some(lat), Some(lon)) = (
                it.get("latitude").and_then(|x| x.as_f64()),
                it.get("longitude").and_then(|x| x.as_f64()),
            ) else {
                continue;
            };
            let admin = it.get("admin1").and_then(|x| x.as_str()).unwrap_or("").trim();
            let country = it.get("country").and_then(|x| x.as_str()).unwrap_or("").trim();
            let province = if !admin.is_empty() {
                format!("{} · {}", admin, country)
            } else {
                country.to_string()
            };
            if name.is_empty() {
                continue;
            }
            out.push(CityHit {
                key: format!("geo:{:.5},{:.5}", lat, lon),
                name: name.to_string(),
                province,
            });
            if out.len() >= 8 {
                break;
            }
        }
    }
    out
}

/// 搜索城市 / 地点：先搜全国（中国天气网），没有再查全球地名库。
pub fn search_cities(q: &str) -> Vec<CityHit> {
    let q = q.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let mut out = search_china(q);
    if out.is_empty() {
        out = search_geo(q);
    }
    out
}

/// 空气质量指数 -> 中文等级。
fn quality_of_aqi(aqi: i32) -> &'static str {
    match aqi {
        i32::MIN..=50 => "优",
        51..=100 => "良",
        101..=150 => "轻度污染",
        151..=200 => "中度污染",
        201..=300 => "重度污染",
        _ => "严重污染",
    }
}

/// 拉取中国天气网实时观测（d1，10 分钟级，无需 key；失败返回 None 沿用 itboy 实时）。
fn fetch_realtime(code: &str) -> Option<Realtime> {
    let url = format!("http://d1.weather.com.cn/sk_2d/{}.html", code);
    let text = http_get_text(&url, Some("http://www.weather.com.cn/"), 3000, 5000, 128 * 1024)?;
    let idx = text.find('=')?;
    let js = text[idx + 1..].trim().trim_end_matches(';').trim();
    let v: serde_json::Value = serde_json::from_str(js).ok()?;
    let temp = v
        .get("temp")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<f32>().ok())?;
    let aqi = v
        .get("aqi")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);
    let pm25 = v
        .get("aqi_pm25")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.0);
    Some(Realtime {
        temp,
        humidity: v.get("SD").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        pm25,
        quality: quality_of_aqi(aqi).to_string(),
        weather: v.get("weather").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        updated: v.get("time").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        wind_dir: v.get("WD").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        wind_level: v.get("WS").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    })
}

/// 英文天气描述（wttr.in）-> (图标, 中文简述)。
fn sky_from_english(desc: &str) -> (Sky, String) {
    let l = desc.to_lowercase();
    let (sky, label) = if l.contains("thunder") || l.contains("storm") {
        (Sky::Thunder, "雷阵雨")
    } else if l.contains("snow") || l.contains("sleet") || l.contains("blizzard") {
        (Sky::Snow, "雪")
    } else if l.contains("rain") || l.contains("drizzle") || l.contains("shower") {
        (Sky::Rain, "雨")
    } else if l.contains("fog") || l.contains("mist") || l.contains("haze") || l.contains("smoke") {
        (Sky::Fog, "霾")
    } else if l.contains("overcast") {
        (Sky::Cloud, "阴")
    } else if l.contains("cloud") || l.contains("partly") {
        (Sky::Partly, "多云")
    } else if l.contains("clear") || l.contains("sunny") {
        (Sky::Sun, "晴")
    } else {
        (Sky::Cloud, "多云")
    };
    (sky, label.to_string())
}

/// 公历日期 -> 周几（中文）。
fn week_cn(date: &str) -> String {
    let p: Vec<i64> = date.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() != 3 {
        return String::new();
    }
    let n = days_from_civil(p[0], p[1], p[2]);
    let names = ["周四", "周五", "周六", "周日", "周一", "周二", "周三"];
    names[n.rem_euclid(7) as usize].to_string()
}

/// "06:23 AM" -> "06:23"。
fn to_24h(s: &str) -> String {
    let mut it = s.split_whitespace();
    let hm = it.next().unwrap_or("");
    let ap = it.next().unwrap_or("").to_uppercase();
    let mut p = hm.split(':');
    let (Some(h), Some(m)) = (p.next(), p.next()) else {
        return hm.to_string();
    };
    let (Ok(mut h), Ok(m)) = (h.parse::<i32>(), m.parse::<i32>()) else {
        return hm.to_string();
    };
    if ap == "PM" && h < 12 {
        h += 12;
    } else if ap == "AM" && h == 12 {
        h = 0;
    }
    format!("{:02}:{:02}", h, m)
}

/// 全球实时 + 3 天预报（wttr.in，无需 key；用于非中国地点）。
fn fetch_wttr(lat: f64, lon: f64) -> Option<Weather> {
    let url = format!("https://wttr.in/{:.4},{:.4}?format=j1&lang=zh", lat, lon);
    let text = http_get_text(&url, None, 4000, 8000, 1024 * 1024)?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let cc = v.get("current_condition")?.get(0)?;
    let temp = cc.get("temp_C").and_then(|x| x.as_str()).and_then(|s| s.parse::<f32>().ok())?;
    let humidity = format!("{}%", cc.get("humidity").and_then(|x| x.as_str()).unwrap_or(""));
    let desc = cc
        .get("weatherDesc")
        .and_then(|x| x.get(0))
        .and_then(|x| x.get("value"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let (_, cond) = sky_from_english(desc);
    let mut w = Weather {
        city: String::new(),
        city_key: format!("geo:{:.5},{:.5}", lat, lon),
        temp,
        humidity,
        quality: String::new(),
        ganmao: "境外地点使用全球预报，仅供参考".to_string(),
        updated: cc
            .get("observation_time")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        today_type: cond,
        fetched_at: now_secs(),
        ..Default::default()
    };
    if let Some(days) = v.get("weather").and_then(|x| x.as_array()) {
        for d in days.iter().take(3) {
            let date = d.get("date").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let high = d
                .get("maxtempC")
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);
            let low = d
                .get("mintempC")
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);
            let astro = d.get("astronomy").and_then(|x| x.get(0));
            let sunrise = astro
                .and_then(|x| x.get("sunrise"))
                .and_then(|x| x.as_str())
                .map(to_24h)
                .unwrap_or_default();
            let sunset = astro
                .and_then(|x| x.get("sunset"))
                .and_then(|x| x.as_str())
                .map(to_24h)
                .unwrap_or_default();
            let noon = d
                .get("hourly")
                .and_then(|x| x.as_array())
                .and_then(|a| a.get(4))
                .and_then(|x| x.get("weatherDesc"))
                .and_then(|x| x.get(0))
                .and_then(|x| x.get("value"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let (_, day_type) = sky_from_english(noon);
            w.days.push(Day {
                date: date.clone(),
                week: week_cn(&date),
                high,
                low,
                day_type,
                sunrise,
                sunset,
                ..Default::default()
            });
        }
    }
    Some(w)
}

/// 两点间球面距离（公里，haversine）。
/// 按经纬度找最近的内置城市（用于系统定位成功但区县反查失败时的兜底）。
fn nearest_builtin(lat: f64, lon: f64) -> Option<(String, String)> {
    let mut best: Option<(f64, &City)> = None;
    for c in CITIES {
        if let Some((clat, clon)) = city_coords(c.key) {
            let d = haversine(lat, lon, clat, clon);
            if best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, c));
            }
        }
    }
    best.map(|(_, c)| (c.name.to_string(), c.key.to_string()))
}

fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// Windows 定位服务（GPS / Wi-Fi）拿精确经纬度；无权限或失败返回 None。
fn windows_coords() -> Option<(f64, f64)> {
    use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator, PositionAccuracy};
    // 工作线程里调用 WinRT 需要先初始化 COM（MTA 足够）。
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_MULTITHREADED,
        );
    }
    let access = Geolocator::RequestAccessAsync().ok()?.get().ok()?;
    if access != GeolocationAccessStatus::Allowed {
        return None;
    }
    let locator = Geolocator::new().ok()?;
    let _ = locator.SetDesiredAccuracy(PositionAccuracy::High);
    let pos = locator.GetGeopositionAsync().ok()?.get().ok()?;
    let p = pos.Coordinate().ok()?.Point().ok()?.Position().ok()?;
    if p.Latitude.abs() < 0.01 && p.Longitude.abs() < 0.01 {
        return None;
    }
    Some((p.Latitude, p.Longitude))
}

/// 经纬度 -> 区县名（BigDataCloud 反向地理编码，免费无 key）。
fn reverse_district(lat: f64, lon: f64) -> Option<String> {
    let url = format!(
        "https://api.bigdatacloud.net/data/reverse-geocode-client?latitude={:.6}&longitude={:.6}&localityLanguage=zh",
        lat, lon
    );
    let text = http_get_text(&url, None, 3000, 6000, 256 * 1024)?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let city = v.get("city").and_then(|x| x.as_str()).unwrap_or("").trim();
    let locality = v.get("locality").and_then(|x| x.as_str()).unwrap_or("").trim();
    // locality 在上海等直辖市就是区名；若与 city 相同则再从 administrative 里取最细一级
    if !locality.is_empty() && locality != city {
        return Some(locality.to_string());
    }
    if let Some(arr) = v
        .get("localityInfo")
        .and_then(|x| x.get("administrative"))
        .and_then(|x| x.as_array())
    {
        let mut best: Option<(i64, String)> = None;
        for it in arr {
            let name = it.get("name").and_then(|x| x.as_str()).unwrap_or("").trim();
            let level = it.get("adminLevel").and_then(|x| x.as_i64()).unwrap_or(0);
            if name.is_empty() {
                continue;
            }
            if best.as_ref().map_or(true, |(l, _)| level >= *l) {
                best = Some((level, name.to_string()));
            }
        }
        if let Some((_, name)) = best {
            if name != city {
                return Some(name);
            }
        }
    }
    None
}

/// 在搜索结果里挑出最贴近目标区县名的命中（优先同名 / 去后缀同名）。
fn pick_district_hit(q: &str, hits: &[CityHit]) -> Option<CityHit> {
    let base = q.trim_end_matches(|c| c == '市' || c == '区' || c == '县');
    hits.iter()
        .find(|h| h.name == q)
        .or_else(|| hits.iter().find(|h| h.name == base))
        .or_else(|| hits.iter().find(|h| h.name.contains(base) && !base.is_empty()))
        .cloned()
}

/// 自动定位：优先 Windows 系统定位（GPS/Wi-Fi，可到区县），
/// 反查区县名后匹配中国天气网区级代码；失败再用 ip-api 城市级定位兜底。
pub fn locate() -> Option<(String, String)> {
    if let Some((lat, lon)) = windows_coords() {
        if let Some(district) = reverse_district(lat, lon) {
            let hits = search_china(&district);
            if let Some(hit) = pick_district_hit(&district, &hits) {
                if let Ok(mut cache) = coord_cache().lock() {
                    cache.insert(hit.key.clone(), (lat, lon));
                }
                return Some((hit.name, hit.key));
            }
        }
        // 系统定位已经拿到经纬度时，绝不回退到 IP 定位（IP 可能相差几百公里）。
        // 区县反查失败就按经纬度落到最近的内置城市，精度仍远高于 IP。
        if let Some(hit) = nearest_builtin(lat, lon) {
            return Some(hit);
        }
        return Some(("当前位置".to_string(), format!("geo:{:.5},{:.5}", lat, lon)));
    }

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(5))
        .build();
    let resp = agent
        .get("http://ip-api.com/json/?lang=zh-CN&fields=status,city,regionName,lat,lon")
        .set("User-Agent", "WinIsle-Weather")
        .call()
        .ok()?;
    let mut s = String::new();
    resp.into_reader().take(64 * 1024).read_to_string(&mut s).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    if v.get("status").and_then(|x| x.as_str()) != Some("success") {
        return None;
    }
    let city = v.get("city").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let region = v.get("regionName").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let lat = v.get("lat").and_then(|x| x.as_f64());
    let lon = v.get("lon").and_then(|x| x.as_f64());

    // 1) 中国天气网城市搜索：市 / 区县都能命中
    for q in [city.as_str(), region.as_str()] {
        if q.is_empty() {
            continue;
        }
        if let Some(hit) = search_china(q).into_iter().next() {
            return Some((hit.name, hit.key));
        }
    }
    // 2) 全球地点：经纬度直接生成坐标 key（随后走全球实时 + 7timer 预报）
    if let (Some(la), Some(lo)) = (lat, lon) {
        let name = if city.is_empty() { "当前位置".to_string() } else { city };
        return Some((name, format!("geo:{:.5},{:.5}", la, lo)));
    }
    // 3) 兜底：离经纬度最近的内置城市
    if let (Some(la), Some(lo)) = (lat, lon) {
        if let Some(hit) = nearest_builtin(la, lo) {
            return Some(hit);
        }
    }
    None
}

/// 7timer 的 weather 字符串 -> (图标种类, 中文简述)。
fn sky_from_7timer(s: &str) -> (Sky, String) {
    let l = s.to_lowercase();
    let night = l.contains("night");
    let sky = if l.contains("thunder") {
        Sky::Thunder
    } else if l.contains("snow") {
        Sky::Snow
    } else if l.contains("rain") || l.contains("shower") {
        Sky::Rain
    } else if l.contains("fog") {
        Sky::Fog
    } else if l.contains("partly") {
        if night { Sky::Night } else { Sky::Partly }
    } else if l.contains("cloudy") {
        Sky::Cloud
    } else if l.contains("clear") {
        if night { Sky::Night } else { Sky::Sun }
    } else {
        Sky::Cloud
    };
    let label = if l.contains("thunder") {
        "雷"
    } else if l.contains("snow") {
        "雪"
    } else if l.contains("rain") || l.contains("shower") {
        "雨"
    } else if l.contains("fog") {
        "雾"
    } else if l.contains("partly") {
        "多云"
    } else if l.contains("cloudy") {
        "阴"
    } else {
        "晴"
    };
    (sky, label.to_string())
}

/// 公历 -> 自 1970-01-01 起的天数（Howard Hinnant 算法）。
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// 解析 7timer civil 返回，取未来 24 小时（最多 8 个 3 小时点）。
fn parse_7timer(root: &serde_json::Value) -> Vec<Hour> {
    let init = root.get("init").and_then(|x| x.as_str()).unwrap_or("");
    if init.len() != 10 || !init.bytes().all(|b| b.is_ascii_digit()) {
        return Vec::new();
    }
    let y: i64 = init[0..4].parse().unwrap_or(1970);
    let m: i64 = init[4..6].parse().unwrap_or(1);
    let d: i64 = init[6..8].parse().unwrap_or(1);
    let hh: i64 = init[8..10].parse().unwrap_or(0);
    let init_epoch = days_from_civil(y, m, d) * 86400 + hh * 3600; // UTC 秒
    let now = now_secs() as i64;
    let mut out = Vec::new();
    if let Some(arr) = root.get("dataseries").and_then(|x| x.as_array()) {
        for it in arr {
            let tp = it.get("timepoint").and_then(|x| x.as_i64()).unwrap_or(0);
            let ep = init_epoch + tp * 3600;
            if ep < now - 3600 {
                continue;
            }
            let temp = it.get("temp2m").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
            let wstr = it.get("weather").and_then(|x| x.as_str()).unwrap_or("");
            let (sky, _) = sky_from_7timer(wstr);
            // 转为东八区本地时钟（仅取小时，跨日不影响显示）
            let hod = ((ep + 8 * 3600).rem_euclid(86400)) / 3600;
            out.push(Hour { label: format!("{:02}:00", hod), temp, sky });
            if out.len() >= 8 {
                break;
            }
        }
    }
    out
}

/// 解析中国天气网「1 天」页内嵌的 `var hour3data={...}`，取未来 24 小时（3 小时粒度）。
/// 数据形如 `19日11时,d02,阴,10℃,西北风,<3级,3`，国内官方、无需 key。
fn parse_hour3data(text: &str) -> Vec<Hour> {
    let Some(i) = text.find("var hour3data=") else {
        return Vec::new();
    };
    let rest = &text[i + "var hour3data=".len()..];
    let Some(j) = rest.find("</script>") else {
        return Vec::new();
    };
    let json = rest[..j].trim().trim_end_matches(';').trim();
    let Ok(root) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(arr) = root.get("1d").and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for it in arr {
        let Some(s) = it.as_str() else { continue };
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() < 4 {
            continue;
        }
        let time = parts[0].trim(); // "19日11时"
        let code = parts[1].trim(); // d02 / n01
        let desc = parts[2].trim(); // 阴 / 多云 / 晴
        let temp = parse_temp_text(parts[3]).unwrap_or(0);
        let hour: u32 = time
            .find('时')
            .map(|p| &time[..p])
            .and_then(|x| x.rsplit('日').next())
            .and_then(|h| h.trim().parse().ok())
            .unwrap_or(0);
        let is_night = code.starts_with('n') || hour >= 18 || hour < 6;
        out.push(Hour {
            label: format!("{:02}:00", hour),
            temp,
            sky: sky_of(desc, is_night),
        });
        if out.len() >= 8 {
            break;
        }
    }
    out
}

/// 拉取中国天气网 1 天页的逐 3 小时预报（仅 9 位国内城市代码可用）。
fn fetch_hours_cn(city_key: &str) -> Vec<Hour> {
    if city_key.len() != 9 || !city_key.bytes().all(|b| b.is_ascii_digit()) {
        return Vec::new();
    }
    let url = format!("http://www.weather.com.cn/weather1d/{}.shtml", city_key);
    let Some(text) = http_get_text(
        &url,
        Some("http://www.weather.com.cn/"),
        3000,
        6000,
        300 * 1024,
    ) else {
        return Vec::new();
    };
    parse_hour3data(&text)
}

/// 拉取未来 24 小时（3h）预报；优先国内中国天气网，失败回退 7timer，任何失败都静默返回空。
fn fetch_hours(city_key: &str) -> Vec<Hour> {
    let cn = fetch_hours_cn(city_key);
    if !cn.is_empty() {
        return cn;
    }
    let Some((lat, lon)) = city_coords(city_key) else {
        return Vec::new();
    };
    fetch_hours_at(lat, lon)
}

fn fetch_hours_at(lat: f64, lon: f64) -> Vec<Hour> {
    let url = format!(
        "https://www.7timer.info/bin/api.pl?lon={:.2}&lat={:.2}&product=civil&output=json",
        lon, lat
    );
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(4))
        .build();
    let resp = match agent.get(&url).set("User-Agent", "WinIsle-Weather").call() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut s = String::new();
    if resp.into_reader().take(256 * 1024).read_to_string(&mut s).is_err() {
        return Vec::new();
    }
    match serde_json::from_str::<serde_json::Value>(&s) {
        Ok(v) => parse_7timer(&v),
        Err(_) => Vec::new(),
    }
}

fn cache_path() -> std::path::PathBuf {
    crate::paths::file("weather_cache.json")
}
/// 缓存成功拉取的原始 JSON（附带城市 key 与拉取时刻），供断网/失败时兜底。
fn save_cache(city_key: &str, root: &serde_json::Value) {
    let pack = serde_json::json!({
        "city_key": city_key,
        "fetched_at": now_secs(),
        "raw": root,
    });
    let _ = std::fs::write(cache_path(), pack.to_string());
}

/// 读取缓存。`city_key` 不匹配则视为无缓存（避免展示别的城市旧数据）；
/// 空 key 表示自动定位模式，接受任意城市的最近缓存作为离线兜底。
pub fn load_cache(city_key: &str) -> Option<Weather> {
    let text = std::fs::read_to_string(cache_path()).ok()?;
    let pack: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let ck = pack.get("city_key").and_then(|x| x.as_str()).unwrap_or("");
    if !city_key.is_empty() && ck != city_key {
        return None;
    }
    let raw = pack.get("raw")?;
    let mut w = parse_itboy(raw)?;
    w.fetched_at = pack.get("fetched_at").and_then(|x| x.as_u64()).unwrap_or_else(now_secs);
    w.from_cache = true;
    Some(w)
}

/// 线上拉取（阻塞，调用方应放到后台线程）。中国城市成功后写入缓存。
pub fn fetch(city_key: &str) -> Result<Weather, String> {
    if let Some(rest) = city_key.strip_prefix("geo:") {
        let mut it = rest.split(',');
        let lat = it
            .next()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .ok_or_else(|| "坐标无效".to_string())?;
        let lon = it
            .next()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .ok_or_else(|| "坐标无效".to_string())?;
        let mut w = fetch_wttr(lat, lon).ok_or_else(|| "全球天气拉取失败".to_string())?;
        w.hours = fetch_hours_at(lat, lon);
        return Ok(w);
    }
    let url = format!("http://t.weather.itboy.net/api/weather/city/{}", city_key);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(6))
        .build();
    let resp = agent
        .get(&url)
        .set("User-Agent", "WinIsle-Weather")
        .call()
        .map_err(|e| format!("请求失败：{e}"))?;
    let mut s = String::new();
    resp.into_reader()
        .take(512 * 1024)
        .read_to_string(&mut s)
        .map_err(|e| format!("读取失败：{e}"))?;
    let root: serde_json::Value =
        serde_json::from_str(s.trim_start_matches('\u{feff}')).map_err(|e| format!("解析失败：{e}"))?;
    let mut w = parse_itboy(&root).ok_or_else(|| "数据格式异常".to_string())?;
    // 实时层用中国天气网官方观测覆盖 itboy 的滞后实时值（温度 / 湿度 / AQI / 天气现象 / 风）
    if let Some(rt) = fetch_realtime(city_key) {
        w.temp = rt.temp;
        if !rt.humidity.is_empty() {
            w.humidity = rt.humidity;
        }
        if rt.pm25 > 0.0 {
            w.pm25 = rt.pm25;
        }
        if !rt.quality.is_empty() {
            w.quality = rt.quality;
        }
        if !rt.weather.is_empty() {
            w.today_type = rt.weather;
        }
        if !rt.updated.is_empty() {
            w.updated = rt.updated;
        }
        if let Some(d) = w.days.first_mut() {
            if !rt.wind_dir.is_empty() {
                d.wind_dir = rt.wind_dir;
            }
            if !rt.wind_level.is_empty() {
                d.wind_level = rt.wind_level;
            }
        }
    }
    save_cache(city_key, &root);
    w.hours = fetch_hours(city_key);
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "message":"success","status":200,"date":"20261118","time":"2026-11-18 10:56:27",
      "cityInfo":{"city":"北京市","citykey":"101010100","parent":"北京","updateTime":"07:32"},
      "data":{"shidu":"29%","pm25":3.0,"pm10":6.0,"quality":"优","wendu":"-2.8","ganmao":"各类人群可自由活动",
        "forecast":[
          {"date":"18","high":"高温 9℃","low":"低温 1℃","ymd":"2026-11-18","week":"星期三","sunrise":"07:02","sunset":"16:56","aqi":29,"fx":"西北风","fl":"2级","type":"晴","notice":"x"},
          {"date":"19","high":"高温 8℃","low":"低温 -1℃","ymd":"2026-11-19","week":"星期四","sunrise":"07:03","sunset":"16:55","aqi":27,"fx":"西北风","fl":"2级","type":"多云","notice":"y"}
        ],
        "yesterday":{}
      }
    }"#;

    #[test]
    fn parses_itboy() {
        let v: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        let w = parse_itboy(&v).expect("should parse");
        assert_eq!(w.city, "北京市");
        assert_eq!(w.city_key, "101010100");
        assert!((w.temp - (-2.8)).abs() < 0.01);
        assert_eq!(w.quality, "优");
        assert_eq!(w.days.len(), 2);
        assert_eq!(w.days[0].high, 9);
        assert_eq!(w.days[0].low, 1);
        assert_eq!(w.days[1].low, -1);
        assert_eq!(w.days[0].aqi, 29);
        assert_eq!(w.today_type, "晴");
    }

    #[test]
    fn temp_text_parsing() {
        assert_eq!(parse_temp_text("高温 9℃"), Some(9));
        assert_eq!(parse_temp_text("低温 -1℃"), Some(-1));
        assert_eq!(parse_temp_text("nope"), None);
    }

    #[test]
    fn sky_mapping() {
        assert_eq!(sky_of("晴", false), Sky::Sun);
        assert_eq!(sky_of("晴", true), Sky::Night);
        assert_eq!(sky_of("多云", false), Sky::Partly);
        assert_eq!(sky_of("雷阵雨", false), Sky::Thunder);
        assert_eq!(sky_of("小雪", false), Sky::Snow);
        assert_eq!(sky_of("霾", false), Sky::Fog);
        assert_eq!(sky_of("阴", false), Sky::Cloud);
    }

    #[test]
    fn hour3data_parsing() {
        let sample = "<script>var hour3data={\"1d\":[\"19日11时,d02,阴,10℃,西北风,<3级,3\",\"19日14时,d02,阴,11℃,西北风,<3级,3\",\"19日20时,n02,阴,9℃,西风,<3级,0\",\"20日02时,n01,多云,7℃,西北风,<3级,0\",\"20日08时,d01,多云,6℃,西风,<3级,2\"]};</script>";
        let h = parse_hour3data(sample);
        assert_eq!(h.len(), 5);
        assert_eq!(h[0].label, "11:00");
        assert_eq!(h[0].temp, 10);
        assert_eq!(h[0].sky, Sky::Cloud);
        assert_eq!(h[2].sky, Sky::Cloud); // 夜间阴
        assert_eq!(h[3].label, "02:00");
        assert_eq!(h[4].sky, Sky::Partly);
    }

    #[test]
    fn seven_timer_mapping() {
        assert_eq!(sky_from_7timer("clearday").0, Sky::Sun);
        assert_eq!(sky_from_7timer("clearnight").0, Sky::Night);
        assert_eq!(sky_from_7timer("partlycloudyday").0, Sky::Partly);
        assert_eq!(sky_from_7timer("cloudy").0, Sky::Cloud);
        assert_eq!(sky_from_7timer("lightrain").0, Sky::Rain);
        assert_eq!(sky_from_7timer("lightsnow").0, Sky::Snow);
        assert_eq!(sky_from_7timer("thunderstorm").0, Sky::Thunder);
        assert_eq!(sky_from_7timer("fog").0, Sky::Fog);
    }

    #[test]
    fn aqi_quality_mapping() {
        assert_eq!(quality_of_aqi(30), "优");
        assert_eq!(quality_of_aqi(80), "良");
        assert_eq!(quality_of_aqi(120), "轻度污染");
        assert_eq!(quality_of_aqi(280), "重度污染");
    }

    /// 联网手工核查（默认忽略）：cargo test --release weather::tests::live_checks -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_checks() {
        let hits = search_cities("杭州");
        eprintln!("search 杭州 -> {:?}", hits.iter().take(3).map(|h| (&h.name, &h.key, &h.province)).collect::<Vec<_>>());
        assert!(!hits.is_empty(), "city search should return results");
        let w = fetch("101020100").expect("live fetch should succeed");
        eprintln!(
            "live Shanghai: city={} temp={} quality={} pm25={} updated={} days={} hours={} first_hours={:?}",
            w.city, w.temp, w.quality, w.pm25, w.updated, w.days.len(), w.hours.len(),
            w.hours.iter().take(3).map(|h| (&h.label, h.temp)).collect::<Vec<_>>()
        );
        assert!(w.temp > -60.0 && w.temp < 60.0);
    }

    /// 联网核查全球地点（默认忽略）。
    #[test]
    #[ignore]
    fn live_geo() {
        let w = fetch("geo:35.68950,139.69170").expect("geo fetch should succeed");
        eprintln!(
            "live Tokyo: temp={} humid={} today={} days={} hours={} first_day={:?}",
            w.temp,
            w.humidity,
            w.today_type,
            w.days.len(),
            w.hours.len(),
            w.days.first().map(|d| (&d.date, d.high, d.low, &d.day_type, &d.week))
        );
        assert!(!w.days.is_empty(), "geo daily forecast should not be empty");
    }

    /// 联网核查自动定位（默认忽略）。
    #[test]
    #[ignore]
    fn live_locate() {
        eprintln!("locate -> {:?}", locate());
    }
}

//! 歌词获取（网易云音乐公开接口，按 歌名+歌手 搜索取 LRC 并解析成时间轴），
//! 附带专辑封面下载。网络/查询均在后台线程执行，带磁盘缓存与重试，避免重复请求被限流(405)。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone)]
pub struct Seg {
    pub t: f64,   // 该字/词开始（秒）
    pub dur: f64, // 该字/词持续（秒）
    pub n: u32,   // 字数（不含空格）
}

#[derive(Clone)]
pub struct LrcLine {
    pub t: f64, // 秒（已按 [offset] 校正）
    pub text: String,
    pub segs: Vec<Seg>, // 逐字时间（来自 yrc）；为空表示只有整行时间
}

/// 纯音乐占位文案：歌词源只给出“纯音乐，请欣赏”这类“整首无人声歌词”的标记时，
/// 用它作为展示歌词（收起态、展开态都显示这一句，而不是“暂无歌词”）。
pub const INSTRUMENTAL_TEXT: &str = "纯音乐，请欣赏";

/// 严格判定「单行文本」是否为纯音乐占位标记。
///
/// 只有「整句本身就是占位短语」才判为纯音乐：包含“纯音乐”且整句很短（≤24 字），
/// 或整句核心字面就是“请欣赏 / 谢谢欣赏”等固定短语。这样正常歌词里偶然出现
/// “请欣赏”几个字（如“我请你欣赏这满山花开”）不会被误判成纯音乐。
pub fn is_instrumental_marker(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    // 归一化：去掉空白与常见中英文标点，得到“核心字面”再做严格比较。
    let core: String = t
        .chars()
        .filter(|c| {
            !c.is_whitespace()
                && !c.is_ascii_punctuation()
                && !"、。，！？；：…—－~～·“”‘’「」『』（）《》〈〉【】".contains(*c)
        })
        .collect();
    if core.is_empty() {
        return false;
    }
    if core.contains("纯音乐") && core.chars().count() <= 24 {
        return true;
    }
    matches!(
        core.as_str(),
        "请欣赏" | "请您欣赏" | "谢谢欣赏" | "感谢欣赏" | "敬请欣赏" | "请欣赏谢谢"
    )
}

/// 去掉 LRC 的 [时间] 标签与 YRC 的 (起,时,0) 逐字时间括号，只保留歌词正文。
fn strip_time_tags(line: &str) -> String {
    let mut out = String::new();
    let mut sq = 0i32;
    let mut par = 0i32;
    for ch in line.chars() {
        match ch {
            '[' => sq += 1,
            ']' => sq = (sq - 1).max(0),
            '(' => par += 1,
            ')' => par = (par - 1).max(0),
            _ => {
                if sq == 0 && par == 0 {
                    out.push(ch);
                }
            }
        }
    }
    out
}

/// 判断一段歌词原文是否为「纯音乐 / 请欣赏」占位。
///
/// 逐行严格判定（先剥掉时间标签）：所有非空行都必须本身是占位短语才算纯音乐；
/// 正常歌词（含多行）绝不会因为某一句里出现“请欣赏”而被误判。同时兼容带时间戳
/// 与不带时间戳两种返回格式（`[00:00.00]纯音乐，请欣赏` / `纯音乐，请欣赏`）。
pub fn looks_instrumental(raw: &str) -> bool {
    let mut any = false;
    for line in raw.lines() {
        let text = strip_time_tags(line);
        let t = text.trim();
        if t.is_empty() {
            continue;
        }
        any = true;
        if !is_instrumental_marker(t) {
            return false;
        }
    }
    any
}

/// 构造「纯音乐」占位歌词（单行、时间 0，便于统一走现有的歌词展示逻辑）。
pub fn instrumental_line() -> Vec<LrcLine> {
    vec![LrcLine {
        t: 0.0,
        text: INSTRUMENTAL_TEXT.to_string(),
        segs: Vec::new(),
    }]
}

/// 判断一组歌词是否为「纯音乐」占位（全部行都是纯音乐标记）。
pub fn is_instrumental(lines: &[LrcLine]) -> bool {
    !lines.is_empty() && lines.iter().all(|l| is_instrumental_marker(&l.text))
}

/// 判断一段文本是否为汽水的“暂无歌词”占位（如 `暂无歌词，请欣赏`）。
/// 这表示汽水也**没有**这首歌的歌词（缺词），但它不是纯音乐——用来区分“纯音乐”与“暂无歌词”，
/// 从而对“暂无歌词”直接下结论、停止无谓轮询。
pub fn is_no_lyric_marker(text: &str) -> bool {
    let core: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    core.contains("暂无歌词")
}

/// 汽水「轻量」歌词探测的三态结果（见 `soda_probe_cdp`）。
pub enum SodaProbeResult {
    /// 拿到了可用歌词：可能是真实歌词，也可能是「纯音乐，请欣赏」占位。
    Got(Vec<LrcLine>),
    /// 汽水明确给出“暂无歌词（非纯音乐）”——已可下结论，无需再轮询。
    NoLyric,
    /// 此刻还读不到，可稍后再试。
    Unknown,
}

/// 汽水专用「轻量」歌词探测：只通过 CDP 读当前播放页的 LyricsView / .no-lyric 文本，
/// 不做任何内存扫描、不读写磁盘缓存。用于「切歌后歌词页比歌词晚到」时的高频补取：
/// 汽水歌词页可能几秒后才渲染出来，甚至要等用户点开播放页才出现「纯音乐，请欣赏」，
/// 用这个廉价探测每隔几秒补一次，既不烧 CPU，又能及时把纯音乐/迟到歌词补上。
///
/// 返回三态（见 `SodaProbeResult`）：`Got(instrumental_line())` 表示当前曲目确为纯音乐；
/// `Got(真实歌词)` 表示歌词迟到但已可读；`NoLyric` 表示汽水明确“暂无歌词”（非纯音乐）；
/// `Unknown` 表示此刻还读不到（调用方稍后再试）。
pub fn soda_probe_cdp(title: &str, artist: &str) -> SodaProbeResult {
    match crate::cdp_lyric::probe_current_no_nav(title, artist) {
        crate::cdp_lyric::SodaProbe::Got(sl) => {
            if looks_instrumental(&sl.content) {
                SodaProbeResult::Got(instrumental_line())
            } else {
                let l = crate::cdp_lyric::parse_content(&sl.content);
                if l.is_empty() {
                    SodaProbeResult::Unknown
                } else {
                    SodaProbeResult::Got(l)
                }
            }
        }
        crate::cdp_lyric::SodaProbe::NoLyric => SodaProbeResult::NoLyric,
        crate::cdp_lyric::SodaProbe::Unknown => SodaProbeResult::Unknown,
    }
}

fn dlog(s: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(crate::paths::file("lyrics_debug.log"))
    {
        let _ = writeln!(f, "{}", s);
    }
}

fn cache_dir() -> Option<PathBuf> {
    let d = crate::paths::ensure().join("cache");
    let _ = std::fs::create_dir_all(&d);
    Some(d)
}

fn key_hash(title: &str, artist: &str, dur_sec: f64) -> String {
    let mut h = DefaultHasher::new();
    title.hash(&mut h);
    artist.hash(&mut h);
    // 时长分桶（5 秒一档）：不同版本（录音室/现场）时长不同 → 歌词/时间轴也不同，必须分开缓存
    (((dur_sec / 5.0).round()) as i64).hash(&mut h);
    format!("{:016x}", h.finish())
}

fn pct_encode(s: &str) -> String {
    let mut o = String::new();
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{:02X}", b));
        }
    }
    o
}

fn get_json_with(url: &str, referer: &str) -> Option<serde_json::Value> {
    for attempt in 0..3 {
        let r = ureq::get(url)
            .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .set("Referer", referer)
            .timeout(Duration::from_secs(8))
            .call();
        match r {
            Ok(resp) => return resp.into_json::<serde_json::Value>().ok(),
            Err(e) => {
                dlog(&format!("GET err attempt{} {} -> {}", attempt, url, e));
                std::thread::sleep(Duration::from_millis(400));
            }
        }
    }
    None
}

fn get_json(url: &str) -> Option<serde_json::Value> {
    get_json_with(url, "https://music.163.com/")
}

/// 快速请求：LRCLIB 在部分网络下握手/响应要 5–10 秒，不能让它拖慢整首歌的取词。
/// 只发一次，最多等 2.5 秒；失败由调用方做 10 分钟冷却。
fn get_json_fast(url: &str, referer: &str) -> Option<serde_json::Value> {
    let r = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .set("Referer", referer)
        .timeout(Duration::from_millis(2500))
        .call();
    match r {
        Ok(resp) => resp.into_json::<serde_json::Value>().ok(),
        Err(e) => {
            dlog(&format!("GET fast err {} -> {}", url, e));
            None
        }
    }
}

/// LRCLIB 连续失败时临时跳过，避免每首新歌都白等 3 秒。
static LRCLIB_DOWN_UNTIL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

fn lrclib_available() -> bool {
    let g = LRCLIB_DOWN_UNTIL.lock().unwrap_or_else(|e| e.into_inner());
    match *g {
        Some(t) => t.elapsed() >= Duration::from_secs(600),
        None => true,
    }
}

fn lrclib_mark_down() {
    let mut g = LRCLIB_DOWN_UNTIL.lock().unwrap_or_else(|e| e.into_inner());
    *g = Some(std::time::Instant::now());
}

fn get_bytes(url: &str) -> Option<Vec<u8>> {
    for attempt in 0..3 {
        let r = ureq::get(url)
            .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .set("Referer", "https://music.163.com/")
            .timeout(Duration::from_secs(10))
            .call();
        match r {
            Ok(resp) => {
                let mut buf = Vec::new();
                if resp.into_reader().read_to_end(&mut buf).is_ok() {
                    return Some(buf);
                }
            }
            Err(e) => {
                dlog(&format!("GETBYTES err attempt{} -> {}", attempt, e));
                std::thread::sleep(Duration::from_millis(400));
            }
        }
    }
    None
}

use std::io::Read;

fn parse_ts(s: &str) -> Option<f64> {
    let mut it = s.split(':');
    let m: f64 = it.next()?.trim().parse().ok()?;
    let sec: f64 = it.next()?.trim().parse().ok()?;
    Some(m * 60.0 + sec)
}

/// 开头/结尾的制作署名行（词/曲/编曲/制作人/混音/发行/营销…）。
/// 这些行在 LRC 里常被压在同一两秒内，播放时会让歌词像“一刷到底”，
/// 因此默认不参与逐字滚动；只用“带冒号的标签行”判定，避免误伤
/// “词不达意的话”这类正常歌词。
fn is_credit_line(text: &str) -> bool {
    let s = text.trim();
    let head = match s.split_once('：').or_else(|| s.split_once(':')) {
        Some((h, _)) => h.trim(),
        None => return false,
    };
    if head.is_empty() || head.chars().count() > 48 {
        return false;
    }
    const KEYS: &[&str] = &[
        "作词", "词", "作曲", "曲", "编曲", "制作人", "监制", "混音", "录音", "母带",
        "吉他", "贝斯", "鼓", "键盘", "和声", "合声", "配唱", "人声", "弦乐", "出品",
        "发行", "OP", "SP", "营销", "统筹", "协力", "推广", "企划", "策划", "封面",
        "设计", "制作助理", "助理", "Lyrics", "Composed", "Arranged", "Producer",
        "Music", "Mixing", "Recording", "Mastering", "Publishing", "Written",
    ];
    let lower = head.to_ascii_lowercase();
    KEYS.iter().any(|k| {
        head.starts_with(k) || lower.starts_with(&k.to_ascii_lowercase())
    })
}

fn is_noise(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    if t.contains("纯音乐") || t.contains("此歌曲为没有填词的纯音乐") {
        return true;
    }
    if t.contains("未经著作权人") || (t.contains("未经许可") && t.contains("翻唱")) {
        return true;
    }
    if is_credit_line(t) {
        return true;
    }
    false
}

fn filt(v: Vec<LrcLine>) -> Vec<LrcLine> {
    v.into_iter().filter(|x| !is_noise(&x.text)).collect()
}

/// 解析 LRC；支持一行多时间标签，并处理 [offset:±N]（毫秒）。
/// 网易云约定：offset 正值表示歌词整体提前，因此校正为 t - offset/1000。
pub fn parse_lrc(s: &str) -> Vec<LrcLine> {
    let mut out: Vec<LrcLine> = Vec::new();
    let mut offset_ms: f64 = 0.0;
    for raw in s.lines() {
        let mut rest = raw.trim();
        let mut times: Vec<f64> = Vec::new();
        while let Some(strip) = rest.strip_prefix('[') {
            if let Some(end) = strip.find(']') {
                let inner = &strip[..end];
                if let Some(off) = inner.strip_prefix("offset:") {
                    if let Ok(v) = off.trim().parse::<f64>() {
                        offset_ms = v;
                    }
                } else if let Some(t) = parse_ts(inner) {
                    times.push(t);
                }
                rest = &strip[end + 1..];
            } else {
                break;
            }
        }
        let text = rest.trim();
        if times.is_empty() || text.is_empty() {
            continue;
        }
        for t in times {
            out.push(LrcLine { t, text: text.to_string(), segs: Vec::new() });
        }
    }
    let off = offset_ms / 1000.0;
    if off != 0.0 {
        for l in out.iter_mut() {
            l.t -= off;
        }
    }
    out.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// 解析网易云逐字歌词 yrc。格式：
/// `[行开始ms,行时长ms](字开始ms,字时长ms,0)字(字开始ms,字时长ms,0)字…`
/// 时间是相对整首歌的毫秒数。返回带 segs 的 LrcLine。
pub fn parse_yrc(s: &str) -> Vec<LrcLine> {
    let mut out: Vec<LrcLine> = Vec::new();
    for raw in s.lines() {
        let line = raw.trim();
        if !line.starts_with('[') {
            continue;
        }
        let close = match line.find(']') {
            Some(i) => i,
            None => continue,
        };
        let head = &line[1..close];
        let mut hp = head.split(',');
        let start_ms: f64 = match hp.next().and_then(|x| x.trim().parse().ok()) {
            Some(v) => v,
            None => continue,
        };
        let mut rest = &line[close + 1..];
        let mut segs: Vec<Seg> = Vec::new();
        let mut text = String::new();
        while rest.starts_with('(') {
            let end = match rest.find(')') {
                Some(i) => i,
                None => break,
            };
            let inner = &rest[1..end];
            let mut ip = inner.split(',');
            let ws: f64 = ip.next().and_then(|x| x.trim().parse().ok()).unwrap_or(start_ms);
            let wd: f64 = ip.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0.0);
            rest = &rest[end + 1..];
            let next_paren = rest.find('(').unwrap_or(rest.len());
            let wtext = rest[..next_paren].trim_end_matches(['\r', '\n']);
            let n = wtext.chars().filter(|c| !c.is_whitespace()).count() as u32;
            text.push_str(wtext);
            if n > 0 {
                segs.push(Seg { t: ws / 1000.0, dur: wd / 1000.0, n });
            }
            rest = &rest[next_paren..];
        }
        let text = text.trim().to_string();
        if segs.is_empty() || text.is_empty() {
            continue;
        }
        out.push(LrcLine { t: start_ms / 1000.0, text, segs });
    }
    out.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn clean_title(t: &str) -> String {
    // 归一化各种空白（含全角空格、不换行空格、零宽字符），避免窗口标题里的特殊空格导致搜索失配
    let norm: String = t
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '\u{3000}' || c == '\u{00A0}' || c == '\u{200B}' || c == '\u{FEFF}' {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut s = norm.split_whitespace().collect::<Vec<_>>().join(" ");
    for (op, cl) in [('(', ')'), ('（', '）'), ('[', ']'), ('【', '】')] {
        if let (Some(a), Some(b)) = (s.find(op), s.rfind(cl)) {
            if b > a {
                s = format!("{}{}", &s[..a], &s[b + cl.len_utf8()..]);
            }
        }
    }
    if let Some(i) = s.find(" - ") {
        s = s[..i].to_string();
    }
    s.trim().to_string()
}

/// 返回按匹配度降序的 (score, 歌曲 id, 时长秒) 候选列表（同名多版本极常见，需要逐个试并用时长区分版本）。
fn find_song_candidates(title: &str, artist: &str) -> Vec<(i32, i64, f64)> {
    let ctitle = clean_title(title);
    let q = format!("{} {}", ctitle, artist);
    let q = q.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let surl = format!(
        "https://music.163.com/api/search/get/web?s={}&type=1&limit=30&offset=0",
        pct_encode(q)
    );
    let sj = match get_json(&surl) {
        Some(x) => x,
        None => return Vec::new(),
    };
    let songs = match sj.get("result").and_then(|v| v.get("songs")).and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut scored: Vec<(i32, i64, f64)> = Vec::new();
    for s in songs {
        let id = match s.get("id").and_then(|v| v.as_i64()) {
            Some(x) => x,
            None => continue,
        };
        let dur_ms = s.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let dur_s = dur_ms / 1000.0;
        let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("");
        // 归一化比较：只保留字母数字（含中文），忽略空格/标点/大小写；
        // 歌名必须“完全相同”才算命中（候选带 (Live)/(cover) 等后缀会被排除）。
        let norm = |x: &str| -> String {
            x.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect::<String>()
        };
        let nt = norm(&ctitle);
        let nm = norm(name);
        let na = norm(artist);
        let mut sc = 0i32;
        if !nt.is_empty() && nm == nt {
            sc += 10; // 歌名完全一致
        } else if !nt.is_empty() && (nm.contains(&nt) || nt.contains(&nm)) {
            sc += 4; // 仅包含（可能是 cover/remix，低权重）
        }
        if let Some(ars) = s.get("artists").and_then(|v| v.as_array()) {
            let artist_ok = ars.iter().any(|a| {
                a.get("name").and_then(|v| v.as_str()).map(|n| {
                    let nn = norm(n);
                    !na.is_empty() && !nn.is_empty() && (nn == na || nn.contains(&na) || na.contains(&nn))
                }).unwrap_or(false)
            });
            if artist_ok {
                sc += 6; // 歌手一致
            }
        }
        scored.push((sc, id, dur_s));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
}

/// 归一化：只保留字母数字（含中文），忽略空格/标点/大小写；用于歌名/歌手比较。
fn norm_alpha(x: &str) -> String {
    x.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect::<String>()
}

// ===== 备用歌词源 1：QQ 音乐（smartbox 搜索 mid → 歌词接口），中文/抖音热歌覆盖好 =====
fn qq_search_mid(title: &str, artist: &str) -> Option<String> {
    let q = format!("{} {}", clean_title(title), artist.trim());
    let q = q.trim();
    if q.is_empty() {
        return None;
    }
    let url = format!(
        "https://c.y.qq.com/splcloud/fcgi-bin/smartbox_new.fcg?key={}&format=json&g_tk=5381",
        pct_encode(q)
    );
    let v = get_json_with(&url, "https://y.qq.com/")?;
    let items = v
        .get("data")?
        .get("song")?
        .get("itemlist")?
        .as_array()?;
    let nt = norm_alpha(&clean_title(title));
    let na = norm_alpha(artist);
    let mut best: Option<(i32, String)> = None;
    for it in items {
        let name = it.get("name").and_then(|x| x.as_str()).unwrap_or("");
        let singer = it.get("singer").and_then(|x| x.as_str()).unwrap_or("");
        let mid = it.get("mid").and_then(|x| x.as_str()).unwrap_or("");
        if mid.is_empty() {
            continue;
        }
        let nm = norm_alpha(name);
        let mut sc = 0i32;
        if !nt.is_empty() && nm == nt {
            sc += 10;
        } else if !nt.is_empty() && (nm.contains(&nt) || nt.contains(&nm)) {
            sc += 4;
        } else {
            continue; // 歌名必须相关
        }
        let nn = norm_alpha(singer);
        if !na.is_empty() && !nn.is_empty() && (nn == na || nn.contains(&na) || na.contains(&nn)) {
            sc += 6;
        }
        // 精度要求：歌手已知时必须“歌名+歌手”都命中（sc>=16）；歌手未知时才接受歌名完全一致（sc>=10）。
        // 绝不再接受仅“歌名包含”的其它版本/同名歌。
        if !na.is_empty() && sc < 16 {
            continue;
        }
        if na.is_empty() && sc < 10 {
            continue;
        }
        if best.as_ref().map(|(bs, _)| sc > *bs).unwrap_or(true) {
            best = Some((sc, mid.to_string()));
        }
    }
    best.map(|(_, mid)| mid)
}

fn qq_fetch(title: &str, artist: &str) -> Option<Vec<LrcLine>> {
    let mid = qq_search_mid(title, artist)?;
    let url = format!(
        "https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg?songmid={}&format=json&nobase64=1",
        mid
    );
    let v = get_json_with(&url, "https://y.qq.com/")?;
    let lyr = v.get("lyric").and_then(|x| x.as_str()).unwrap_or("");
    if lyr.trim().is_empty() {
        return None;
    }
    // QQ 音乐对整首无词曲目会返回“纯音乐，请欣赏”标记：作为占位歌词。
    if looks_instrumental(lyr) {
        return Some(instrumental_line());
    }
    let l = filt(parse_lrc(lyr));
    if l.len() >= 2 {
        dlog(&format!("qq lyric mid={} lines={}", mid, l.len()));
        Some(l)
    } else {
        None
    }
}

// ===== 备用歌词源 2：LRCLIB（https://lrclib.net，免登录，返回 LRC 同步歌词）=====
fn lrclib_fetch(title: &str, artist: &str, expected_dur: f64) -> Option<Vec<LrcLine>> {
    let ctitle = clean_title(title);
    if ctitle.is_empty() {
        return None;
    }
    if !lrclib_available() {
        dlog("lrclib skipped (recent timeout)");
        return None;
    }
    let mut url = format!("https://lrclib.net/api/search?track_name={}", pct_encode(&ctitle));
    if !artist.trim().is_empty() {
        url.push_str(&format!("&artist_name={}", pct_encode(artist.trim())));
    }
    let v = match get_json_fast(&url, "https://lrclib.net/") {
        Some(v) => v,
        None => {
            lrclib_mark_down();
            return None;
        }
    };
    let arr = v.as_array()?;
    let nt = norm_alpha(&ctitle);
    let na = norm_alpha(artist);
    let mut best: Option<(f64, &serde_json::Value)> = None;
    for it in arr {
        if it.get("instrumental").and_then(|x| x.as_bool()).unwrap_or(false) {
            continue;
        }
        let synced = it.get("syncedLyrics").and_then(|x| x.as_str()).unwrap_or("");
        if synced.trim().is_empty() {
            continue;
        }
        let tn = it.get("trackName").and_then(|x| x.as_str()).unwrap_or("");
        let an = it.get("artistName").and_then(|x| x.as_str()).unwrap_or("");
        let nm = norm_alpha(tn);
        let mut sc = 0.0f64;
        if !nt.is_empty() && nm == nt {
            sc += 10.0;
        } else if !nt.is_empty() && (nm.contains(&nt) || nt.contains(&nm)) {
            sc += 4.0;
        } else {
            continue;
        }
        let nn = norm_alpha(an);
        let artist_match =
            !na.is_empty() && !nn.is_empty() && (nn == na || nn.contains(&na) || na.contains(&nn));
        if !na.is_empty() {
            // 歌手已知：必须“歌名+歌手”都命中，任何只沾歌名的同名歌都拒绝
            if !artist_match {
                continue;
            }
            sc += 6.0;
        } else {
            // 歌手未知：必须歌名完全一致 + 时长强吻合，杜绝同名不同曲
            if sc < 10.0 {
                continue;
            }
            if expected_dur >= 20.0 {
                match it.get("duration").and_then(|x| x.as_f64()) {
                    Some(d) if (d - expected_dur).abs() <= (expected_dur * 0.03).max(3.0) => {}
                    _ => continue,
                }
            }
        }
        if expected_dur >= 20.0 {
            if let Some(d) = it.get("duration").and_then(|x| x.as_f64()) {
                sc += 10.0 - (d - expected_dur).abs().min(10.0); // 时长越接近分越高
            }
        }
        if best.as_ref().map(|(bs, _)| sc > *bs).unwrap_or(true) {
            best = Some((sc, it));
        }
    }
    let (_, it) = best?;
    let synced = it.get("syncedLyrics").and_then(|x| x.as_str()).unwrap_or("");
    let l = filt(parse_lrc(synced));
    if l.len() >= 2 {
        dlog(&format!("lrclib pick lines={}", l.len()));
        Some(l)
    } else {
        None
    }
}

/// 依次尝试备用源：QQ 音乐 → LRCLIB。返回带时间轴的歌词。
fn fetch_alternates(title: &str, artist: &str, expected_dur: f64) -> Option<Vec<LrcLine>> {
    if let Some(l) = qq_fetch(title, artist) {
        return Some(l);
    }
    lrclib_fetch(title, artist, expected_dur)
}

/// 拉取歌词 + 专辑封面图（JPEG/PNG 原始字节）。任一失败返回 None。
/// 带磁盘缓存：同一首（歌名+歌手哈希）只联网一次。
pub fn fetch_full(
    title: &str,
    artist: &str,
    expected_dur: f64,
    soda: bool,
) -> (Option<Vec<LrcLine>>, Option<Vec<u8>>) {
    if std::env::var_os("WINISLE_NOLYRIC").is_some() {
        return (None, None);
    }
    // 传进来的可能是可疑试听片段时长（<5s/0 表示未知）：若已知整首时长就用它，避免同一首歌
    // 在“片段桶”和“整首桶”里各缓存一份、且选错版本。
    let mut expected_dur = expected_dur;
    if expected_dur < 5.0 {
        if let Some(kd) = known_duration(title, artist) {
            if kd >= 20.0 {
                expected_dur = kd;
            }
        }
    }
    let key = key_hash(title, artist, expected_dur);
    let dir = cache_dir();
    if let Some(d) = &dir {
        let lp = d.join(format!("{}.lrc", key));
        let yp = d.join(format!("{}.yrc", key));
        let cp = d.join(format!("{}.cover", key));
        let np = d.join(format!("{}.nolyric", key));
        // 纯音乐占位缓存：歌词源只给“纯音乐，请欣赏”时，写这个标记（不写 .lrc，避免被 is_noise 过滤掉）。
        let purep = d.join(format!("{}.pure", key));
        let cached_pure = std::fs::read_to_string(&purep)
            .ok()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        // 负缓存：之前确认过“确实没有歌词”（网易云/QQ/LRCLIB 都没有），6 小时内不再重复联网。
        let no_recent = std::fs::metadata(&np)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map(|e| e.as_secs() < 6 * 3600)
            .unwrap_or(false);
        // 旧版缓存可能存过“被试听片段时长压扁”的歌词：整首时长远大于缓存最后一句时间 → 判为坏缓存，重新联网取。
        let known_full = known_duration(title, artist).unwrap_or(0.0);
        let stale = |v: &Vec<LrcLine>| {
            let last = v.last().map(|l| l.t).unwrap_or(0.0);
            // 旧版缓存可能存过“被试听片段时长压扁”的歌词
            let squashed = known_full >= 120.0 && last < known_full * 0.4;
            // 缓存歌词末句时间与当前实际时长差太多 → 多半是“同名不同歌”的错误匹配缓存，判为坏缓存重新取。
            // 阈值放到 35%（时长来源本身可能偏差几十秒：如汽水时长缓存滞后），只拦“整首差一截”的明显错配。
            let wrong_song =
                expected_dur >= 60.0 && last >= 1.0 && (last - expected_dur).abs() > (expected_dur * 0.35).max(25.0);
            squashed || wrong_song
        };
        // 汽水歌曲：歌词只认「汽水内存读到的官方 LRC」；把它单独缓存成 {key}.soda2.lrc，
        // 记住这首读成功过，下次直接命中（不必每次都让汽水重新渲染歌词页）。
        // 网络兜底（严格匹配后）另存 {key}.sodanet2.lrc，与官方缓存分开，避免把网络歌词当官方永久信任。
        // 版本号 soda2/sodanet2：旧版 .soda.lrc 可能残留“同名不同曲”的错误歌词，直接弃用重新取。
        let soda_cache = d.join(format!("{}.soda2.lrc", key));
        let soda_net_cache = d.join(format!("{}.sodanet2.lrc", key));
        // 绝不读取任何网络来源的 .lrc/.yrc 缓存（避免历史错配又被读出来）。
        let cached_lines = if cached_pure {
            Some(instrumental_line())
        } else if soda {
            std::fs::read_to_string(&soda_cache)
                .ok()
                .map(|s| filt(parse_lrc(&s)))
                .filter(|v| !v.is_empty() && !stale(v))
                .or_else(|| {
                    std::fs::read_to_string(&soda_net_cache)
                        .ok()
                        .map(|s| filt(parse_lrc(&s)))
                        .filter(|v| !v.is_empty() && !stale(v))
                })
        } else {
            std::fs::read_to_string(&yp)
                .ok()
                .map(|s| filt(parse_yrc(&s)))
                .filter(|v| !v.is_empty() && !stale(v))
                .or_else(|| {
                    std::fs::read_to_string(&lp)
                        .ok()
                        .map(|s| filt(parse_lrc(&s)))
                        .filter(|v| !v.is_empty() && !stale(v))
                })
        };
        let cached_cover = std::fs::read(&cp).ok().filter(|v| !v.is_empty());
        if cached_lines.is_some() || cached_cover.is_some() {
            dlog(&format!("cache hit {} lines={} cover={}B", title, cached_lines.as_ref().map(|v| v.len()).unwrap_or(0), cached_cover.as_ref().map(|v| v.len()).unwrap_or(0)));
            if cached_lines.is_some() && cached_cover.is_some() {
                return (cached_lines, cached_cover);
            }
            if cached_lines.is_none() && no_recent {
                dlog(&format!("nolyric cached {}", title));
                return (None, cached_cover);
            }
            // 缓存不全则继续联网补齐（保留已有部分）
            let (l2, c2) = fetch_remote(title, artist, expected_dur, &key, dir.as_ref(), soda);
            match &l2 {
                Some(v) if is_instrumental(v) => {
                    let _ = std::fs::write(&purep, INSTRUMENTAL_TEXT.as_bytes());
                    if !soda {
                        let _ = std::fs::remove_file(&np);
                    }
                }
                None if !soda => {
                    let _ = std::fs::write(&np, b"1");
                }
                _ => {}
            }
            return (l2.or(cached_lines), c2.or(cached_cover));
        }
    }
    let (l, c) = fetch_remote(title, artist, expected_dur, &key, dir.as_ref(), soda);
    if let Some(d) = &dir {
        if let Some(v) = &l {
            if is_instrumental(v) {
                let _ = std::fs::write(d.join(format!("{}.pure", key)), INSTRUMENTAL_TEXT.as_bytes());
                if !soda {
                    let _ = std::fs::remove_file(d.join(format!("{}.nolyric", key)));
                }
            }
        } else if !soda {
            // 汽水不写“无歌词”负缓存：歌词可能只是还没加载，下次播放应重新读内存。
            let _ = std::fs::write(d.join(format!("{}.nolyric", key)), b"1");
        }
    }
    (l, c)
}

/// 汽水音乐在播时：歌词只认汽水渲染进程内存里的官方 LRC（不跨源拼凑，避免错配）；
/// 封面仍尽量从网易云按歌名取（仅封面，不用其歌词）。取不到歌词就返回空（显示“暂无歌词”）。
fn fetch_remote(
    title: &str,
    artist: &str,
    expected_dur: f64,
    key: &str,
    dir: Option<&PathBuf>,
    soda: bool,
) -> (Option<Vec<LrcLine>>, Option<Vec<u8>>) {
    if soda {
        // 汽水歌曲：歌词只认汽水自己的官方数据，绝不跨源适配，从根上杜绝“把别首歌贴上来”。
        // 首选：通过 CDP 读渲染进程里 LyricsView 的官方歌词（含逐字时间，不需要显示歌词页）。
        // 用 no_nav 版本：绝不在用户浏览歌单/场景模式时强行把汽水切回播放页；读不到再走内存扫描兜底。
        if let Some(sl) = crate::cdp_lyric::fetch_current_no_nav(title, artist) {
            let cover = if !sl.cover.is_empty() {
                get_bytes(&sl.cover)
            } else {
                None
            };
            if let (Some(d), Some(c)) = (dir, cover.as_ref()) {
                let _ = std::fs::write(d.join(format!("{}.cover", key)), c);
            }
            // 纯音乐（歌词正文只有“纯音乐，请欣赏”之类）：返回占位，收起/展开都显示这句话。
            if looks_instrumental(&sl.content) {
                dlog(&format!("soda instrumental {} - {}", sl.name, sl.artist));
                return (Some(instrumental_line()), cover);
            }
            let l = crate::cdp_lyric::parse_content(&sl.content);
            if l.len() >= 2 {
                dlog(&format!(
                    "soda cdp {} - {} dur={:.1} lines={}",
                    sl.name,
                    sl.artist,
                    sl.duration,
                    l.len()
                ));
                if let Some(d) = dir {
                    let body: String = l
                        .iter()
                        .map(|x| format!("[{:02}:{:05.2}]{}\n", (x.t as u32) / 60, x.t % 60.0, x.text))
                        .collect();
                    let _ = std::fs::write(d.join(format!("{}.soda2.lrc", key)), body);
                }
                return (Some(l), cover);
            }
        }
        // 兜底：直接从汽水进程内存里扫官方 LRC（name 精确匹配）。
        if let Some(l) = soda_read_with_retry(title, artist, expected_dur) {
            dlog(&format!("soda lyric {} - {} lines={}", title, artist, l.len()));
            // 记住「这首从汽水内存成功读到过官方歌词」，写成 {key}.soda.lrc，下次直接命中。
            // 纯音乐占位不写 .soda.lrc（读回会被 is_noise 过滤），由上层写 .pure 标记。
            if let Some(d) = dir {
                if !is_instrumental(&l) {
                    let body: String = l
                        .iter()
                        .map(|x| format!("[{:02}:{:05.2}]{}\n", (x.t as u32) / 60, x.t % 60.0, x.text))
                        .collect();
                    let _ = std::fs::write(d.join(format!("{}.soda2.lrc", key)), body);
                }
            }
            let cover: Option<Vec<u8>> = None;
            return (Some(l), cover);
        }
        dlog(&format!("soda none {} - {} (official only, network fallback disabled)", title, artist));
        // 汽水官方源拿不到（典型：停留在歌单/场景页，LyricsView 未挂载，内存也没扫到）时，
        // 回退到网络歌词源（网易云/QQ/LRCLIB），但用「歌词末句时间 vs 实际时长」做校验，
        // 时长对不上（同名不同歌/翻唱/片段）就丢弃，尽量避免把别的歌贴上来。
        if std::env::var_os("WINISLE_SODA_NETLYRIC").is_some() {
            let (nl, nc) = fetch_remote_net(title, artist, expected_dur, key, dir);
            if let Some(v) = nl {
                let last = v.last().map(|l| l.t).unwrap_or(0.0);
                let ok = if expected_dur >= 60.0 && last >= 1.0 {
                    (last - expected_dur).abs() <= (expected_dur * 0.20).max(20.0)
                } else {
                    true
                };
                if ok && v.len() >= 2 {
                    dlog(&format!("soda netlyric {} - {} lines={}", title, artist, v.len()));
                    if let Some(d) = dir {
                        let body: String = v
                            .iter()
                            .map(|x| format!("[{:02}:{:05.2}]{}\n", (x.t as u32) / 60, x.t % 60.0, x.text))
                            .collect();
                        let _ = std::fs::write(d.join(format!("{}.sodanet2.lrc", key)), body);
                    }
                    return (Some(v), nc);
                } else {
                    dlog(&format!(
                        "soda netlyric reject {} - {} last={:.1} exp={:.1}",
                        title, artist, last, expected_dur
                    ));
                }
            }
        }
        let cover: Option<Vec<u8>> = None;
        return (None, cover);
    }
    fetch_remote_net(title, artist, expected_dur, key, dir)
}

/// 汽水歌词刚开播可能还没写进内存。历史上的“单次请求内 3 次重扫”会与上层重试叠加成
/// 数十次全量内存扫描（每次上限 ~1.5GB），是播放时 CPU 飙到一核的主因。现在改为单次读取，
/// 由上层（带冷却/限次）负责重试，避免同一首歌被反复全量扫描。
fn soda_read_with_retry(title: &str, artist: &str, expected_dur: f64) -> Option<Vec<LrcLine>> {
    crate::soda_lyrics::read_for(title, artist, expected_dur)
}


fn fetch_remote_net(
    title: &str,
    artist: &str,
    expected_dur: f64,
    key: &str,
    dir: Option<&PathBuf>,
) -> (Option<Vec<LrcLine>>, Option<Vec<u8>>) {
    // 优先从网易云客户端本地播放队列里解析出「确切的 song id」，再按 id 取歌词，
    // 从根上避免「靠歌名搜索 → 同名歌曲/翻唱/伴奏错配」（如 Keep the Roses→roses）。
    let mem_id = crate::netease_mem::current_song_id(title, artist, expected_dur);
    let cands: Vec<(i32, i64, f64)> = match mem_id {
        Some(id) => {
            dlog(&format!("netease mem id={} for {} - {}", id, title, artist));
            vec![(100, id, expected_dur)]
        }
        None => find_song_candidates(title, artist),
    };
    if cands.is_empty() {
        dlog(&format!("no song id for {} - {}", title, artist));
        return (None, None);
    }
    // 精确度优先（最准方案）：只接受「歌名完全一致 + 歌手一致」(>=16) 的候选。
    // 歌手未知时才允许“歌名完全一致”(>=10)，且必须时长强吻合（见 strict_dur_ok）。
    // 绝不接受“仅包含/相似”或“只靠歌手”的其它歌 —— 这正是之前把英文歌配成中文歌词的根源
    // （clean_title 会去掉括号里的版本号，像 “Hong Kong (AeroX Mix)” → “Hong Kong”，
    //  而网易云有大量同名中文《Hong Kong》，一旦不校验歌手就会错配）。
    let artist_known = norm_alpha(artist).chars().count() >= 2;
    let strict_dur_ok = |cd: f64| -> bool {
        if artist_known {
            true
        } else if expected_dur >= 30.0 && cd >= 5.0 {
            (cd - expected_dur).abs() <= (expected_dur * 0.03).max(3.0)
        } else {
            true
        }
    };
    let mut pool: Vec<usize> = (0..cands.len()).filter(|&i| cands[i].0 >= 16).collect();
    if pool.is_empty() && !artist_known {
        pool = (0..cands.len()).filter(|&i| cands[i].0 >= 10).collect();
    }
    if pool.is_empty() {
        dlog(&format!(
            "no strict lyric candidate {} - {} (artist_known={})",
            title, artist, artist_known
        ));
        // 网易云没有严格候选时，仍用同样严格的标准试备用源（QQ/LRCLIB），
        // 而不是直接放弃——避免“其实别处有正确歌词却没去取”。
        if let Some(l) = fetch_alternates(title, artist, expected_dur) {
            dlog(&format!("alt lyric(strict) {} - {} lines={}", title, artist, l.len()));
            return (Some(l), None);
        }
        return (None, None);
    }
    // 版本优先：同一池内，时长与"当前播放器实际时长"最接近的排前面。
    // 但若 expected_dur 疑似 VIP 试听片段（<60s，且有明显更长的完整版本），
    // 就不能按“接近片段时长”选（会挑到 30s 试听版歌词）——改为按匹配分、再按时长从长到短。
    let trial = expected_dur < 5.0
        || (expected_dur < 60.0 && cands.iter().any(|c| c.2 > expected_dur * 1.5));
    let mut idxs: Vec<usize> = pool;
    idxs.sort_by(|&i, &j| {
        if trial {
            // 只按匹配分排序（稳定排序保留搜索原始相关度顺序），不按时长挑——
            // 否则可能挑到更长的现场/重制版。完整版搜索结果的 dt 本来就是整首时长。
            return cands[j].0.cmp(&cands[i].0);
        }
        let ci = (cands[i].2 - expected_dur).abs();
        let cj = (cands[j].2 - expected_dur).abs();
        let class_i = if expected_dur > 5.0 && ci > 8.0 { 1 } else { 0 };
        let class_j = if expected_dur > 5.0 && cj > 8.0 { 1 } else { 0 };
        class_i
            .cmp(&class_j)
            .then(ci.partial_cmp(&cj).unwrap_or(std::cmp::Ordering::Equal))
            .then(cands[j].0.cmp(&cands[i].0))
    });

    // 逐个候选尝试取歌词：只试“强匹配”候选（歌名精确或歌手命中），
    // 跳过“暂无歌词/纯音乐/空”的版本，取第一个有真实歌词的；都无则视为无歌词。
    let mut lines: Option<Vec<LrcLine>> = None;
    let mut chosen_id: Option<i64> = None;
    let mut chosen_yrc: Option<String> = None;
    let mut chosen_dur: f64 = 0.0;
    // 记录最近一个“强匹配候选”的歌词正文只是“纯音乐，请欣赏”标记的版本（整首无人声歌词）。
    // 只有在所有候选都拿不到真实歌词时，才用它作为占位歌词展示。
    let mut instrumental: Option<(i64, f64)> = None;
    // 第一轮：在强匹配候选里优先找“带逐字(yrc)”的版本（同名歌曲不同上传，有的有 yrc）
    for &ci in idxs.iter().take(12) {
        let (sc, id, _cd) = cands[ci];
        // 时长闸门：仅在“歌手未知、靠歌名匹配”(artist_known=false) 时启用。
        if !strict_dur_ok(_cd) {
            dlog(&format!("skip yrc dur id={} cd={:.1} exp={:.1}", id, _cd, expected_dur));
            continue;
        }
        let lurl = format!("https://music.163.com/api/song/lyric?id={}&lv=-1&kv=-1&tv=-1&yv=-1&yrv=-1", id);
        if let Some(lj) = get_json(&lurl) {
            let yrc_txt = lj.get("yrc").and_then(|v| v.get("lyric")).and_then(|v| v.as_str()).unwrap_or("");
            if !yrc_txt.trim().is_empty() {
                if sc >= 10 && instrumental.is_none() && looks_instrumental(yrc_txt) {
                    instrumental = Some((id, _cd));
                }
                let l = filt(parse_yrc(yrc_txt));
                let bad = l.iter().any(|x| x.text.contains("暂无歌词") || x.text.contains("纯音乐"));
                if l.len() >= 2 && !bad {
                    dlog(&format!("yrc pick id={} sc={} lines={}", id, sc, l.len()));
                    lines = Some(l);
                    chosen_id = Some(id);
                    chosen_yrc = Some(yrc_txt.to_string());
                    chosen_dur = _cd;
                    break;
                }
            }
        }
    }
    // 第二轮：没有逐字版本时，退回整行 lrc
    if lines.is_none() {
        for &ci in idxs.iter().take(12) {
            let (sc, id, _cd) = cands[ci];
            // 时长闸门：同上，仅在歌手未知时启用。
            if !strict_dur_ok(_cd) {
                dlog(&format!("skip lrc dur id={} cd={:.1} exp={:.1}", id, _cd, expected_dur));
                continue;
            }
            let lurl = format!("https://music.163.com/api/song/lyric?id={}&lv=-1&kv=-1&tv=-1", id);
            if let Some(lj) = get_json(&lurl) {
                if let Some(lrc) = lj.get("lrc").and_then(|v| v.get("lyric")).and_then(|v| v.as_str()) {
                    if sc >= 10 && instrumental.is_none() && looks_instrumental(lrc) {
                        instrumental = Some((id, _cd));
                    }
                    let l = filt(parse_lrc(lrc));
                    let bad = l.iter().any(|x| x.text.contains("暂无歌词") || x.text.contains("纯音乐"));
                    if l.len() >= 2 && !bad {
                        dlog(&format!("lyric pick id={} sc={} lines={}", id, sc, l.len()));
                        lines = Some(l);
                        chosen_id = Some(id);
                        chosen_dur = _cd;
                        break;
                    }
                }
            }
        }
    }

    // 网易云明确标注“纯音乐，请欣赏”但没有任何真实歌词行：用占位歌词展示（收起/展开都显示这句话）。
    if lines.is_none() {
        if let Some((id, cd)) = instrumental {
            dlog(&format!("netease instrumental id={} {} - {}", id, title, artist));
            lines = Some(instrumental_line());
            chosen_id = Some(id);
            chosen_dur = cd;
        }
    }

    // 网易云没有可用歌词时，尝试备用源（QQ 音乐 → LRCLIB），提高汽水/抖音等曲目覆盖。
    if lines.is_none() {
        if let Some(l) = fetch_alternates(title, artist, expected_dur) {
            dlog(&format!("alt lyric {} - {} lines={}", title, artist, l.len()));
            lines = Some(l);
            chosen_dur = 0.0; // 备用源时长未知，跳过下面的比例缩放
        }
    }

    // 同名不同版本（现场/录音室/重新编曲）时长不同：若最接近版本的时长仍与实际播放时长差 >2.5s，
    // 按比例缩放歌词时间轴，避免整首歌词系统性快/慢（比直接错位好得多）。
    // 但只在两边都是“整首时长”（>=60s）且比例接近 1（0.7~1.4）时才缩放：
    // VIP 试听片段（30~40s）绝不能把完整歌词按比例压扁，否则整首歌词会挤成一团。
    let trial = expected_dur < 5.0
        || (expected_dur < 60.0 && cands.iter().any(|c| c.2 > expected_dur * 1.5));
    if !trial && expected_dur >= 60.0 && chosen_dur >= 60.0 {
        let ratio = expected_dur / chosen_dur;
        if (ratio - 1.0).abs() > 0.01 && ratio > 0.7 && ratio < 1.4 {
            if let Some(v) = lines.as_mut() {
                for l in v.iter_mut() {
                    l.t *= ratio;
                    for s in l.segs.iter_mut() {
                        s.t *= ratio;
                        s.dur *= ratio;
                    }
                }
            }
            dlog(&format!("time-scale {:.3} expected={:.1} chosen={:.1}", ratio, expected_dur, chosen_dur));
        }
    }

    // 封面 + 时长：优先用选中歌曲，否则用第一个候选
    let mut cover: Option<Vec<u8>> = None;
    let mut dur_ms: Option<i64> = None;
    if let Some(id) = chosen_id.or_else(|| cands.first().map(|(_, id, _)| *id)) {
        let durl = format!("https://music.163.com/api/song/detail?ids=[{}]", id);
        if let Some(dj) = get_json(&durl) {
            if let Some(s0) = dj.get("songs").and_then(|v| v.as_array()).and_then(|a| a.get(0)) {
                dur_ms = s0
                    .get("dt")
                    .and_then(|v| v.as_i64())
                    .or_else(|| s0.get("duration").and_then(|v| v.as_i64()));
                if let Some(url) = s0
                    .get("album")
                    .and_then(|al| al.get("picUrl"))
                    .and_then(|u| u.as_str())
                {
                    if !url.is_empty() {
                        // 取 400x400 缩略图，避免 6MB 大图
                        let u2 = if url.contains('?') { url.to_string() } else { format!("{}?param=400y400", url) };
                        cover = get_bytes(&u2);
                    }
                }
            }
        }
    }

    dlog(&format!("net {} - {} => lines={} cover={}B", title, artist, lines.as_ref().map(|v| v.len()).unwrap_or(0), cover.as_ref().map(|v| v.len()).unwrap_or(0)));

    // 写缓存
    if let Some(d) = dir {
        if let Some(y) = &chosen_yrc {
            let _ = std::fs::write(d.join(format!("{}.yrc", key)), y);
        }
        if let Some(l) = &lines {
            if !is_instrumental(l) {
                let body: String = l.iter().map(|x| format!("[{:02}:{:05.2}]{}\n", (x.t as u32) / 60, x.t % 60.0, x.text)).collect();
                let _ = std::fs::write(d.join(format!("{}.lrc", key)), body);
            }
        }
        if let Some(c) = &cover {
            if c.len() > 1000 {
                let _ = std::fs::write(d.join(format!("{}.cover", key)), c);
            }
        }
        if let Some(ms) = dur_ms {
            let _ = std::fs::write(d.join(format!("{}.dur", key)), ms.to_string());
            // 另存“歌名/歌手 -> 时长”索引（key_hash 含时长分桶，无法靠它反查时长）
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(d.join("durations.txt"))
            {
                use std::io::Write as _;
                let _ = writeln!(f, "{}\t{}\t{}", title, artist, ms);
            }
        }
    }
    (lines, cover)
}

/// 仅取歌词（保留旧接口）。
pub fn fetch(title: &str, artist: &str) -> Option<Vec<LrcLine>> {
    fetch_full(title, artist, 0.0, false).0
}

/// 已知歌曲总时长（秒）：读缓存（联网时随歌词一起写入）。
pub fn known_duration(title: &str, artist: &str) -> Option<f64> {
    let dir = cache_dir()?;
    let txt = std::fs::read_to_string(dir.join("durations.txt")).ok()?;
    for line in txt.lines() {
        let mut it = line.split('\t');
        let (t, a, d) = (it.next()?, it.next()?, it.next()?);
        if t == title && a == artist {
            if let Ok(ms) = d.trim().parse::<f64>() {
                if ms > 5000.0 {
                    return Some(ms / 1000.0);
                }
            }
        }
    }
    None
}

#[allow(dead_code)]
fn known_duration_old(title: &str, artist: &str) -> Option<f64> {
    let dir = cache_dir()?;
    let key = key_hash(title, artist, 0.0);
    let txt = std::fs::read_to_string(dir.join(format!("{}.dur", key))).ok()?;
    let ms: f64 = txt.trim().parse().ok()?;
    if ms > 5000.0 {
        Some(ms / 1000.0)
    } else {
        None
    }
}

#[cfg(test)]
mod probes {
    // 验证“同名不同歌”闸门：Keep the Roses - KAVO 实际时长 156.9s，
    // 网易云旧逻辑会把 Laleh《roses》(~264s) 贴上来；修复后应被时长闸门拒掉。
    #[test]
    #[ignore]
    fn probe_keep_roses() {
        let (l, _c) = super::fetch_full("Keep the Roses", "KAVO", 156.9, false);
        match l {
            Some(v) => eprintln!(
                "lines={} first={:?} last_t={:.1}",
                v.len(),
                v.first().map(|x| x.text.clone()).unwrap_or_default(),
                v.last().map(|x| x.t).unwrap_or(0.0)
            ),
            None => eprintln!("none"),
        }
    }

    // 验证「网易云按 song id 取词」：清掉缓存后走一遍 fetch_full（非汽水），
    // 期望日志出现 `netease mem id=` 且拿到正确整首歌词。
    #[test]
    #[ignore]
    fn probe_fetch_netease_env() {
        let title = std::env::var("NE_TITLE").unwrap_or_default();
        let artist = std::env::var("NE_ARTIST").unwrap_or_default();
        let dur: f64 = std::env::var("NE_DUR")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        if title.is_empty() {
            eprintln!("set NE_TITLE");
            return;
        }
        if let Some(d) = super::cache_dir() {
            let key = super::key_hash(&title, &artist, dur);
            for ext in ["lrc", "yrc", "cover", "nolyric"] {
                let _ = std::fs::remove_file(d.join(format!("{}.{}", key, ext)));
            }
        }
        let id = crate::netease_mem::current_song_id(&title, &artist, dur);
        let (l, c) = super::fetch_full(&title, &artist, dur, false);
        eprintln!(
            "NE id={:?} lines={} cover={}B first={:?} last={:.1}",
            id,
            l.as_ref().map(|v| v.len()).unwrap_or(0),
            c.as_ref().map(|v| v.len()).unwrap_or(0),
            l.as_ref().and_then(|v| v.first().map(|x| x.text.clone())).unwrap_or_default(),
            l.as_ref().and_then(|v| v.last().map(|x| x.t)).unwrap_or(0.0)
        );
    }

    // 验证「汽水 CDP 取词」整条链路：清掉汽水缓存后 fetch_full(soda=true)。
    #[test]
    #[ignore]
    fn probe_fetch_soda_env() {
        let title = std::env::var("SODA_TITLE").unwrap_or_default();
        let artist = std::env::var("SODA_ARTIST").unwrap_or_default();
        let dur: f64 = std::env::var("SODA_DUR")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        if title.is_empty() {
            eprintln!("set SODA_TITLE");
            return;
        }
        if let Some(d) = super::cache_dir() {
            let key = super::key_hash(&title, &artist, dur);
            let _ = std::fs::remove_file(d.join(format!("{}.soda2.lrc", key)));
            let _ = std::fs::remove_file(d.join(format!("{}.sodanet2.lrc", key)));
        }
        let (l, c) = super::fetch_full(&title, &artist, dur, true);
        eprintln!(
            "SODA lines={} cover={}B first={:?} last={:.1}",
            l.as_ref().map(|v| v.len()).unwrap_or(0),
            c.as_ref().map(|v| v.len()).unwrap_or(0),
            l.as_ref().and_then(|v| v.first().map(|x| x.text.clone())).unwrap_or_default(),
            l.as_ref().and_then(|v| v.last().map(|x| x.t)).unwrap_or(0.0)
        );
    }
}

#[cfg(test)]
mod instrumental_tests {
    use super::*;

    #[test]
    fn credit_lines_are_filtered() {        let l = super::parse_lrc(
            "[00:01.86]词Lyrics by：姜洄\n\
             [00:01.87]曲Composed by：李哲\n\
             [00:01.88]编曲 Arranger：史嵩昊/韩冰\n\
             [00:01.89]制作人Producer：贺贺\n\
             [00:01.90]合声 Backing Vocals：王宇宙Leto/乔浚丞\n\
             [00:01.92]录音师 Recording engineer：刘彦宏\n\
             [00:01.93]混音师 Mixing engineer：刘彦宏\n\
             [00:01.94]监制 Music supervisor：金华\n\
             [00:01.95]发行人 Publisher：潘昊铖\n\
             [00:01.97]OP：北京华数文化传媒有限公司/即刻半音\n\
             [00:02.00]营销协力 Marketing collaboration：阿子/隋冬雪//可可豆/派大星\n\
             [00:02.05]营销推广机构 Marketing and promotion agencies：百纳娱乐/网益文化/华数音乐\n\
             [00:02.07]【未经著作权人许可，不得翻唱翻录或使用】\n\
             [00:02.08]风吹过山 船靠了岸\n",
        );
        let f = super::filt(l);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].text, "风吹过山 船靠了岸");
    }

    #[test]
    fn looks_instrumental_detects_markers() {
        assert!(looks_instrumental("纯音乐，请欣赏"));
        assert!(looks_instrumental("纯音乐"));
        assert!(looks_instrumental("此歌曲为没有填词的纯音乐，请您欣赏"));
        assert!(looks_instrumental("[00:00.00]纯音乐，请欣赏"));
        assert!(looks_instrumental("[00:00.00]纯音乐，请欣赏\n"));
        assert!(looks_instrumental("[00:00.00]纯音乐，请欣赏\n[00:03.00]谢谢欣赏"));
        assert!(looks_instrumental("[00:00.00](0,100,0)纯(100,80,0)音乐，请欣赏"));
        assert!(looks_instrumental("请欣赏"));
        assert!(!looks_instrumental("作词 : 林夕"));
        assert!(!looks_instrumental(""));
        // 正常歌词里出现“请欣赏”不应误判为纯音乐
        assert!(!looks_instrumental("[00:12.00]我请你欣赏这满山花开\n[00:15.00]第二句歌词"));
        // 有真实歌词行 + 结尾一句“纯音乐”也不能把整首判为纯音乐
        assert!(!looks_instrumental("[00:00.00]第一句\n[00:03.00]纯音乐，请欣赏"));
        // 汽水把「有曲目但缺歌词」标成“暂无歌词，请欣赏”，这不是纯音乐，不能误判
        assert!(!looks_instrumental("暂无歌词，请欣赏"));
        assert!(!is_instrumental_marker("暂无歌词，请欣赏"));
    }

    #[test]
    fn instrumental_line_is_single_placeholder() {
        let l = instrumental_line();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].text, INSTRUMENTAL_TEXT);
        assert_eq!(l[0].t, 0.0);
        assert!(l[0].segs.is_empty());
    }

    #[test]
    fn is_instrumental_only_for_all_placeholder_lines() {
        assert!(is_instrumental(&instrumental_line()));
        assert!(!is_instrumental(&[]));
        let real = vec![
            LrcLine { t: 1.0, text: "第一句歌词".to_string(), segs: Vec::new() },
            LrcLine { t: 3.0, text: "第二句歌词".to_string(), segs: Vec::new() },
        ];
        assert!(!is_instrumental(&real));
    }
}

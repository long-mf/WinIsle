//! 应用内更新：检查 GitHub Release 的 latest.json、下载安装包、校验并启动安装器。
//!
//! 清单地址优先级：
//!   1. 环境变量 `WINISLE_UPDATE_URL`（可为 http(s) 或本地文件路径，方便测试）
//!   2. 编译期 `WINISLE_REPO`（CI 注入 `owner/repo`）→
//!      https://github.com/<owner>/<repo>/releases/latest/download/latest.json
//!
//! latest.json 结构：
//! { "version":"0.2.0", "notes":"...", "url":"https://.../WinIsle-Setup-0.2.0.exe",
//!   "sha256":"...", "size":12345678, "mandatory":false, "pub_date":"2026-11-20" }

use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct Manifest {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub mandatory: bool,
}

fn repo_slug() -> Option<String> {
    let r = std::env::var("WINISLE_REPO")
        .ok()
        .or_else(|| option_env!("WINISLE_REPO").map(|s| s.to_string()))?;
    let r = r.trim().to_string();
    if r.is_empty() || r.contains("YOUR_") {
        None
    } else {
        Some(r)
    }
}

/// 返回清单 URL；没有配置仓库时返回 None（开发版默认不联网检查）。
pub fn manifest_url() -> Option<String> {
    if let Ok(u) = std::env::var("WINISLE_UPDATE_URL") {
        if !u.trim().is_empty() {
            return Some(u.trim().to_string());
        }
    }
    repo_slug().map(|r| format!("https://github.com/{}/releases/latest/download/latest.json", r))
}

/// 本地日期（yyyy-mm-dd），用于「当天已检查过」的节流。
pub fn today_local() -> String {
    #[repr(C)]
    struct SystemTimeLocal {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    extern "system" {
        fn GetLocalTime(lp_system_time: *mut SystemTimeLocal);
    }
    let mut st = SystemTimeLocal {
        year: 0,
        month: 0,
        day_of_week: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
        milliseconds: 0,
    };
    unsafe {
        GetLocalTime(&mut st);
    }
    format!("{:04}-{:02}-{:02}", st.year, st.month, st.day)
}

fn read_text(url: &str) -> Result<String, String> {
    if let Some(p) = url.strip_prefix("file://") {
        return std::fs::read_to_string(p).map_err(|e| e.to_string());
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(3))
            .timeout_read(Duration::from_secs(6))
            .build();
        let resp = agent.get(url).call().map_err(|e| e.to_string())?;
        let mut s = String::new();
        resp.into_reader()
            .take(256 * 1024)
            .read_to_string(&mut s)
            .map_err(|e| e.to_string())?;
        return Ok(s);
    }
    std::fs::read_to_string(url).map_err(|e| e.to_string())
}

fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let text = text.trim_start_matches('\u{feff}');
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let version = v.get("version").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if version.is_empty() {
        return Err("latest.json 缺少 version".into());
    }
    Ok(Manifest {
        version,
        notes: v.get("notes").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        url: v.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        sha256: v.get("sha256").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        size: v.get("size").and_then(|x| x.as_u64()).unwrap_or(0),
        mandatory: v.get("mandatory").and_then(|x| x.as_bool()).unwrap_or(false),
    })
}

fn check_via_api(repo: &str) -> Result<Option<Manifest>, String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(10))
        .build();
    let resp = agent
        .get(&url)
        .set("User-Agent", "WinIsle-Update")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let mut s = String::new();
    resp.into_reader()
        .take(512 * 1024)
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
    let version = v
        .get("tag_name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .trim_start_matches('v')
        .to_string();
    if version.is_empty() {
        return Err("release 缺少 tag_name".into());
    }
    let wanted = format!("WinIsle-Setup-{}.exe", version);
    let assets = v.get("assets").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let mut chosen = None;
    for a in &assets {
        let name = a.get("name").and_then(|x| x.as_str()).unwrap_or("");
        if name.eq_ignore_ascii_case(&wanted) {
            chosen = Some(a.clone());
            break;
        }
    }
    if chosen.is_none() {
        for a in &assets {
            let name = a.get("name").and_then(|x| x.as_str()).unwrap_or("").to_lowercase();
            if name.starts_with("winisle-setup-") && name.ends_with(".exe") {
                chosen = Some(a.clone());
                break;
            }
        }
    }
    let a = match chosen {
        Some(a) => a,
        None => return Err("release 中没有 WinIsle-Setup-*.exe".into()),
    };
    let id = a.get("id").and_then(|x| x.as_u64()).ok_or("asset 缺少 id")?;
    let size = a.get("size").and_then(|x| x.as_u64()).unwrap_or(0);
    let digest = a.get("digest").and_then(|x| x.as_str()).unwrap_or("");
    let sha = digest.trim_start_matches("sha256:").to_string();
    let notes = v
        .get("body")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("name").and_then(|x| x.as_str()))
        .unwrap_or("")
        .to_string();
    Ok(Some(Manifest {
        version,
        notes,
        url: format!("https://api.github.com/repos/{}/releases/assets/{}", repo, id),
        sha256: sha,
        size,
        mandatory: false,
    }))
}

fn ver_key(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .split('.')
        .map(|x| x.parse::<u64>().unwrap_or(0))
        .collect()
}

/// a 是否比 b 新（按数字段比较；预发布后缀忽略）。
pub fn is_newer(a: &str, b: &str) -> bool {
    let (ka, kb) = (ver_key(a), ver_key(b));
    for i in 0..ka.len().max(kb.len()) {
        let x = *ka.get(i).unwrap_or(&0);
        let y = *kb.get(i).unwrap_or(&0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// 同步检查更新。返回 Ok(Some(m)) 表示有比当前更新的版本；
/// Ok(None) 表示已是最新（或未配置更新源）；Err 表示网络/格式错误。
pub fn check() -> Result<Option<Manifest>, String> {
    // Explicit override (local file / custom URL) keeps the manifest path.
    if let Ok(u) = std::env::var("WINISLE_UPDATE_URL") {
        let u = u.trim().to_string();
        if !u.is_empty() {
            let m = parse_manifest(&read_text(&u)?)?;
            return if is_newer(&m.version, VERSION) { Ok(Some(m)) } else { Ok(None) };
        }
    }
    // Default: GitHub REST API. This also works where github.com itself is
    // unreachable but api.github.com is available.
    let m = match repo_slug() {
        Some(repo) => check_via_api(&repo)?,
        None => return Ok(None),
    };
    match m {
        Some(m) if is_newer(&m.version, VERSION) => Ok(Some(m)),
        _ => Ok(None),
    }
}

fn upd_dir() -> PathBuf {
    let d = crate::paths::data_dir().join("updates");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn sha256_file(path: &Path) -> Result<String, String> {
    // 用系统 certutil 计算 SHA256，避免额外依赖。
    let out = std::process::Command::new("certutil")
        .args(["-hashfile"])
        .arg(path)
        .arg("SHA256")
        .output()
        .map_err(|e| e.to_string())?;
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        let t: String = line.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if t.len() == 64 {
            return Ok(t.to_lowercase());
        }
    }
    Err("certutil 未返回 SHA256".into())
}

fn download(url: &str, dest: &Path) -> Result<(), String> {
    if let Some(p) = url.strip_prefix("file://") {
        std::fs::copy(p, dest).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(60))
        .build();
    let resp = agent
        .get(url)
        .set("User-Agent", "WinIsle-Update")
        .set("Accept", "application/octet-stream")
        .call()
        .map_err(|e| e.to_string())?;
    let mut r = resp.into_reader();
    let mut f = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    std::io::copy(&mut r, &mut f).map_err(|e| e.to_string())?;
    Ok(())
}

/// 带进度回调的下载（百分比 0..100）。
fn download_with_progress(url: &str, dest: &Path, pct: &AtomicU8) -> Result<(), String> {
    if let Some(p) = url.strip_prefix("file://") {
        std::fs::copy(p, dest).map_err(|e| e.to_string())?;
        pct.store(100, Ordering::Relaxed);
        return Ok(());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(120))
        .build();
    let resp = agent
        .get(url)
        .set("User-Agent", "WinIsle-Update")
        .set("Accept", "application/octet-stream")
        .call()
        .map_err(|e| e.to_string())?;
    let total: Option<u64> = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok());
    let mut r = resp.into_reader();
    let mut f = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    let mut got: u64 = 0;
    loop {
        let n = r.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        if let Some(t) = total {
            if t > 0 {
                let p = ((got.saturating_mul(100) / t).min(99)) as u8;
                pct.store(p, Ordering::Relaxed);
            }
        }
    }
    let _ = f.flush();
    pct.store(100, Ordering::Relaxed);
    Ok(())
}

/// 下载并校验，期间通过 `pct` 报告 0..100 百分比。
pub fn download_setup_with_progress(m: &Manifest, pct: Arc<AtomicU8>) -> Result<PathBuf, String> {
    if m.url.trim().is_empty() {
        return Err("清单缺少下载地址".into());
    }
    let name = m
        .url
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .filter(|s| s.to_lowercase().ends_with(".exe"))
        .unwrap_or("WinIsle-Setup.exe");
    let dest = upd_dir().join(format!("{}-{}", m.version, name));
    let tmp = dest.with_extension("part");
    download_with_progress(&m.url, &tmp, &pct)?;
    if !m.sha256.trim().is_empty() {
        let got = sha256_file(&tmp)?;
        if got != m.sha256.trim().to_lowercase() {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("SHA256 不匹配（下载 {}）", &got[..8.min(got.len())]));
        }
    }
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// 下载安装包并校验；成功返回本地路径。`sha256` 为空时跳过校验（仅测试用）。
pub fn download_setup(m: &Manifest) -> Result<PathBuf, String> {
    if m.url.trim().is_empty() {
        return Err("清单缺少下载地址".into());
    }
    let name = m
        .url
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .filter(|s| s.to_lowercase().ends_with(".exe"))
        .unwrap_or("WinIsle-Setup.exe");
    let dest = upd_dir().join(format!("{}-{}", m.version, name));
    let tmp = dest.with_extension("part");
    download(&m.url, &tmp)?;
    if !m.sha256.trim().is_empty() {
        let got = sha256_file(&tmp)?;
        if got != m.sha256.trim().to_lowercase() {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("SHA256 不匹配（下载 {}）", &got[..8.min(got.len())]));
        }
    }
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// 启动安装器（Inno Setup 静默模式）。成功后本进程应尽快退出。
pub fn run_setup(path: &Path) -> Result<(), String> {
    std::process::Command::new(path)
        .args(["/SILENT", "/CLOSEAPPLICATIONS", "/RESTARTAPPLICATIONS", "/NORESTART", "/WINISLEUPDATE=1"])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.1.10", "0.1.9"));
    }

    #[test]
    fn parse_ok() {
        let m = parse_manifest(
            r#"{"version":"0.2.0","notes":"fix","url":"https://x/a.exe","sha256":"aa","size":5,"mandatory":true}"#,
        )
        .unwrap();
        assert_eq!(m.version, "0.2.0");
        assert!(m.mandatory);
        assert_eq!(m.size, 5);
    }

    #[test]
    #[ignore]
    fn probe_download_local() {
        let src = std::env::temp_dir().join("winisle_upd_src.exe");
        std::fs::write(&src, b"MZ fake installer bytes for test").unwrap();
        let sha = sha256_file(&src).unwrap();
        let m = Manifest {
            version: "0.0.9".into(),
            notes: "t".into(),
            url: format!("file://{}", src.display()),
            sha256: sha,
            size: 0,
            mandatory: false,
        };
        let got = download_setup(&m).expect("download");
        assert!(got.is_file());
        assert_eq!(std::fs::read(&got).unwrap(), b"MZ fake installer bytes for test");
        println!("downloaded {} ({} bytes)", got.display(), got.metadata().unwrap().len());
        let _ = std::fs::remove_file(&got);
        let _ = std::fs::remove_file(&src);
    }

    #[test]
    fn parse_missing_version() {
        assert!(parse_manifest(r#"{"notes":"x"}"#).is_err());
    }

    #[test]
    #[ignore]
    fn probe_api_env() {
        match check() {
            Ok(Some(m)) => println!(
                "API_UPDATE version={} size={} sha={} url={}",
                m.version, m.size, m.sha256, m.url
            ),
            Ok(None) => println!("API_NO_UPDATE (current {VERSION})"),
            Err(e) => println!("API_ERR {e}"),
        }
    }

    #[test]
    #[ignore]
    fn probe_download_api_env() {
        match check() {
            Ok(Some(m)) => match download_setup(&m) {
                Ok(p) => println!("API_DOWNLOAD ok {} bytes {}", p.display(), p.metadata().map(|x| x.len()).unwrap_or(0)),
                Err(e) => println!("API_DOWNLOAD_ERR {e}"),
            },
            Ok(None) => println!("API_NO_UPDATE"),
            Err(e) => println!("API_ERR {e}"),
        }
    }

    #[test]
    #[ignore]
    fn probe_manifest_env() {
        let url = manifest_url().expect("no manifest url");
        println!("URL={url}");
        match read_text(&url) {
            Ok(t) => match parse_manifest(&t) {
                Ok(m) => println!("OK version={} size={} url={}", m.version, m.size, m.url),
                Err(e) => println!("PARSE_ERR {e}"),
            },
            Err(e) => println!("FETCH_ERR {e}"),
        }
    }

    /// 端到端：真实检查→下载→校验→静默安装到临时目录→验证产物。
    #[test]
    #[ignore]
    fn probe_update_install_env() {
        let m = match check() {
            Ok(Some(m)) => m,
            Ok(None) => {
                println!("NO_UPDATE (current {VERSION})");
                return;
            }
            Err(e) => {
                println!("CHECK_ERR {e}");
                return;
            }
        };
        println!("FOUND version={} size={}", m.version, m.size);
        let setup = match download_setup(&m) {
            Ok(p) => p,
            Err(e) => {
                println!("DOWNLOAD_ERR {e}");
                return;
            }
        };
        println!("DOWNLOADED {}", setup.display());
        let dir = std::env::temp_dir().join("winisle_upd_install_test");
        let _ = std::fs::remove_dir_all(&dir);
        let st = std::process::Command::new(&setup)
            .arg("/VERYSILENT")
            .arg("/SUPPRESSMSGBOXES")
            .arg("/NORESTART")
            .arg("/SP-")
            .arg(format!("/DIR={}", dir.display()))
            .status();
        println!("SETUP_STATUS {:?}", st);
        let exe = dir.join("WinIsle.exe");
        if exe.is_file() {
            println!("INSTALLED size={}", exe.metadata().map(|x| x.len()).unwrap_or(0));
            let _ = std::process::Command::new(dir.join("unins000.exe"))
                .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
                .status();
            println!("UNINSTALLED exists={}", exe.exists());
        } else {
            println!("INSTALL_MISSING");
        }
    }
}

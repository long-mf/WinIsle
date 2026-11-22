//! 统一的数据目录。
//!
//! 数据默认跟随程序安装目录：优先使用 exe 同目录的 `data\`，这样卸载/迁移
//! 程序时数据跟着走，也不会往 C 盘用户目录里塞东西。
//! 仅当安装目录不可写（例如装进了 Program Files 且无权限）时，才回退到
//! `%APPDATA%\WinIsle\data`，保证程序仍能正常工作。
//! 可用环境变量 `WINISLE_DATA_DIR` 明确指定数据目录。
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

fn appdata_data() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("WinIsle").join("data"))
}

/// 旧版数据目录（品牌更名前的 LingDao / MyIsle）。
fn legacy_appdata_data() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from)?;
    for legacy in ["LingDao", "MyIsle"] {
        let old = base.join(legacy).join("data");
        if old.is_dir() {
            return Some(old);
        }
    }
    None
}

/// 解析并缓存数据目录（进程内只探测一次）。
pub fn data_dir() -> PathBuf {
    DATA_DIR.get_or_init(resolve_data_dir).clone()
}

fn resolve_data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("WINISLE_DATA_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|x| x.to_path_buf()));

    // 目标：数据跟着安装目录。先算出 exe 旁 data 目录，并把旧数据迁移过来。
    if let Some(dir) = &exe_dir {
        let local = dir.join("data");
        if !local.is_dir() {
            // 一次性迁移：%APPDATA%\WinIsle\data（或更早的 LingDao/MyIsle）→ exe\data
            let old = appdata_data().filter(|d| d.is_dir()).or_else(legacy_appdata_data);
            if let Some(old) = old {
                let _ = copy_dir_all(&old, &local);
            }
        }
        if dir_writable(&local) {
            return local;
        }
    }

    // 安装目录不可写时回退到 %APPDATA%，避免程序无法保存配置。
    if let Some(d) = appdata_data() {
        return d;
    }
    if let Some(dir) = exe_dir {
        return dir.join("data");
    }
    PathBuf::from("data")
}

/// 目录是否可创建且可写（用于判断能否把数据放在安装目录）。
fn dir_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".winisle_write_test");
    match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

pub fn ensure() -> PathBuf {
    let d = data_dir();
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn file(name: &str) -> PathBuf {
    ensure().join(name)
}

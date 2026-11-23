//! Embed the 'WinIsle' icon + version info into the exe (RT_GROUP_ICON / VERSIONINFO).
//! Uses the mingw-w64 `windres` that ships with the toolchain configured in genv.ps1,
//! so no Windows SDK / rc.exe is required.
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn find_windres() -> Option<PathBuf> {
    // 1) derive from the configured GNU linker driver (genv.ps1 sets gcc.exe path)
    if let Ok(gcc) = env::var("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER") {
        let cand = PathBuf::from(gcc).with_file_name("windres.exe");
        if cand.exists() {
            return Some(cand);
        }
    }
    // 2) well-known bundled toolchain location
    let p = PathBuf::from(r"D:\rust\toolchains\winlibs-msvcrt\mingw64\bin\windres.exe");
    if p.exists() {
        return Some(p);
    }
    // 3) PATH (CI installs mingw and puts windres on PATH)
    if let Ok(path) = env::var("PATH") {
        for dir in env::split_paths(&path) {
            let cand = dir.join("windres.exe");
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    None
}

/// Locate rc.exe (Windows SDK) for MSVC builds.
fn find_rc() -> Option<PathBuf> {
    if let Ok(p) = env::var("WINISLE_RC") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(path) = env::var("PATH") {
        for dir in env::split_paths(&path) {
            let cand = dir.join("rc.exe");
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    for key in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Ok(base) = env::var(key) {
            let kits = PathBuf::from(base).join("Windows Kits").join("10").join("bin");
            if let Ok(rd) = std::fs::read_dir(&kits) {
                let mut vers: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
                vers.sort();
                vers.reverse();
                for v in vers {
                    for arch in ["x64", "x86"] {
                        let cand = v.join(arch).join("rc.exe");
                        if cand.exists() {
                            return Some(cand);
                        }
                    }
                }
            }
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/winisle.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out_base = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target = env::var("TARGET").unwrap_or_default();

    // MSVC: use the Windows SDK rc.exe to produce a .res linked by link.exe.
    if target.contains("msvc") {
        if let Some(rc) = find_rc() {
            let out = out_base.join("app.res");
            let status = Command::new(&rc)
                .arg("/nologo")
                .arg("/fo")
                .arg(&out)
                .arg("/I")
                .arg(".")
                .arg("app.rc")
                .status();
            match status {
                Ok(s) if s.success() => {
                    println!("cargo:rustc-link-arg={}", out.display());
                }
                _ => println!("cargo:warning=rc failed to compile app.rc"),
            }
            return;
        }
        println!("cargo:warning=rc.exe not found for MSVC target, skipping icon resource");
        return;
    }

    let Some(windres) = find_windres() else {
        println!("cargo:warning=windres not found, skipping icon resource");
        return;
    };
    let out = out_base.join("app.res.o");
    let status = Command::new(&windres)
        .arg("-I").arg(".")
        .arg("-O").arg("coff")
        .arg("-o").arg(&out)
        .arg("app.rc")
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("cargo:rustc-link-arg={}", out.display());
        }
        _ => println!("cargo:warning=windres failed to compile app.rc"),
    }
}

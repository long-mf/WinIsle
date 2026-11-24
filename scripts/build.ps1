param([switch]$Deploy = $true)
$ErrorActionPreference = 'Continue'
. 'D:\LingDao\scripts\genv.ps1'
Set-Location 'D:\LingDao'
$out = & cargo build --release --features dev-icon 2>&1 | Out-String
$code = $LASTEXITCODE
($out -split "`n" | Select-String -Pattern 'error\[|error:|Finished') | ForEach-Object { $_.Line }
if ($code -ne 0) { throw "cargo build failed: $code" }
$exe = 'D:\LingDao\target\release\winisle.exe'
"built: $((Get-Item $exe).Length) bytes"
if ($Deploy) {
    # 只结束“开发版”自己的进程，绝不能误杀用户安装的正式版（可能在 D:\WinIsle 等其它目录）
    $devExes = @('D:\LingDao\WinIsle.exe', 'D:\LingDao\target\release\winisle.exe')
    $devProcs = Get-Process WinIsle -ErrorAction SilentlyContinue | Where-Object {
        try { $_.Path -and ($devExes -contains $_.Path) } catch { $false }
    }
    $devProcs | Stop-Process -Force
    for ($i = 0; $i -lt 25 -and (Get-Process WinIsle -ErrorAction SilentlyContinue | Where-Object {
        try { $_.Path -and ($devExes -contains $_.Path) } catch { $false }
    }); $i++) { Start-Sleep -Milliseconds 200 }
    if (Get-Process WinIsle -ErrorAction SilentlyContinue | Where-Object {
        try { $_.Path -and ($devExes -contains $_.Path) } catch { $false }
    }) { throw "开发版 WinIsle 进程未退出，无法覆盖 exe" }
    Copy-Item $exe 'D:\LingDao\WinIsle.exe' -Force
    # 关键：清掉当前 shell 里可能残留的 WINISLE_* 调试开关，避免被 Start-Process 继承。
    # 之前别的会话调休图时设过 WINISLE_FORCE_EXPAND=1，之后 build.ps1 重启时把它一并继承，
    # 导致新启动的岛被永久强制展开、无法收起（水波频谱当时只是被冤枉的“嫌疑人”）。
    Get-ChildItem Env: | Where-Object { $_.Name -like 'WINISLE_*' } | ForEach-Object {
        Remove-Item -Path ('Env:' + $_.Name) -ErrorAction SilentlyContinue
    }
    Start-Process 'D:\LingDao\WinIsle.exe'
    "deployed & restarted"
}

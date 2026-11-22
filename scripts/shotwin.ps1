param([string]$Out = 'D:\LingDao\docs\img\reminder_page.png', [double]$Scale = 2.0)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class WShot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  public struct RECT { public int Left,Top,Right,Bottom; }
}
'@
$p = $null; $w = 0; $h = 0; $r = New-Object WShot+RECT
for ($i = 0; $i -lt 30; $i++) {
  $p = Get-Process winisle -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
  if ($p) {
    [void][WShot]::GetWindowRect($p.MainWindowHandle, [ref]$r)
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    if ($w -gt 0 -and $h -gt 0) { break }
  }
  Start-Sleep -Milliseconds 200
}
if (-not $p) { throw 'winisle window not found' }
if ($w -le 0 -or $h -le 0) { throw "bad rect $w x $h" }
# High-DPI: real rendered pixels are Scale times GetWindowRect; allocate the scaled bitmap and PrintWindow once.
$dw = [int]($w * $Scale)
$dh = [int]($h * $Scale)
if ($dw -le 0 -or $dh -le 0) { throw "bad bitmap dims w=$w h=$h dw=$dw dh=$dh scale=$Scale" }
$bmp = [System.Drawing.Bitmap]::new($dw, $dh)
$g = [System.Drawing.Graphics]::FromImage([System.Drawing.Image]$bmp)
$hdc = $g.GetHdc()
[void][WShot]::PrintWindow($p.MainWindowHandle, $hdc, 2)
$g.ReleaseHdc($hdc)
$g.Dispose()
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output ("saved $Out  rect=$w x $h  out=$dw x $dh")

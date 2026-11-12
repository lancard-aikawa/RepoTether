# Capture the Offstage window to a PNG (for README images and checking the screen).
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File shoot.ps1 -Out <png> -ProcessId <pid> [-WaitSec 8]
# Always pass -ProcessId: without it the first Offstage window is taken, which may be the real one (real file names).
# Run Offstage first with the OFFSTAGE_* variables pointing at a demo made by make_demo.py.
param([string]$Out, [int]$WaitSec = 8, [int]$ProcessId = 0)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class W {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
[W]::SetProcessDPIAware() | Out-Null

$deadline = (Get-Date).AddSeconds($WaitSec + 20)
$p = $null
while ((Get-Date) -lt $deadline) {
  if ($ProcessId -le 0) { Write-Error "pass -ProcessId (the demo instance)"; exit 1 }
  $p = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 }
  if ($p) { break }
  Start-Sleep -Milliseconds 300
}
if (-not $p) { Write-Error "Offstage window not found"; exit 1 }
Start-Sleep -Seconds $WaitSec
[W]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 500
$r = New-Object W+RECT
[W]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$w = $r.R - $r.L; $h = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
# Ask the window to draw itself (PW_RENDERFULLCONTENT = 2), so other windows on top never end up in the image
$hdc = $g.GetHdc()
[W]::PrintWindow($p.MainWindowHandle, $hdc, 2) | Out-Null
$g.ReleaseHdc($hdc)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
"saved $Out ($w x $h) title=[$($p.MainWindowTitle)]"

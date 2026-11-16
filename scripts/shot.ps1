# Dev tool: launch (or attach to) a windowed exe and save a PNG of its window,
# without showing it to the person at the keyboard.
#   powershell -File tools/shot.ps1 -Exe target\debug\sonicly.exe -Out shot.png [-Wait 4] [-Keep] [-ExeArgs "..."]
#   powershell -File tools/shot.ps1 -ProcId 1234 -Out shot.png
# -Size 1600x1000 resizes the window (physical pixels) once it is off-screen, to test
# layouts larger than the monitor allows a new window to be.
# The child gets SONICLY_SHOT=1; Sonicly's binaries honor it by opening their
# window unfocused. Whatever the exe does, the window is moved off-screen the
# moment it exists, and captured there.
param([string]$Exe, [int]$ProcId = 0, [string]$Out = "shot.png", [double]$Wait = 4, [switch]$Keep, [string]$ExeArgs = "", [string]$Size = "", [string]$Profile = "shot")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class SonShot {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int ht, uint flags);
}
"@
[void][SonShot]::SetProcessDPIAware()
$env:SONICLY_SHOT = "1"
# A separate profile: captures never read or change the user's own library, settings or logins.
$env:SONICLY_PROFILE = $Profile
if ($Exe) {
  $p = if ($ExeArgs) { Start-Process -FilePath $Exe -ArgumentList $ExeArgs -PassThru } else { Start-Process -FilePath $Exe -PassThru }
} else { $p = Get-Process -Id $ProcId }
$deadline = (Get-Date).AddSeconds($Wait)
$moved = $false
while ((Get-Date) -lt $deadline) {
  $p.Refresh()
  if ($p.HasExited) { break }
  if (-not $moved -and $p.MainWindowHandle -ne [IntPtr]::Zero) {
    # SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE: out of sight, same size, no focus change.
    [void][SonShot]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, -25000, 100, 0, 0, 0x15)
    if ($Size -match '^(\d+)x(\d+)$') {
      [void][SonShot]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, -25000, 100, [int]$Matches[1], [int]$Matches[2], 0x14)
    }
    $moved = $true
  }
  Start-Sleep -Milliseconds 40
}
$p.Refresh()
if ($p.HasExited) { Write-Output "EXITED code=$($p.ExitCode)"; exit 1 }
$h = $p.MainWindowHandle
if ($h -eq [IntPtr]::Zero) { Write-Output "NO_WINDOW pid=$($p.Id)"; if (-not $Keep) { $p.Kill() }; exit 2 }
$r = New-Object SonShot+RECT
[void][SonShot]::GetWindowRect($h, [ref]$r)
$w = $r.R - $r.L; $ht = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $ht
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc()
# PW_RENDERFULLCONTENT: captures DirectX-composed content even when not visible on a monitor.
[void][SonShot]::PrintWindow($h, $dc, 2)
$g.ReleaseHdc($dc); $g.Dispose()
$bmp.Save((Join-Path (Get-Location) $Out), [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
$mem = "ws=$([math]::Round($p.WorkingSet64/1MB))MB private=$([math]::Round($p.PrivateMemorySize64/1MB))MB cpu=$([math]::Round($p.TotalProcessorTime.TotalSeconds,1))s"
Write-Output "OK $mem pid=$($p.Id) title='$($p.MainWindowTitle)' size=${w}x${ht} at=$($r.L),$($r.T) out=$Out"
if (-not $Keep) { $p.Kill() }

# Dev tool: launch (or attach to) a windowed exe and save a PNG of its window,
# without showing it to the person at the keyboard.
#   powershell -File scripts/shot.ps1 -Exe target\debug\sonicly.exe -Out shot.png [-Wait 4] [-Keep] [-ExeArgs "..."] [-Profile shot]
#   powershell -File scripts/shot.ps1 -ProcId 1234 -Out shot.png
# -Size 1600x1000 resizes the window (physical pixels) once it is off-screen, to test
# layouts larger than the monitor allows a new window to be.
# -Title "Sonicly mini player" captures the process's window with that exact title instead of
# its main window, and moves that one off-screen too the moment it exists.
# The child gets SONICLY_SHOT=1; Sonicly's binaries honor it by opening their
# window unfocused. Whatever the exe does, the window is moved off-screen the
# moment it exists, and captured there.
param([string]$Exe, [int]$ProcId = 0, [string]$Out = "shot.png", [double]$Wait = 4, [switch]$Keep, [string]$ExeArgs = "", [string]$Size = "", [string]$Profile = "shot", [string]$Title = "")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public static class SonShot {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public delegate bool EnumProc(IntPtr h, IntPtr lp);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lp);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  // The top-level window of process `pid` whose title is exactly `title`, or zero.
  public static IntPtr Titled(int pid, string title) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, lp) => {
      uint owner; GetWindowThreadProcessId(h, out owner);
      if (owner != (uint)pid) return true;
      var text = new StringBuilder(256); GetWindowText(h, text, 256);
      if (text.ToString() != title) return true;
      found = h; return false;
    }, IntPtr.Zero);
    return found;
  }
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
# Time from starting the process to its main window existing, to the loop's 40 ms. Blank for -ProcId.
$clock = [System.Diagnostics.Stopwatch]::StartNew(); $window = ""
if ($Exe) {
  $p = if ($ExeArgs) { Start-Process -FilePath $Exe -ArgumentList $ExeArgs -PassThru } else { Start-Process -FilePath $Exe -PassThru }
} else { $p = Get-Process -Id $ProcId }
$deadline = (Get-Date).AddSeconds($Wait)
$moved = $false
$titled = [IntPtr]::Zero
while ((Get-Date) -lt $deadline) {
  $p.Refresh()
  if ($p.HasExited) { break }
  if ($Title -and $titled -eq [IntPtr]::Zero) {
    $titled = [SonShot]::Titled($p.Id, $Title)
    # Off-screen below the main window, same flags as below: no size change, no focus change.
    if ($titled -ne [IntPtr]::Zero) { [void][SonShot]::SetWindowPos($titled, [IntPtr]::Zero, -25000, 1400, 0, 0, 0x15) }
  }
  if (-not $moved -and $p.MainWindowHandle -ne [IntPtr]::Zero) {
    # SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE: out of sight, same size, no focus change.
    [void][SonShot]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, -25000, 100, 0, 0, 0x15)
    if ($Size -match '^(\d+)x(\d+)$') {
      [void][SonShot]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, -25000, 100, [int]$Matches[1], [int]$Matches[2], 0x14)
    }
    $moved = $true
    if ($Exe) { $window = " window=$($clock.ElapsedMilliseconds)ms" }
  }
  Start-Sleep -Milliseconds 40
}
$p.Refresh()
if ($p.HasExited) { Write-Output "EXITED code=$($p.ExitCode)"; exit 1 }
$h = if ($Title) { [SonShot]::Titled($p.Id, $Title) } else { $p.MainWindowHandle }
if ($h -eq [IntPtr]::Zero) { Write-Output "NO_WINDOW pid=$($p.Id)"; if (-not $Keep) { $p.Kill() }; exit 2 }
# A resize that lands before the app has finished opening can leave it drawing at its first
# size, so the size is nudged once more and given a moment to draw before the capture.
if (-not $Title -and $Size -match '^(\d+)x(\d+)$') {
  [void][SonShot]::SetWindowPos($h, [IntPtr]::Zero, 0, 0, [int]$Matches[1] - 1, [int]$Matches[2], 0x16)
  [void][SonShot]::SetWindowPos($h, [IntPtr]::Zero, 0, 0, [int]$Matches[1], [int]$Matches[2], 0x16)
  Start-Sleep -Milliseconds 800
}
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
$mem = "ws=$([math]::Round($p.WorkingSet64/1MB))MB private=$([math]::Round($p.PrivateMemorySize64/1MB))MB cpu=$([math]::Round($p.TotalProcessorTime.TotalSeconds,1))s$window"
Write-Output "OK $mem pid=$($p.Id) title='$($p.MainWindowTitle)' size=${w}x${ht} at=$($r.L),$($r.T) out=$Out"
if (-not $Keep) { $p.Kill() }

<#
    Copyright (C) 2026 The AiQuokkaWannabe Authors https://github.com/ai-quokka-wannabe

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see https://www.gnu.org/licenses/.
#>

<#
.SYNOPSIS
    Records -Count frames of a window's client area every -IntervalMs into -Out as f0000.png...,
    held in memory until the end so the capture keeps real time (saving as it goes managed ~4 fps).
.EXAMPLE
    .\frames.ps1 -Out "$env:TEMP\f" -Count 80 -IntervalMs 100
#>
param([string]$Out, [int]$Count = 80, [int]$IntervalMs = 100, [string]$Window = "TronGrid Lite - the Grid")
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public struct RECT2 { public int L, T, R, B; }
public static class F {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT2 r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  public static IntPtr Find(string prefix) { IntPtr found = IntPtr.Zero; EnumWindows((h, p) => { var sb = new StringBuilder(256); GetWindowText(h, sb, 256); if (IsWindowVisible(h) && sb.ToString().StartsWith(prefix)) { found = h; return false; } return true; }, IntPtr.Zero); return found; }
}
"@
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force $Out | Out-Null
$win = [F]::Find($Window); $r = New-Object RECT2; [F]::GetClientRect($win, [ref]$r) | Out-Null
$w = $r.R; $h = $r.B
$frames = New-Object System.Collections.Generic.List[System.Drawing.Bitmap]; $sw = [Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt $Count; $i++) {
  $bmp = New-Object System.Drawing.Bitmap $w, $h; $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc(); [F]::PrintWindow($win, $hdc, 3) | Out-Null; $g.ReleaseHdc($hdc)  # 1 = client only, 2 = full content
  $g.Dispose(); $frames.Add($bmp)
  $next = ($i + 1) * $IntervalMs; $wait = $next - $sw.ElapsedMilliseconds; if ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
}
$ms = $sw.ElapsedMilliseconds; for ($i = 0; $i -lt $frames.Count; $i++) { $frames[$i].Save((Join-Path $Out ("f{0:D4}.png" -f $i))); $frames[$i].Dispose() }; "$Count frames $w x $h in $ms ms"


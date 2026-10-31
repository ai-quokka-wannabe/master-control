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
    Drives a live world and looks at it: keys posted to a window, rc-worm's sliders and call button
    set through UI Automation, and any window captured with PrintWindow (it works while covered).

.DESCRIPTION
    -Plan is a comma-separated list of steps, run in order:
      W+ / W-        key down / up (W S A D X E Q), sent to the window titled -KeysTo
      fwd=N, turn=N  set rc-worm's forward / turn slider (-100..100): exact, and it HOLDS - zero it after
      call           press rc-worm's call button
      2.5            sleep that many seconds
    Keys to rc-worm's panel move its sliders, and the sliders hold a course after the key is released;
    prefer fwd= / turn=. Keys to the Grid window fly the spectator camera (W S A D, E up, Q down).
    -Shot saves -Window (default the Grid) as a PNG at -Scale. Windows will not hand an agent's process
    the foreground, so capture never relies on it.
.EXAMPLE
    .\drive.ps1 -Plan "fwd=100,turn=25,4,turn=0,6" -Shot "$env:TEMP\grid.png"
.EXAMPLE
    .\drive.ps1 -KeysTo 'TronGrid Lite - the Grid' -Plan "W+,1.5,W-,Q+,0.3,Q-" -Shot "$env:TEMP\grid.png"
#>
param([string]$Plan = "", [string]$Shot = "", [string]$Window = "TronGrid Lite - the Grid", [double]$Scale = 0.6, [string]$KeysTo = "rc-worm - creature")

Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public struct RECT { public int L, T, R, B; }
public static class D {
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  public static IntPtr Find(string prefix) { IntPtr found = IntPtr.Zero; EnumWindows((h, p) => { var sb = new StringBuilder(256); GetWindowText(h, sb, 256); if (IsWindowVisible(h) && sb.ToString().StartsWith(prefix)) { found = h; return false; } return true; }, IntPtr.Zero); return found; }
}
"@
$panel = [D]::Find($KeysTo)
$vk = @{ W = 0x57; S = 0x53; A = 0x41; D = 0x44; X = 0x58; E = 0x45; Q = 0x51 }
foreach ($step in ($Plan -split ',' | Where-Object { $_ })) {
    $s = $step.Trim()
    if ($s -match '^([WSADXEQ])\+$') { [D]::PostMessage($panel, 0x100, [IntPtr]$vk[$Matches[1]], [IntPtr]1) | Out-Null }
    elseif ($s -match '^([WSADXEQ])-$') { [D]::PostMessage($panel, 0x101, [IntPtr]$vk[$Matches[1]], [IntPtr]0xC0000001) | Out-Null }
    elseif ($s -eq 'call') {
        Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
        $root = [System.Windows.Automation.AutomationElement]::FromHandle($panel)
        $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
        $b = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond) | Where-Object { $_.Current.Name -match '(?i)call' } | Select-Object -First 1
        if ($b) { $b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke() }
    }
    elseif ($s -match '^(fwd|turn)=(-?[0-9.]+)$') {
        Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
        $wp = [D]::Find('rc-worm - creature') # the sliders live on the panel, whatever -KeysTo says
        $root = [System.Windows.Automation.AutomationElement]::FromHandle($wp)
        $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Slider)
        $sl = @($root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond))
        $which = if ($Matches[1] -eq 'fwd') { 0 } else { 1 }
        $sl[$which].GetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern).SetValue([double]$Matches[2])
    }    elseif ($s -match '^[0-9.]+$') { Start-Sleep -Milliseconds ([int]([double]$s * 1000)) }
}
if ($Shot) {
    $win = [D]::Find($Window)
    $r = New-Object RECT; [D]::GetWindowRect($win, [ref]$r) | Out-Null
    Add-Type -AssemblyName System.Drawing
    $w = $r.R - $r.L; $h = $r.B - $r.T
    $bmp = New-Object System.Drawing.Bitmap $w, $h; $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc(); [D]::PrintWindow($win, $hdc, 2) | Out-Null; $g.ReleaseHdc($hdc)
    $small = New-Object System.Drawing.Bitmap $bmp, ([int]($w * $Scale)), ([int]($h * $Scale)); $small.Save($Shot)
    $g.Dispose(); $bmp.Dispose(); $small.Dispose(); "shot $w x $h -> $Shot"
}

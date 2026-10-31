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
    Stops a Master Control the proper way - a stop request, so the tick in hand finishes, the
    log ends with its end line and the Disk closes - by sending Ctrl+Break to its console.

.DESCRIPTION
    Ctrl+C does not work from an agent's shell: a process started from a parent that ignores
    Ctrl+C inherits the ignore flag, and Master Control never hears it. Ctrl+Break is always
    delivered, and Master Control treats it as a stop request. Run this in its own PowerShell
    process: it attaches to the target's console, and the break reaches this helper too (its
    exit code says so; ignore it).

.EXAMPLE
    Start-Process powershell -ArgumentList '-NoProfile','-File','stop-world.ps1','-ProcessId',$mc.Id -WindowStyle Hidden -Wait
#>
param([Parameter(Mandatory = $true)] [int] $ProcessId)
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Console32 {
  [DllImport("kernel32.dll")] public static extern bool FreeConsole();
  [DllImport("kernel32.dll")] public static extern bool AttachConsole(uint p);
  [DllImport("kernel32.dll")] public static extern bool SetConsoleCtrlHandler(IntPtr h, bool add);
  [DllImport("kernel32.dll")] public static extern bool GenerateConsoleCtrlEvent(uint e, uint g);
}
"@
[Console32]::FreeConsole() | Out-Null
if (-not [Console32]::AttachConsole([uint32]$ProcessId)) { exit 2 }
[Console32]::SetConsoleCtrlHandler([IntPtr]::Zero, $true) | Out-Null
[Console32]::GenerateConsoleCtrlEvent(1, 0) | Out-Null # CTRL_BREAK_EVENT
Start-Sleep -Milliseconds 500
exit 0

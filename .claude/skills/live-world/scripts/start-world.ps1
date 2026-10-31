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
    Starts a live world on this desk: Master Control, the Grid's spectator window, and the Grid
    hosting rc-worm with its panel - each in its own window - and writes their process ids.

.DESCRIPTION
    The four clones sit side by side (link, tron-grid-lite, master-control, rc-worm). Binaries must
    be built first (the windows-build skill): master-control\target\release\master-control.exe,
    tron-grid-lite\build\windows-msvc\src\Release\TronGridLite.exe with rc_worm.dll deployed into
    its programs\ folder. Stop the world with stop-world.ps1, never by killing Master Control.

.PARAMETER Root      The folder that holds the four clones. Default: three levels above this script's repo.
.PARAMETER Port      Master Control's port. Default 47200.
.PARAMETER Density   --food-density (orbs per 100 m^2). Default 0.1.
.PARAMETER Log       Path for Master Control's --log (an input log Clu can replay). Optional, recommended.
.PARAMETER NoWindow  Skip the spectator window.
.PARAMETER NoWorm    Skip hosting rc-worm.
#>
param(
    [string] $Root = "",
    [int] $Port = 47200,
    [double] $Density = 0.1,
    [string] $Log = "",
    [switch] $NoWindow,
    [switch] $NoWorm
)
$ErrorActionPreference = 'Stop'
if ($Root -eq "") { $Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..\..')).Path }
$mcExe = Join-Path $Root 'master-control\target\release\master-control.exe'
$grid = Join-Path $Root 'tron-grid-lite\build\windows-msvc\src\Release\TronGridLite.exe'
foreach ($p in $mcExe, $grid) { if (-not (Test-Path $p)) { throw "missing $p - build first (windows-build skill)" } }

$mcArgs = @("$Port", '--food-density', "$Density")
if ($Log -ne "") { $mcArgs += @('--log', "`"$Log`"") }
$mc = Start-Process -FilePath $mcExe -ArgumentList $mcArgs -PassThru
Start-Sleep -Seconds 2
$ids = @{ master_control = $mc.Id }
if (-not $NoWindow) { $ids.window = (Start-Process -FilePath $grid -ArgumentList @("127.0.0.1:$Port", '--window') -PassThru).Id; Start-Sleep -Seconds 3 }
if (-not $NoWorm) { $ids.host = (Start-Process -FilePath $grid -ArgumentList @("127.0.0.1:$Port", '--program', 'rc_worm') -PassThru).Id; Start-Sleep -Seconds 4 }
$ids | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $env:TEMP 'live-world-pids.json')
$ids | ConvertTo-Json

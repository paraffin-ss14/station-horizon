param([switch]$Volumetrics, [switch]$Shadows, [switch]$AmbientOcclusion, [switch]$MaterialDebug, [string]$ShaderPack = "$env:USERPROFILE/Downloads/photon_v1.4.zip", [string]$ScratchRoot = "$PSScriptRoot/../../../work")
$ErrorActionPreference = 'Stop'
$pack = (Resolve-Path -LiteralPath $ShaderPack).Path
$client = Join-Path $PSScriptRoot 'target/debug/station-client.exe'
if (!(Test-Path -LiteralPath $client)) { throw 'Build station-client first (see SHADER-PACKS.md).' }
$run = Join-Path ([IO.Path]::GetFullPath($ScratchRoot)) ('photon-play-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -Force | Out-Null
$env:HORIZON_HOME = Join-Path $run 'home'
$env:HORIZON_LAUNCHER_HOME = Join-Path $run 'launcher'
$env:XDG_DATA_HOME = Join-Path $run 'data'
$env:XDG_CONFIG_HOME = Join-Path $run 'config'
$env:HORIZON_LAUNCHER_NO_SELF_UPDATE = '1'
$env:HORIZON_NO_AUDIO = '1'
if(([int]$Volumetrics.IsPresent + [int]$Shadows.IsPresent + [int]$AmbientOcclusion.IsPresent + [int]$MaterialDebug.IsPresent) -gt 1) {throw 'Choose Volumetrics, Shadows, AmbientOcclusion or MaterialDebug'}
$mode = if($Volumetrics) {'--photon-volumetrics'} elseif($Shadows) {'--photon-shadows'} elseif($AmbientOcclusion) {'--photon-ao'} elseif($MaterialDebug) {'--photon-material-debug'} else {'--shader-pack-final'}
Start-Process -FilePath $client -WorkingDirectory $run -WindowStyle Hidden -ArgumentList @('--content',('"'+(Join-Path $PSScriptRoot 'content/test-station.json')+'"'),$mode,('"'+$pack+'"'))

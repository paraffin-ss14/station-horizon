param([Parameter(Mandatory=$true)][string]$ScratchRoot)
$ErrorActionPreference='Stop'
$scratch=Join-Path ([IO.Path]::GetFullPath($ScratchRoot)) ('sprite-runtime-'+[guid]::NewGuid().ToString('N'))
$evidence=Join-Path $PSScriptRoot 'evidence/sprite-runtime'
$pack=Join-Path $scratch 'pack'
New-Item -ItemType Directory -Path $scratch,$pack,$evidence -Force | Out-Null
$env:HORIZON_HOME=Join-Path $scratch 'home'
$env:HORIZON_LAUNCHER_HOME=Join-Path $scratch 'launcher'
$env:XDG_DATA_HOME=Join-Path $scratch 'data'
$env:XDG_CONFIG_HOME=Join-Path $scratch 'config'
$env:HORIZON_LAUNCHER_NO_SELF_UPDATE='1'
$env:HORIZON_NO_AUDIO='1'
$env:STATION_SPRITE_DIR=$pack
$env:STATION_PROFILE='1'
$args=@('--editor','--editor-sprite-smoke','--content',('"'+(Join-Path $PSScriptRoot 'content/sprite-lab.json')+'"'),'--map-output',('"'+(Join-Path $scratch 'map.json')+'"'),'--editor-smoke',('"'+(Join-Path $evidence 'lab.png')+'"'))
$process=Start-Process -FilePath (Join-Path $PSScriptRoot 'target/debug/station-client.exe') -ArgumentList $args -WorkingDirectory $scratch -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $evidence 'lab.log') -RedirectStandardError (Join-Path $evidence 'lab-error.log')
try {
    if (!$process.WaitForExit(40000)) {throw 'Sprite runtime smoke timed out'}
    $process.Refresh()
    Get-Content (Join-Path $evidence 'lab.log')
    if ($process.ExitCode -ne 0) {Get-Content (Join-Path $evidence 'lab-error.log');throw 'Sprite runtime smoke failed'}
} finally {if (!$process.HasExited) {Stop-Process -Id $process.Id}}

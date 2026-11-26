param([Parameter(Mandatory=$true)][string]$ScratchRoot)
$ErrorActionPreference = 'Stop'
$runRoot = Join-Path ([IO.Path]::GetFullPath($ScratchRoot)) ('lighting-' + [guid]::NewGuid().ToString('N'))
$evidence = Join-Path $PSScriptRoot 'evidence/lighting'
New-Item -ItemType Directory -Path $runRoot,$evidence -Force | Out-Null
$env:HORIZON_HOME = Join-Path $runRoot 'home'
$env:HORIZON_LAUNCHER_HOME = Join-Path $runRoot 'launcher'
$env:XDG_DATA_HOME = Join-Path $runRoot 'data'
$env:XDG_CONFIG_HOME = Join-Path $runRoot 'config'
$env:HORIZON_LAUNCHER_NO_SELF_UPDATE = '1'
$env:HORIZON_NO_AUDIO = '1'
$clientExe = Join-Path $PSScriptRoot 'target/debug/station-client.exe'
$map = Join-Path $evidence 'lit-map.json'
$processes = [Collections.Generic.List[Diagnostics.Process]]::new()
function Start-Checked([string]$exe,[string[]]$arguments,[string]$name,[string]$directory) {
    New-Item -ItemType Directory -Path (Join-Path $directory 'station/evidence') -Force | Out-Null
    $p=Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $directory -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $evidence "$name.log") -RedirectStandardError (Join-Path $evidence "$name-error.log")
    $processes.Add($p)
    return $p
}
function Wait-Checked([Diagnostics.Process]$p,[string]$name) {
    if (!$p.WaitForExit(40000)) { throw "$name timed out" }
    $p.Refresh()
    Get-Content (Join-Path $evidence "$name.log")
    if ($p.ExitCode -ne 0) { Get-Content (Join-Path $evidence "$name-error.log"); throw "$name failed ($($p.ExitCode))" }
}
try {
    $editor=Start-Checked $clientExe @('--editor','--editor-light-smoke','--map-output',('"'+$map+'"'),'--editor-smoke',('"'+(Join-Path $evidence 'editor.png')+'"')) 'editor' (Join-Path $runRoot 'editor')
    Wait-Checked $editor 'editor'
    $listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
    $listener.Start(); $address='127.0.0.1:'+$listener.LocalEndpoint.Port; $listener.Stop()
    $server=Start-Checked (Join-Path $PSScriptRoot 'target/debug/station-server.exe') @('--listen',$address,'--content',('"'+$map+'"')) 'server' (Join-Path $runRoot 'server')
    $engineer=Start-Checked $clientExe @('--connect',$address,'--smoke','--smoke-job','engineer','--smoke-visibility') 'engineer' (Join-Path $runRoot 'engineer')
    $doctor=Start-Checked $clientExe @('--connect',$address,'--smoke','--smoke-job','doctor','--smoke-visibility') 'doctor' (Join-Path $runRoot 'doctor')
    Wait-Checked $engineer 'engineer'
    Wait-Checked $doctor 'doctor'
    foreach($role in @('engineer','doctor')) {
        foreach($state in @('closed','open')) {
            Copy-Item -LiteralPath (Join-Path $runRoot "$role/station/evidence/visibility-$state.png") -Destination (Join-Path $evidence "$role-$state.png")
        }
    }
    Write-Output 'Lighting editor + two-client visibility smoke passed.'
} finally {
    foreach($p in $processes) { if(!$p.HasExited) {Stop-Process -Id $p.Id} }
}

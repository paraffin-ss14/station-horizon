param([Parameter(Mandatory=$true)][string]$ScratchRoot)
$ErrorActionPreference = 'Stop'
$runRoot = Join-Path ([IO.Path]::GetFullPath($ScratchRoot)) ('sprites-' + [guid]::NewGuid().ToString('N'))
$evidence = Join-Path $PSScriptRoot 'evidence/rooms'
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
    $labText=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'content/rooms-lab.json') -Raw
    $cases=@(@('closed',3,0),@('open',2,0),@('upper-hole',1,0),@('wall-breach',2,1),@('space-hole',1,1))
    foreach($case in $cases) {
        $name=$case[0]
        $content=$labText | ConvertFrom-Json -Depth 100
        if($name -ne 'closed') { $content.map.doors=@() }
        $cells=@{}
        foreach($box in $content.map.base) {
            for($x=$box.min[0];$x -le $box.max[0];$x++) {
                for($y=$box.min[1];$y -le $box.max[1];$y++) {
                    for($z=$box.min[2];$z -le $box.max[2];$z++) { $cells["$x,$y,$z"]=@($x,$y,$z) }
                }
            }
        }
        if($name -eq 'wall-breach') { $cells.Remove('-4,0,1');$cells.Remove('-4,0,2') }
        if($name -in @('upper-hole','space-hole')) { $cells.Remove('-2,0,0') }
        if($name -eq 'space-hole') { $cells.Remove('-2,0,-4') }
        $content.map.base=@($cells.Values | ForEach-Object { @{min=$_;max=$_} })
        $fixture=Join-Path $evidence ($name+'.json')
        $content | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $fixture -Encoding utf8
        $preview=Start-Checked $clientExe @('--editor','--content',('"'+$fixture+'"'),'--rooms-preview','--preview-capture',('"'+(Join-Path $evidence ($name+'.png'))+'"')) $name (Join-Path $runRoot $name)
        Wait-Checked $preview $name
        $log=Get-Content -LiteralPath (Join-Path $evidence ($name+'.log')) -Raw
        if($log -notmatch ('rooms preview: regions='+$case[1]+' exposed='+$case[2])) { throw "$name topology mismatch" }
    }
    $editor=Start-Checked $clientExe @('--editor','--content',('"'+(Join-Path $PSScriptRoot 'content/test-station.json')+'"'),'--rooms-preview','--editor-floor-smoke','--map-output',('"'+$map+'"'),'--editor-smoke',('"'+(Join-Path $evidence 'editor.png')+'"')) 'editor' (Join-Path $runRoot 'editor')
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
    Write-Output 'Rooms: five topology previews, editor and two graphical clients passed.'
} finally {
    foreach($p in $processes) { if(!$p.HasExited) {Stop-Process -Id $p.Id} }
}

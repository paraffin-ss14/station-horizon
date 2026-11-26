param([Parameter(Mandatory=$true)][string]$ScratchRoot, [Parameter(Mandatory=$true)][string]$ShaderPack, [string]$SolasPack)
$ErrorActionPreference = 'Stop'
$runRoot = Join-Path ([IO.Path]::GetFullPath($ScratchRoot)) ('shader-pack-' + [guid]::NewGuid().ToString('N'))
$evidence = Join-Path $PSScriptRoot 'evidence/shader-pack'
New-Item -ItemType Directory -Path $runRoot,$evidence -Force | Out-Null
$env:HORIZON_HOME = Join-Path $runRoot 'home'
$env:HORIZON_LAUNCHER_HOME = Join-Path $runRoot 'launcher'
$env:XDG_DATA_HOME = Join-Path $runRoot 'data'
$env:XDG_CONFIG_HOME = Join-Path $runRoot 'config'
$env:HORIZON_LAUNCHER_NO_SELF_UPDATE = '1'
$env:HORIZON_NO_AUDIO = '1'
$clientExe = Join-Path $PSScriptRoot 'target/debug/station-client.exe'
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
    $cases=@('baseline','photon','scene-data','photon-material','photon-ao','photon-shadows','photon-volumetrics','final-slot','missing')
    if($SolasPack) {$cases+= @('solas','solas-post','solas-bloom','solas-dof','solas-autofocus','solas-soft','solas-motion','solas-motion-pan','solas-bloom-pan','solas-chromatic','solas-chromatic-off','solas-taa','solas-taa-pan','solas-taa-cut','solas-taa-object')}
    foreach($name in $cases) {
        $argsList=@('--editor','--content',('"'+(Join-Path $PSScriptRoot 'content/test-station.json')+'"'),'--camera-y','-8','--preview-capture',('"'+(Join-Path $evidence ($name+'.png'))+'"'))
        if($name -eq 'final-slot') {$argsList+=@('--shader-pack-final',('"'+(Join-Path $PSScriptRoot 'render/fixtures/final-slot')+'"'),'--shader-pack-input','colortex7','--shader-pack-encoding','srgb')}
        if($name -in @('solas-bloom','solas-bloom-pan')) {$argsList+=@('--solas-bloom',('"'+([IO.Path]::GetFullPath($SolasPack))+'"'))}
        if($name -in @('solas-taa','solas-taa-pan','solas-taa-cut','solas-taa-object')) {$argsList+=@('--solas-taa',('"'+([IO.Path]::GetFullPath($SolasPack))+'"'))}
        if($name -in @('solas-motion','solas-motion-pan')) {$argsList+=@('--solas-motion',('"'+([IO.Path]::GetFullPath($SolasPack))+'"'))}
        if($name -in @('solas-motion-pan','solas-bloom-pan','solas-taa-cut')) {$argsList+='--preview-motion'}
        if($name -eq 'solas-taa-object') {$argsList+='--preview-object-motion'}
        if($name -eq 'solas-taa-pan') {$argsList+='--preview-slow-motion'}
        if($name -eq 'solas-soft') {$argsList+=@('--solas-bloom',('"'+([IO.Path]::GetFullPath($SolasPack))+'"'),'--solas-settings',('"'+(Join-Path $PSScriptRoot 'solas-presets/soft.json')+'"'))}
        if($name -in @('solas-chromatic','solas-chromatic-off')) {
            $settingsPath=if($name -eq 'solas-chromatic'){'solas-presets/chromatic.json'}else{'render/fixtures/solas-settings/chromatic-off.json'}
            $argsList+=@('--solas-bloom',('"'+([IO.Path]::GetFullPath($SolasPack))+'"'),'--solas-settings',('"'+(Join-Path $PSScriptRoot $settingsPath)+'"'))
        }
        if($name -eq 'solas-dof') {$argsList+=@('--solas-dof',('"'+([IO.Path]::GetFullPath($SolasPack))+'"'))}
        if($name -eq 'solas-autofocus') {$argsList+=@('--solas-autofocus',('"'+([IO.Path]::GetFullPath($SolasPack))+'"'))}
        if($name -eq 'solas-post') {$argsList+=@('--solas-post',('"'+([IO.Path]::GetFullPath($SolasPack))+'"'))}
        if($name -eq 'solas') {$argsList+=@('--shader-pack-final',('"'+([IO.Path]::GetFullPath($SolasPack))+'"'),'--shader-pack-input','colortex1','--shader-pack-encoding','srgb')}
        if($name -eq 'photon') { $argsList += @('--shader-pack-final',('"'+([IO.Path]::GetFullPath($ShaderPack))+'"')) }
        if($name -eq 'photon-volumetrics') { $argsList += @('--photon-volumetrics',('"'+([IO.Path]::GetFullPath($ShaderPack))+'"')) }
        if($name -eq 'photon-shadows') { $argsList += @('--photon-shadows',('"'+([IO.Path]::GetFullPath($ShaderPack))+'"')) }
        if($name -eq 'photon-ao') { $argsList += @('--photon-ao',('"'+([IO.Path]::GetFullPath($ShaderPack))+'"')) }
        if($name -eq 'photon-material') { $argsList += @('--photon-material-debug',('"'+([IO.Path]::GetFullPath($ShaderPack))+'"')) }
        if($name -eq 'scene-data') { $argsList += @('--shader-pack-final',('"'+(Join-Path $PSScriptRoot 'render/fixtures/scene-data')+'"')) }
        if($name -eq 'missing') { $argsList += @('--shader-pack-final',('"'+(Join-Path $runRoot 'absent.zip')+'"')) }
        $preview=Start-Checked $clientExe $argsList $name (Join-Path $runRoot $name)
        Wait-Checked $preview $name
        if($name -in @('photon','scene-data','final-slot','solas') -and !(Select-String -Path (Join-Path $evidence ($name+'-error.log')) -Pattern 'final GLSL' -Quiet)) { throw 'Shader pack did not activate' }
    }
    if (!(Select-String -Path (Join-Path $evidence 'photon-volumetrics-error.log') -Pattern 'Photon volumetric fog' -Quiet)) {throw 'Photon volumetrics did not activate'}
    if (!(Select-String -Path (Join-Path $evidence 'photon-shadows-error.log') -Pattern 'Photon opaque shadows' -Quiet)) {throw 'Photon shadow pass did not activate'}
    if (!(Select-String -Path (Join-Path $evidence 'photon-ao-error.log') -Pattern 'Photon SSAO' -Quiet)) {throw 'Photon AO did not activate'}
    if (!(Select-String -Path (Join-Path $evidence 'photon-material-error.log') -Pattern 'Photon material roundtrip' -Quiet)) {throw 'Photon material bridge did not activate'}
    if (!(Select-String -Path (Join-Path $evidence 'missing-error.log') -Pattern 'Shader pack:' -Quiet)) { throw 'Expected missing-pack diagnostic' }
    foreach($name in @('photon-shadows','photon-volumetrics')) {
        if (!(Select-String -Path (Join-Path $evidence ($name+'-error.log')) -Pattern 'Photon shadow cache: updates=1 reused=[1-9][0-9]*' -Quiet)) {throw "$name did not reuse the static shadow map"}
    }
    if($SolasPack -and !(Select-String -Path (Join-Path $evidence 'solas-post-error.log') -Pattern 'Solas composite14' -Quiet)) {throw 'Solas post chain did not activate'}
    if($SolasPack -and !(Select-String -Path (Join-Path $evidence 'solas-bloom-error.log') -Pattern 'Solas bloom' -Quiet)) {throw 'Solas bloom did not activate'}
    if($SolasPack -and !(Select-String -Path (Join-Path $evidence 'solas-dof-error.log') -Pattern 'Solas DOF' -Quiet)) {throw 'Solas DOF did not activate'}
    if($SolasPack -and !(Select-String -Path (Join-Path $evidence 'solas-autofocus-error.log') -Pattern 'Solas autofocus' -Quiet)) {throw 'Solas autofocus did not activate'}
    if($SolasPack -and !(Select-String -Path (Join-Path $evidence 'solas-soft-error.log') -Pattern 'Solas settings: 6 options' -Quiet)) {throw 'Solas preset did not activate'}
    if($SolasPack -and !(Select-String -Path (Join-Path $evidence 'solas-soft-error.log') -Pattern 'Solas bloom' -Quiet)) {throw 'Solas preset rendering failed'}
    if($SolasPack) {foreach($motionCase in @('solas-motion','solas-motion-pan')) {
        if(!(Select-String -Path (Join-Path $evidence ($motionCase+'-error.log')) -Pattern 'Solas motion blur' -Quiet)) {throw "$motionCase did not activate"}
    }}
    if($SolasPack) {foreach($chromaticCase in @('solas-chromatic','solas-chromatic-off')) {
        $caseLog=Join-Path $evidence ($chromaticCase+'-error.log')
        if(!(Select-String -Path $caseLog -Pattern 'Solas settings: 2 options' -Quiet) -or !(Select-String -Path $caseLog -Pattern 'Solas bloom' -Quiet)) {throw "$chromaticCase did not activate"}
    }}
    if($SolasPack) {foreach($taaCase in @('solas-taa','solas-taa-pan','solas-taa-cut','solas-taa-object')) {
        if(!(Select-String -Path (Join-Path $evidence ($taaCase+'-error.log')) -Pattern 'Solas TAA' -Quiet)) {throw "$taaCase did not activate"}
    }}
    if($SolasPack -and !(Select-String -Path (Join-Path $evidence 'solas-taa-object.log') -Pattern 'TAA object fixture: shifted sprite' -Quiet)) {throw 'Object motion fixture did not execute'}
    $pixelArgs=@($evidence)
    if($SolasPack){$pixelArgs+='--solas'}
    & cargo run --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') -p horizon-station-render --example scene_data_check --locked --offline -- @pixelArgs
    if($LASTEXITCODE -ne 0) {throw 'Native scene GPU pixel check failed'}
    & cargo test --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') -p horizon-station-render gpu_fog_filter --locked --offline -- --ignored
    if($LASTEXITCODE -ne 0) {throw 'Fog filter GPU regression failed'}
    & cargo test --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') -p horizon-station-render gpu_mips_keep_hdr --locked --offline -- --ignored
    if($LASTEXITCODE -ne 0) {throw 'Mipmap GPU regression failed'}
    & cargo test --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') -p horizon-station-render gpu_colortex_ --locked --offline -- --ignored
    if($LASTEXITCODE -ne 0) {throw 'Colortex history GPU regression failed'}
    & cargo test --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') -p horizon-station-render gpu_raster_blend --locked --offline -- --ignored
    if($LASTEXITCODE -ne 0) {throw 'Raster blend/viewport GPU regression failed'}
    & cargo test --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') -p horizon-station-render gpu_solas_focus --locked --offline -- --ignored
    if($LASTEXITCODE -ne 0) {throw 'Solas autofocus GPU regression failed'}
    Get-FileHash -LiteralPath $ShaderPack -Algorithm SHA256 | Format-List | Out-File (Join-Path $evidence 'pack-sha256.txt')
    if($SolasPack) {Get-FileHash -LiteralPath $SolasPack -Algorithm SHA256 | Format-List | Out-File (Join-Path $evidence 'solas-sha256.txt')}
    Write-Output 'Original ZIP final shader executed on the station GPU renderer.'
} finally {
    foreach($p in $processes) { if(!$p.HasExited) {Stop-Process -Id $p.Id} }
}

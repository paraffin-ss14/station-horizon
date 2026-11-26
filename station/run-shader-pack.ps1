param(
    [switch]$SolasPost,
    [switch]$SolasBloom,
    [switch]$SolasDof,
    [switch]$SolasAutofocus,
    [switch]$SolasMotion,
    [switch]$SolasTaa,
    [Parameter(Mandatory=$true)][string]$ShaderPack,
    [ValidatePattern('^colortex([0-9]|1[0-5])$')][string]$ColorInput = 'colortex0',
    [ValidateSet('linear','srgb')][string]$Encoding = 'linear',
    [string]$Settings,
    [string]$ScratchRoot = "$PSScriptRoot/../../../work"
)
$ErrorActionPreference = 'Stop'
if(($SolasPost -or $SolasBloom -or $SolasDof -or $SolasAutofocus -or $SolasMotion -or $SolasTaa) -and ($PSBoundParameters.ContainsKey('ColorInput') -or $PSBoundParameters.ContainsKey('Encoding'))) {throw 'SolasPost defines its own buffer/color contract; omit ColorInput and Encoding.'}
if($Settings -and !($SolasPost -or $SolasBloom -or $SolasDof -or $SolasAutofocus -or $SolasMotion -or $SolasTaa)){throw 'Settings requires a Solas mode'}
if($Settings){$Settings=(Resolve-Path -LiteralPath $Settings).Path}
$pack = (Resolve-Path -LiteralPath $ShaderPack).Path
$client = Join-Path $PSScriptRoot 'target/debug/station-client.exe'
if (!(Test-Path -LiteralPath $client)) {throw 'Build station-client first (see SHADER-PACKS.md).'}
$run = Join-Path ([IO.Path]::GetFullPath($ScratchRoot)) ('shader-pack-play-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -Force | Out-Null
$env:HORIZON_HOME = Join-Path $run 'home'
$env:HORIZON_LAUNCHER_HOME = Join-Path $run 'launcher'
$env:XDG_DATA_HOME = Join-Path $run 'data'
$env:XDG_CONFIG_HOME = Join-Path $run 'config'
$env:HORIZON_LAUNCHER_NO_SELF_UPDATE = '1'
$env:HORIZON_NO_AUDIO = '1'
if(([int]$SolasPost.IsPresent + [int]$SolasBloom.IsPresent + [int]$SolasDof.IsPresent + [int]$SolasAutofocus.IsPresent + [int]$SolasMotion.IsPresent + [int]$SolasTaa.IsPresent) -gt 1){throw 'Choose one Solas mode'}
$mode=if($SolasTaa){'--solas-taa'}elseif($SolasMotion){'--solas-motion'}elseif($SolasAutofocus){'--solas-autofocus'}elseif($SolasDof){'--solas-dof'}elseif($SolasBloom){'--solas-bloom'}elseif($SolasPost) {'--solas-post'} else {'--shader-pack-final'}
$launchArgs=@('--content',('"'+(Join-Path $PSScriptRoot 'content/test-station.json')+'"'),$mode,('"'+$pack+'"'))
if(!$SolasPost -and !$SolasBloom -and !$SolasDof -and !$SolasAutofocus -and !$SolasMotion -and !$SolasTaa){$launchArgs+=@('--shader-pack-input',$ColorInput,'--shader-pack-encoding',$Encoding)}
if($Settings){$launchArgs+=@('--solas-settings',('"'+$Settings+'"'))}
Start-Process -FilePath $client -WorkingDirectory $run -WindowStyle Hidden -ArgumentList $launchArgs

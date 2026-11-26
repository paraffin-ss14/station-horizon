param([Parameter(Mandatory=$true)][string]$RustAnything)
$ErrorActionPreference = 'Stop'
$crate = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../ss14-rooms-rules'))
$metadataPath = Join-Path $crate 'rust-anything.json'
$metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
$metadata.root = Join-Path $PSScriptRoot 'rooms-oracle'
$metadata | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $metadataPath
& $RustAnything record $crate --count 100 --inputs (Join-Path $PSScriptRoot 'rooms-inputs.jsonl')
if ($LASTEXITCODE) { throw 'C# oracle recording failed' }
& $RustAnything build $crate
if ($LASTEXITCODE) { throw 'Rust bridge build failed' }
& $RustAnything gate $crate --json | Set-Content (Join-Path $PSScriptRoot 'ROOMS-GATE.json')
if ($LASTEXITCODE) { throw 'Migration gate failed' }
$report = Get-Content (Join-Path $PSScriptRoot 'ROOMS-GATE.json') -Raw | ConvertFrom-Json
if (!$report.passed) { throw 'Migration gate reported failure' }
Copy-Item (Join-Path $crate '.rust-anything/diff.json') (Join-Path $PSScriptRoot 'ROOMS-DIFF.json')

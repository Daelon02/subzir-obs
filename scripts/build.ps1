param([Parameter(Mandatory)][string]$BundleDirectory)
$ErrorActionPreference = 'Stop'
$bundle = (Resolve-Path -LiteralPath $BundleDirectory).Path
if (-not (Test-Path -LiteralPath (Join-Path $bundle 'app/python/python.exe'))) {
    throw 'Select the extracted PS5-Subtitles bundle.'
}
Push-Location (Join-Path $PSScriptRoot '..')
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed.' }
    $metadata = cargo metadata --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cannot locate Cargo build output.' }
    $target = ($metadata | ConvertFrom-Json).target_directory
    $csc = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
    & $csc /nologo /target:winexe /platform:x64 /optimize+ /codepage:65001 /r:System.Windows.Forms.dll /r:System.Drawing.dll /r:System.Web.Extensions.dll "/out:$bundle/Субтитри PS5.exe" launcher\Launcher.cs
    if ($LASTEXITCODE -ne 0) { throw 'Launcher build failed.' }
    Copy-Item -LiteralPath (Join-Path $target 'release/subtitle-rs.exe') -Destination "$bundle/app/"
    Copy-Item worker/worker.py "$bundle/app/"
    Copy-Item obs/PS5-Subtitles.lua "$bundle/OBS/"
    Copy-Item docs/PLAYER-UA.txt "$bundle/README.txt"
    Copy-Item demo/* "$bundle/app/demo/"
    Copy-Item src/main.rs "$bundle/app/source/src/"
    Copy-Item Cargo.toml,Cargo.lock,launcher/Launcher.cs "$bundle/app/source/"
} finally {
    Pop-Location
}

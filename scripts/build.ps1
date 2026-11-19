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
    & "$PSScriptRoot/build-setup.ps1" -Destination $bundle -LauncherOnly
    Copy-Item -LiteralPath (Join-Path $target 'release/subtitle-rs.exe') -Destination "$bundle/app/"
    Copy-Item worker/worker.py,worker/hy_worker.py,worker/hy-languages.json "$bundle/app/"
    Copy-Item obs/PS5-Subtitles.lua "$bundle/OBS/"
    Copy-Item docs/PLAYER-UA.txt "$bundle/README.txt"
    Copy-Item docs/PLAYER-EN.txt "$bundle/README.en.txt"
    Copy-Item docs/START-HERE.txt "$bundle/"
    $guideDir = New-Item -ItemType Directory -Path "$bundle/docs/images" -Force
    Copy-Item docs/images/launcher-7b-*.png,docs/images/obs-*.jpg,docs/images/god-of-war-obs.jpg,docs/images/god-of-war-hud.jpg $guideDir.FullName
    Copy-Item docs/PLAYER-EN.md,docs/PLAYER-UA.md "$bundle/docs/"
    if (Get-Command ConvertFrom-Markdown -ErrorAction SilentlyContinue) {
        foreach ($code in 'EN','UA') {
            $html = (ConvertFrom-Markdown -Path "docs/PLAYER-$code.md").Html
            $lang = if ($code -eq 'UA') { 'uk' } else { 'en' }
            $page = "<!doctype html><html lang='$lang'><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><title>SubZir</title><style>body{font:17px/1.55 system-ui;max-width:52rem;margin:2rem auto;padding:0 1rem}img{max-width:100%;height:auto}</style><main>$html</main></html>"
            [IO.File]::WriteAllText("$bundle/docs/PLAYER-$code.html",$page,[Text.UTF8Encoding]::new($false))
        }
    }
    Copy-Item launcher/languages.json "$bundle/app/"
    Copy-Item launcher/locales "$bundle/app/" -Recurse -Force
    Copy-Item docs/COMPONENTS.txt,docs/m2m100-LICENSE.txt,docs/unicode-LICENSE.txt,docs/Hy-MT2-7B-LICENSE.txt "$bundle/app/licenses/"
    Copy-Item docs/benchmarks "$bundle/" -Recurse -Force
    Copy-Item LICENSE "$bundle/LICENSE"
    Copy-Item demo/0.gray,demo/lines.txt "$bundle/app/demo/"
    Copy-Item demo/frames20 "$bundle/benchmarks/" -Recurse -Force
    Copy-Item scripts/run20.py "$bundle/benchmarks/"
    Copy-Item scripts/Test-20.cmd "$bundle/"
    New-Item -ItemType Directory -Path "$bundle/app/models" -Force | Out-Null
    Copy-Item src/*.rs "$bundle/app/source/src/"
    Copy-Item Cargo.toml,Cargo.lock,launcher/*.cs "$bundle/app/source/"
} finally {
    Pop-Location
}

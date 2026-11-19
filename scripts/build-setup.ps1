param([Parameter(Mandatory)][string]$Destination,[switch]$LauncherOnly,
    [string]$RepositoryUrl=$env:SUBZIR_REPOSITORY_URL,[string]$LatestUrl=$env:SUBZIR_LATEST_URL)
$ErrorActionPreference = 'Stop'
$out = [IO.Directory]::CreateDirectory($ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Destination)).FullName
$scratch = [IO.Directory]::CreateDirectory((Join-Path ([IO.Path]::GetTempPath()) ('subzir-build-'+[guid]::NewGuid()))).FullName
Push-Location (Join-Path $PSScriptRoot '..')
try {
    if (!$RepositoryUrl) { $RepositoryUrl = (git remote get-url origin) -replace '\.git$','' }
    $repo = [uri]$RepositoryUrl
    if (!$LatestUrl) { $LatestUrl = 'https://api.'+$repo.Host+'/repos'+$repo.AbsolutePath.TrimEnd('/')+'/releases/latest' }
    if ($repo.Scheme -ne 'https' -or ([uri]$LatestUrl).Scheme -ne 'https') {throw 'Set SUBZIR_REPOSITORY_URL and SUBZIR_LATEST_URL to HTTPS addresses.'}
    $channel = Join-Path $scratch 'channel.json'
    @{repository=$RepositoryUrl.TrimEnd('/');latest=$LatestUrl} | ConvertTo-Json | Set-Content $channel -Encoding UTF8
    $version='0.5.0.0'
    $versionFile=Join-Path $scratch 'Version.cs'
    @"
using System.Reflection;
[assembly: AssemblyVersion("$version")]
[assembly: AssemblyFileVersion("$version")]
[assembly: AssemblyProduct("SubZir")]
[assembly: AssemblyDescription("SubZir/0.4.0")]
"@ | Set-Content $versionFile -Encoding UTF8
    $csc = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
    $flags=@('/nologo','/target:winexe','/platform:x64','/optimize+','/codepage:65001','/r:System.Windows.Forms.dll','/r:System.Drawing.dll','/r:System.Web.Extensions.dll',"/resource:$channel,channel")
    $launcher=if($LauncherOnly){Join-Path $out 'SubZir.exe'}else{Join-Path $scratch 'SubZir.exe'}
    $sources=@(Get-ChildItem launcher/*.cs | ForEach-Object FullName)
    & $csc @flags "/out:$launcher" '/resource:launcher/locales/en.json,SubZir.English.json' '/resource:launcher/update-labels.json,update-labels' $versionFile installer\Channel.cs @sources
    if ($LASTEXITCODE) {throw 'Launcher build failed.'}
    if (!$LauncherOnly){
        cargo build --release --locked
        if ($LASTEXITCODE) {throw 'Translation engine build failed.'}
        $metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
        $engine = Join-Path $metadata.target_directory 'release/subtitle-rs.exe'
        & $csc @flags "/out:$out/SubZir-Setup.exe" "/resource:$launcher,launcher" "/resource:$engine,engine" '/resource:worker/worker.py,worker' '/resource:worker/hy_worker.py,hyworker' '/resource:obs/PS5-Subtitles.lua,obs' '/resource:installer/base.json,baseline' '/r:System.IO.Compression.dll' '/r:Microsoft.CSharp.dll' $versionFile installer\Channel.cs installer\Install.cs installer\Setup.cs
        if ($LASTEXITCODE) {throw 'Installer build failed.'}
    }
} finally {Pop-Location; [IO.Directory]::Delete($scratch,$true)}

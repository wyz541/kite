param(
    [switch]$Clean,
    [switch]$SkipKensaku
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSCommandPath
$dist = Join-Path $root 'dist'
$binary = Join-Path $root 'target\release\kite-explorer.exe'
$version = '0.6.65'
$releaseName = "Kite-v$version-windows-x64"
$staging = Join-Path $dist $releaseName
$archive = Join-Path $dist "$releaseName.zip"

Push-Location $root
try {
    if (-not $SkipKensaku -and ([string]::IsNullOrWhiteSpace($env:KENSAKU_CLI_PATH) -or [string]::IsNullOrWhiteSpace($env:KENSAKU_DAEMON_PATH))) {
        python scripts\kensaku\build_sidecars.py
        if ($LASTEXITCODE -ne 0) {
            throw 'Kensaku sidecar build failed'
        }
    }
}
finally {
    Pop-Location
}

Push-Location $root
try {
    if ($Clean) {
        cargo clean
    }

    cargo build --release --locked --features windows-ui
    New-Item -ItemType Directory -Force -Path $dist | Out-Null

    $resolvedDist = [IO.Path]::GetFullPath($dist).TrimEnd('\') + '\'
    $resolvedStaging = [IO.Path]::GetFullPath($staging)
    if (-not $resolvedStaging.StartsWith($resolvedDist, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Staging path is outside dist'
    }
    if (Test-Path -LiteralPath $resolvedStaging) {
        [IO.Directory]::Delete($resolvedStaging, $true)
    }

    New-Item -ItemType Directory -Force -Path $resolvedStaging | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $resolvedStaging 'icons') | Out-Null

    Copy-Item -LiteralPath $binary -Destination (Join-Path $resolvedStaging 'Kite.exe') -Force
    Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $resolvedStaging -Force
    Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $resolvedStaging -Force
    Copy-Item -LiteralPath (Join-Path $root 'assets\kite-icon-master.png') -Destination (Join-Path $resolvedStaging 'icons') -Force

    if (-not $SkipKensaku) {
        $kensakuCli = if ($env:KENSAKU_CLI_PATH) {
            $env:KENSAKU_CLI_PATH
        } else {
            Join-Path $root 'vendor\kensaku\kensaku-cli.exe'
        }
        $kensakuDaemon = if ($env:KENSAKU_DAEMON_PATH) {
            $env:KENSAKU_DAEMON_PATH
        } else {
            Join-Path $root 'vendor\kensaku\kensaku-daemon.exe'
        }
        foreach ($path in @($kensakuCli, $kensakuDaemon)) {
            if (-not (Test-Path -LiteralPath $path)) {
                throw "Kensaku sidecar not found: $path"
            }
        }
        Copy-Item -LiteralPath $kensakuCli -Destination (Join-Path $resolvedStaging 'kensaku-cli.exe') -Force
        Copy-Item -LiteralPath $kensakuDaemon -Destination (Join-Path $resolvedStaging 'kensaku-daemon.exe') -Force
        Copy-Item -LiteralPath (Join-Path $root 'docs\search\kensaku-backend.md') -Destination $resolvedStaging -Force
        Copy-Item -LiteralPath (Join-Path $root 'docs\search\KENSAKU_LICENSE') -Destination $resolvedStaging -Force
    }

    if (Test-Path -LiteralPath $archive) {
        [IO.File]::Delete($archive)
    }
    Compress-Archive -Path (Join-Path $resolvedStaging '*') -DestinationPath $archive -CompressionLevel Optimal

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zipArchive = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $zipEntries = @($zipArchive.Entries.FullName)
    }
    finally {
        $zipArchive.Dispose()
    }
    foreach ($requiredEntry in @('Kite.exe', 'kensaku-cli.exe', 'kensaku-daemon.exe')) {
        if ($zipEntries -notcontains $requiredEntry) {
            throw "Release ZIP is missing $requiredEntry"
        }
    }

    $distBinary = Join-Path $dist 'Kite.exe'
    Copy-Item -LiteralPath $binary -Destination $distBinary -Force

    $exeHash = (Get-FileHash -LiteralPath $distBinary -Algorithm SHA256).Hash.ToLowerInvariant()
    $archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii -Value @(
        "$exeHash  Kite.exe"
        "$archiveHash  $releaseName.zip"
    )

    Write-Output $distBinary
    Write-Output $archive
}
finally {
    Pop-Location
}

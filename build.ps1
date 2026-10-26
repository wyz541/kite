param(
    [switch]$Clean
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSCommandPath
$dist = Join-Path $root 'dist'
$binary = Join-Path $root 'target\release\kite-explorer.exe'
$version = '0.5.0'
$releaseName = "Kite-v$version-windows-x64"
$staging = Join-Path $dist $releaseName
$archive = Join-Path $dist "$releaseName.zip"

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
    New-Item -ItemType Directory -Force -Path (Join-Path $resolvedStaging 'screenshots') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $resolvedStaging 'icons') | Out-Null

    Copy-Item -LiteralPath $binary -Destination (Join-Path $resolvedStaging 'Kite.exe') -Force
    Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $resolvedStaging -Force
    Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $resolvedStaging -Force
    Copy-Item -LiteralPath (Join-Path $root 'assets\kite-icon-master.png') -Destination (Join-Path $resolvedStaging 'icons') -Force
    Copy-Item -Path (Join-Path $root 'docs\screenshots\*.png') -Destination (Join-Path $resolvedStaging 'screenshots') -Force

    if (Test-Path -LiteralPath $archive) {
        [IO.File]::Delete($archive)
    }
    Compress-Archive -Path (Join-Path $resolvedStaging '*') -DestinationPath $archive -CompressionLevel Optimal

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

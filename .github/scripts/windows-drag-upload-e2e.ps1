param(
    [string]$KiteExe = "target\release\kite-explorer.exe",
    [string]$OutputDir = "windows-drag-e2e"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$serverScript = Join-Path $PSScriptRoot "windows-drag-upload-server.py"
if ([System.IO.Path]::IsPathRooted($OutputDir)) {
    $output = $OutputDir
} else {
    $output = Join-Path $root $OutputDir
}
New-Item -ItemType Directory -Force -Path $output | Out-Null
$uploadsLog = Join-Path $output "uploads.jsonl"
$serverOut = Join-Path $output "server.stdout.log"
$serverErr = Join-Path $output "server.stderr.log"
$summaryPath = Join-Path $output "summary.json"
$eventsLog = Join-Path $output "windows-application-errors.log"
$uiaLog = Join-Path $output "uia-tree.json"
$screenshotPath = Join-Path $output "screen.png"
$kiteTraceLog = Join-Path $output "kite-drag-trace.log"
$kiteStateDir = Join-Path $output "kite-state"
$sessionPath = Join-Path $kiteStateDir "session.json"
$uiPreferencesPath = Join-Path $kiteStateDir "ui.json"

if (-not (Test-Path -LiteralPath $KiteExe)) {
    $candidate = Join-Path $root $KiteExe
    if (Test-Path -LiteralPath $candidate) {
        $KiteExe = $candidate
    } else {
        throw "Kite executable not found: $KiteExe"
    }
}
$KiteExe = (Resolve-Path -LiteralPath $KiteExe).Path

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class KiteDragNative {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int command);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int width, int height, bool repaint);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr insertAfter, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT point);
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    public const uint LeftDown = 0x0002;
    public const uint LeftUp = 0x0004;
}
"@
[KiteDragNative]::SetProcessDPIAware() | Out-Null

$trackedProcesses = [System.Collections.Generic.List[int]]::new()
$summary = [ordered]@{
    kite = $KiteExe
    chrome = $false
    edge = $false
    uploads = @()
}
$failure = $null

function Track-Process {
    param([System.Diagnostics.Process]$Process)
    if ($null -ne $Process) {
        $trackedProcesses.Add($Process.Id)
    }
    return $Process
}

function Stop-TrackedProcesses {
    foreach ($processId in $trackedProcesses) {
        & taskkill.exe /PID $processId /T /F *> $null
    }
}

function Stop-TrackedProcess {
    param([System.Diagnostics.Process]$Process)
    if ($null -eq $Process) {
        return
    }
    $trackedProcesses.Remove($Process.Id) | Out-Null
    & taskkill.exe /PID $Process.Id /T /F *> $null
    Start-Sleep -Milliseconds 750
}

function Wait-Until {
    param(
        [scriptblock]$Condition,
        [int]$TimeoutSeconds = 30,
        [string]$FailureMessage = "condition timed out"
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        try {
            $value = & $Condition
            if ($null -ne $value -and $value -ne $false) {
                return $value
            }
        } catch {
            # The UI tree can be temporarily unavailable while the app starts.
        }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $deadline)
    throw $FailureMessage
}

function Wait-ProcessWindow {
    param([System.Diagnostics.Process]$Process, [string]$Name)
    return Wait-Until -TimeoutSeconds 30 -FailureMessage "$Name did not create a window" -Condition {
        $Process.Refresh()
        if ($Process.HasExited) {
            throw "$Name exited with code $($Process.ExitCode)"
        }
        if ($Process.MainWindowHandle -ne [IntPtr]::Zero) {
            return $Process.MainWindowHandle
        }
        return $null
    }
}

function Save-E2EDiagnostics {
    param([int]$ProcessId, [string]$ScreenshotPath = $screenshotPath)
    try {
        Add-Type -AssemblyName System.Drawing
        $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
        $bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
            $bitmap.Save($ScreenshotPath, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $graphics.Dispose()
            $bitmap.Dispose()
        }
    } catch {
        Write-Warning "screen capture failed: $_"
    }
    try {
        $rootElement = [System.Windows.Automation.AutomationElement]::RootElement
        $processCondition = [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
            $ProcessId
        )
        $rows = @()
        foreach ($element in $rootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants, $processCondition)) {
            try {
                $rect = $element.Current.BoundingRectangle
                $rows += [pscustomobject]@{
                    name = $element.Current.Name
                    automation_id = $element.Current.AutomationId
                    control_type = $element.Current.ControlType.ProgrammaticName
                    class_name = $element.Current.ClassName
                    offscreen = $element.Current.IsOffscreen
                    x = $rect.X
                    y = $rect.Y
                    width = $rect.Width
                    height = $rect.Height
                }
            } catch {
            }
        }
        $rows | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $uiaLog -Encoding utf8
    } catch {
        Write-Warning "UIA dump failed: $_"
    }
}

function Find-AutomationWindow {
    param([string]$Title, [int]$TimeoutSeconds = 30)
    $rootElement = [System.Windows.Automation.AutomationElement]::RootElement
    return Wait-Until -TimeoutSeconds $TimeoutSeconds -FailureMessage "window not found: $Title" -Condition {
        $children = $rootElement.FindAll(
            [System.Windows.Automation.TreeScope]::Children,
            [System.Windows.Automation.Condition]::TrueCondition
        )
        foreach ($child in $children) {
            try {
                if ($child.Current.Name -like "*$Title*") {
                    return $child
                }
            } catch {
            }
        }
        return $null
    }
}

function Find-FileRow {
    param([int]$ProcessId, [string]$FileName, [int]$TimeoutSeconds = 30)
    $rootElement = [System.Windows.Automation.AutomationElement]::RootElement
    $processCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
        $ProcessId
    )
    return Wait-Until -TimeoutSeconds $TimeoutSeconds -FailureMessage "file row not found: $FileName" -Condition {
        $elements = $rootElement.FindAll(
            [System.Windows.Automation.TreeScope]::Descendants,
            $processCondition
        )
        $namedMatches = @()
        $fallbackRows = @()
        foreach ($element in $elements) {
            try {
                $rect = $element.Current.BoundingRectangle
                $name_matches = $element.Current.Name -like "*$FileName*"
                $is_list_row = $element.Current.ControlType -eq [System.Windows.Automation.ControlType]::DataItem
                if (($name_matches -or $is_list_row) -and
                    $rect.Width -gt 60 -and
                    $rect.Height -ge 12 -and
                    $rect.Height -le 80 -and
                    -not $element.Current.IsOffscreen) {
                    if ($name_matches) {
                        $namedMatches += $element
                    } elseif ($is_list_row) {
                        $fallbackRows += $element
                    }
                }
            } catch {
            }
        }
        if ($namedMatches.Count -gt 0) {
            return $namedMatches | Sort-Object { $_.Current.BoundingRectangle.Width * $_.Current.BoundingRectangle.Height } -Descending | Select-Object -First 1
        }
        if ($fallbackRows.Count -gt 0) {
            return $fallbackRows | Sort-Object @{ Expression = { $_.Current.BoundingRectangle.X }; Descending = $true }, @{ Expression = { $_.Current.BoundingRectangle.Y }; Descending = $false } | Select-Object -First 1
        }
        return $null
    }
}

function Move-Window {
    param([IntPtr]$Handle, [int]$X, [int]$Y, [int]$Width, [int]$Height)
    [KiteDragNative]::ShowWindow($Handle, 9) | Out-Null
    [KiteDragNative]::MoveWindow($Handle, $X, $Y, $Width, $Height, $true) | Out-Null
    Start-Sleep -Milliseconds 500
}

function Start-Browser {
    param([string]$Name, [string]$Executable, [string]$Url, [int]$X, [int]$Y, [int]$Width, [int]$Height, [string]$Profile)
    $arguments = @(
        "--user-data-dir=$Profile",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-sync",
        "--app=$Url",
        "--window-position=$X,$Y",
        "--window-size=$Width,$Height"
    )
    $process = Start-Process -FilePath $Executable -ArgumentList $arguments -PassThru
    Track-Process $process | Out-Null
    $window = Find-AutomationWindow -Title "Kite Drag Upload Test" -TimeoutSeconds 30
    $handle = [IntPtr]$window.Current.NativeWindowHandle
    Move-Window -Handle $handle -X $X -Y $Y -Width $Width -Height $Height
    [KiteDragNative]::SetWindowPos($handle, [IntPtr](-1), $X, $Y, $Width, $Height, 0x0040) | Out-Null
    return [pscustomobject]@{ Process = $process; Window = $window; Handle = $handle }
}

function Move-NativeCursor {
    param([int]$X, [int]$Y)
    $width = [Math]::Max(2, $script:ScreenWidth)
    $height = [Math]::Max(2, $script:ScreenHeight)
    $absoluteX = [uint32][Math]::Round(($X * 65535.0) / ($width - 1))
    $absoluteY = [uint32][Math]::Round(($Y * 65535.0) / ($height - 1))
    [KiteDragNative]::mouse_event(0x8001, $absoluteX, $absoluteY, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 12
}

function Invoke-NativeDrag {
    param([int]$FromX, [int]$FromY, [int]$ToX, [int]$ToY)
    Move-NativeCursor -X $FromX -Y $FromY
    Start-Sleep -Milliseconds 200
    # Prime activation and selection. The browser may still own foreground focus.
    [KiteDragNative]::mouse_event([KiteDragNative]::LeftDown, 0, 0, 0, [UIntPtr]::Zero)
    [KiteDragNative]::mouse_event([KiteDragNative]::LeftUp, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 350
    [KiteDragNative]::mouse_event([KiteDragNative]::LeftDown, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 150
    for ($step = 1; $step -le 30; $step++) {
        $x = [int]($FromX + (($ToX - $FromX) * $step / 30.0))
        $y = [int]($FromY + (($ToY - $FromY) * $step / 30.0))
        Move-NativeCursor -X $x -Y $y
        Start-Sleep -Milliseconds 13
    }
    Start-Sleep -Milliseconds 300
    $point = [KiteDragNative+POINT]::new()
    [KiteDragNative]::GetCursorPos([ref]$point) | Out-Null
    Write-Host "Cursor before drop: ($($point.X),$($point.Y))"
    [KiteDragNative]::mouse_event([KiteDragNative]::LeftUp, 0, 0, 0, [UIntPtr]::Zero)
}

function Wait-ForUpload {
    param([string]$Browser, [string]$FileName, [string]$ExpectedHash, [System.Diagnostics.Process]$KiteProcess)
    return Wait-Until -TimeoutSeconds 30 -FailureMessage "$Browser upload was not received" -Condition {
        $KiteProcess.Refresh()
        if ($KiteProcess.HasExited) {
            throw "Kite exited with code $($KiteProcess.ExitCode) during $Browser drag"
        }
        if (-not (Test-Path -LiteralPath $uploadsLog)) {
            return $null
        }
        foreach ($line in Get-Content -LiteralPath $uploadsLog) {
            if ([string]::IsNullOrWhiteSpace($line)) {
                continue
            }
            try {
                $record = $line | ConvertFrom-Json
            } catch {
                continue
            }
            if ($record.kind -eq "drag-event") {
                if ($record.type -eq "upload-error") {
                    return [pscustomobject]@{ upload_error = $record.message }
                }
                continue
            }
            if ($record.kind -ne "upload") {
                continue
            }
            if ($record.browser -ne $Browser) {
                continue
            }
            foreach ($file in @($record.files)) {
                if ($file.name -eq $FileName -and $file.sha256 -eq $ExpectedHash) {
                    return $record
                }
            }
        }
        return $null
    }
}

function Test-BrowserUpload {
    param(
        [string]$Name,
        [string]$Executable,
        [int]$Port,
        [string]$FileName,
        [string]$ExpectedHash,
        [System.Diagnostics.Process]$KiteProcess,
        [IntPtr]$KiteHandle,
        $FileRow,
        [int]$BrowserX,
        [int]$BrowserY,
        [int]$BrowserWidth,
        [int]$BrowserHeight,
    [string]$ProfileRoot
)
    $profile = Join-Path $ProfileRoot "$Name-profile"
    $url = "http://127.0.0.1:$Port/?browser=$Name"
    $browser = $null
    $browser = Start-Browser -Name $Name -Executable $Executable -Url $url `
        -X $BrowserX -Y $BrowserY -Width $BrowserWidth -Height $BrowserHeight -Profile $profile
    try {
        [KiteDragNative]::ShowWindow($KiteHandle, 9) | Out-Null
        [KiteDragNative]::SetForegroundWindow($KiteHandle) | Out-Null
        Start-Sleep -Milliseconds 500
        $row = $null
        try {
            $row = Find-FileRow -ProcessId $KiteProcess.Id -FileName $FileName -TimeoutSeconds 8
        } catch {
            Write-Warning "UIA file row unavailable; using controlled Kite column coordinates"
        }
        if ($null -ne $row) {
            $rect = $row.Current.BoundingRectangle
            $fromX = [int]($rect.Left + ($rect.Width / 2))
            $fromY = [int]($rect.Top + ($rect.Height / 2))
        } else {
            $kiteWindow = [System.Windows.Automation.AutomationElement]::FromHandle($KiteHandle)
            $rect = $kiteWindow.Current.BoundingRectangle
            # The E2E directory contains one file in the rightmost active column.
            $fromX = [int]($rect.Left + ($rect.Width * 0.75))
            $fromY = [int]($rect.Top + 211)
        }
        $toX = [int]($BrowserX + ($BrowserWidth / 2))
        $toY = [int]($BrowserY + ($BrowserHeight / 2))
        Write-Host "Dragging $FileName from ($fromX,$fromY) to $Name ($toX,$toY)"
        Save-E2EDiagnostics -ProcessId $KiteProcess.Id -ScreenshotPath (Join-Path $output "before-$Name.png")
        Invoke-NativeDrag -FromX $fromX -FromY $fromY -ToX $toX -ToY $toY
        $record = Wait-ForUpload -Browser $Name -FileName $FileName -ExpectedHash $ExpectedHash -KiteProcess $KiteProcess
        if ($record.PSObject.Properties.Name -contains "upload_error") {
            throw "$Name reported upload error: $($record.upload_error)"
        }
        Write-Host "$Name upload verified: $($record.files[0].name) ($($record.files[0].size) bytes)"
        return $record
    } finally {
        Stop-TrackedProcess -Process $browser.Process
    }
}

try {
    $testRoot = [System.IO.Path]::GetPathRoot($env:RUNNER_TEMP)
    $testDir = Join-Path $testRoot "kite-drag-e2e-$([Guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force -Path $testDir | Out-Null
    $fileName = "drag-test.txt"
    $filePath = Join-Path $testDir $fileName
    $content = "Kite Chrome and Edge drag upload test`r`n"
    [System.IO.File]::WriteAllText($filePath, $content, [System.Text.UTF8Encoding]::new($false))
    $expectedHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
    New-Item -ItemType Directory -Force -Path $kiteStateDir | Out-Null
    $session = @{
        schema_version = 1
        active_tab = 0
        tabs = @(
            @{
                current_directory = $testDir
                view_mode = "columns"
                show_path_bar = $true
            }
        )
    } | ConvertTo-Json -Depth 5
    [System.IO.File]::WriteAllText($sessionPath, $session, [System.Text.UTF8Encoding]::new($false))
    $env:KITE_SESSION_PATH = $sessionPath
    $env:KITE_UI_PREFERENCES_PATH = $uiPreferencesPath

    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    $listener.Stop()

    $python = (Get-Command python).Source
    $server = Start-Process -FilePath $python -ArgumentList @("-u", $serverScript, "--port", $port, "--log", $uploadsLog) `
        -RedirectStandardOutput $serverOut -RedirectStandardError $serverErr -PassThru
    Track-Process $server | Out-Null
    Wait-Until -TimeoutSeconds 20 -FailureMessage "upload server did not start" -Condition {
        try {
            $response = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$port/health" -TimeoutSec 2
            return $response.StatusCode -eq 200
        } catch {
            return $false
        }
    } | Out-Null

    $env:KITE_DRAG_TRACE = $kiteTraceLog
    $kite = Start-Process -FilePath $KiteExe -PassThru
    Track-Process $kite | Out-Null
    $kiteHandle = Wait-ProcessWindow -Process $kite -Name "Kite"
    $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $script:ScreenWidth = $screen.Width
    $script:ScreenHeight = $screen.Height
    $windowHeight = [Math]::Max(600, $screen.Height - 80)
    $browserWidth = 200
    $browserHeight = $windowHeight
    $browserX = $screen.Width - $browserWidth
    $browserY = 0
    $kiteWidth = $screen.Width - $browserWidth
    $profileRoot = Join-Path $env:RUNNER_TEMP "kite-drag-browser-profiles-$([Guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force -Path $profileRoot | Out-Null
    Move-Window -Handle $kiteHandle -X 0 -Y 0 -Width $kiteWidth -Height $windowHeight

    $chromeCandidates = @(
        @(
            (Join-Path $env:ProgramFiles "Google\Chrome\Application\chrome.exe"),
            (Join-Path ${env:ProgramFiles(x86)} "Google\Chrome\Application\chrome.exe"),
            (Join-Path $env:LOCALAPPDATA "Google\Chrome\Application\chrome.exe")
        ) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
    )
    if ($chromeCandidates.Count -eq 0) {
        throw "Chrome executable not found"
    }

    $edgeCandidates = @(
        @(
            (Join-Path ${env:ProgramFiles(x86)} "Microsoft\Edge\Application\msedge.exe"),
            (Join-Path $env:ProgramFiles "Microsoft\Edge\Application\msedge.exe"),
            (Join-Path $env:LOCALAPPDATA "Microsoft\Edge\Application\msedge.exe")
        ) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
    )
    if ($edgeCandidates.Count -eq 0) {
        throw "Edge executable not found"
    }

    $chromeRecord = Test-BrowserUpload -Name "chrome" -Executable $chromeCandidates[0] -Port $port `
        -FileName $fileName -ExpectedHash $expectedHash -KiteProcess $kite -KiteHandle $kiteHandle `
        -BrowserX $browserX -BrowserY $browserY -BrowserWidth $browserWidth -BrowserHeight $browserHeight -ProfileRoot $profileRoot
    $summary.chrome = $true
    $summary.uploads += $chromeRecord

    Stop-TrackedProcess -Process $kite
    $env:KITE_DRAG_TRACE = $kiteTraceLog
    $kite = Start-Process -FilePath $KiteExe -PassThru
    Track-Process $kite | Out-Null
    $kiteHandle = Wait-ProcessWindow -Process $kite -Name "Kite"
    Move-Window -Handle $kiteHandle -X 0 -Y 0 -Width $kiteWidth -Height $windowHeight

    $edgeRecord = Test-BrowserUpload -Name "edge" -Executable $edgeCandidates[0] -Port $port `
        -FileName $fileName -ExpectedHash $expectedHash -KiteProcess $kite -KiteHandle $kiteHandle `
        -BrowserX $browserX -BrowserY $browserY -BrowserWidth $browserWidth -BrowserHeight $browserHeight -ProfileRoot $profileRoot
    $summary.edge = $true
    $summary.uploads += $edgeRecord

    $kite.Refresh()
    if ($kite.HasExited) {
        throw "Kite exited with code $($kite.ExitCode) after drag tests"
    }
} catch {
    $failure = $_
    if ($null -ne $kite -and -not $kite.HasExited) {
        Save-E2EDiagnostics -ProcessId $kite.Id
    }
    Write-Error $_
} finally {
    try {
        if (Test-Path -LiteralPath $eventsLog) {
            Remove-Item -LiteralPath $eventsLog -Force
        }
        Get-WinEvent -FilterHashtable @{ LogName = "Application"; StartTime = (Get-Date).AddMinutes(-10) } -ErrorAction SilentlyContinue |
            Where-Object { $_.ProviderName -in @("Application Error", "Windows Error Reporting") -and $_.Message -match "Kite" } |
            Format-List TimeCreated, ProviderName, Id, LevelDisplayName, Message |
            Out-File -FilePath $eventsLog -Encoding utf8
    } catch {
    }
    if ($null -ne $failure) {
        $summary["failure"] = $failure.Exception.Message
        try {
            $summary["file_exists_after"] = Test-Path -LiteralPath $filePath
            $summary["test_directory_items"] = @(Get-ChildItem -LiteralPath $testDir -Force | Select-Object -ExpandProperty Name)
        } catch {
        }
    }
    $summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $summaryPath -Encoding utf8
    Stop-TrackedProcesses
}

if ($null -ne $failure) {
    Get-Content -LiteralPath $summaryPath -ErrorAction SilentlyContinue | Write-Host
    exit 1
}

Write-Host "Chrome and Edge drag upload E2E passed."

#Requires -RunAsAdministrator
[CmdletBinding()]
param(
    [string]$Executable,
    [string]$Config
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$projectDir = Split-Path $PSScriptRoot -Parent
if (-not $Executable) {
    $Executable = Join-Path $projectDir 'target\release\shu-net-timer.exe'
}
if (-not $Config) {
    $Config = Join-Path $projectDir 'config.toml'
}
$taskName = 'SHU Net Keeper (Ethernet)'
$sourceExecutable = (Resolve-Path -LiteralPath $Executable).Path
$sourceConfig = (Resolve-Path -LiteralPath $Config).Path
$programData = [Environment]::GetFolderPath('CommonApplicationData')
$installDir = Join-Path $programData 'SHUNetTimer'
$installedExecutable = Join-Path $installDir 'shu-net-timer.exe'
$installedConfig = Join-Path $installDir 'config.toml'
$logsDir = Join-Path $installDir 'logs'
$currentUser = [Security.Principal.WindowsIdentity]::GetCurrent().Name

# Stop every task-owned instance before deployment. Also terminate a detached instance
# only when its executable path exactly matches the installed executable.
$existingTask = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
if ($existingTask) {
    Disable-ScheduledTask -TaskName $taskName -ErrorAction Stop | Out-Null
    Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
}

$installedFullPath = [IO.Path]::GetFullPath($installedExecutable)
$installedProcesses = @(Get-CimInstance Win32_Process -Filter "Name = 'shu-net-timer.exe'" -ErrorAction SilentlyContinue |
    Where-Object {
        $_.ExecutablePath -and
        [IO.Path]::GetFullPath($_.ExecutablePath).Equals(
            $installedFullPath,
            [StringComparison]::OrdinalIgnoreCase
        )
    }
)
foreach ($process in $installedProcesses) {
    Stop-Process -Id $process.ProcessId -Force -ErrorAction Stop
}

$deadline = (Get-Date).AddSeconds(10)
do {
    $remainingProcesses = @(Get-CimInstance Win32_Process -Filter "Name = 'shu-net-timer.exe'" -ErrorAction SilentlyContinue |
        Where-Object {
            $_.ExecutablePath -and
            [IO.Path]::GetFullPath($_.ExecutablePath).Equals(
                $installedFullPath,
                [StringComparison]::OrdinalIgnoreCase
            )
        })
    if ($remainingProcesses.Count -eq 0) { break }
    Start-Sleep -Milliseconds 200
} while ((Get-Date) -lt $deadline)

if ($remainingProcesses.Count -gt 0) {
    $processIds = ($remainingProcesses.ProcessId -join ', ')
    throw "Installed executable is still running (PID: $processIds); cannot update it."
}

if ($existingTask) {
    $taskAfterStop = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    if ($taskAfterStop -and $taskAfterStop.State -eq 'Running') {
        throw "Scheduled task '$taskName' is still running; cannot update it."
    }
}

New-Item -ItemType Directory -Path $installDir -Force | Out-Null

# A previous installation intentionally leaves the interactive user with read-only
# access. Restore deployment rights before replacing files, then restrict them again
# after the copy. Taking ownership also repairs installations created with an older ACL.
& takeown.exe /F $installDir /R /D Y | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Failed to take ownership of the install directory.' }
& icacls.exe $installDir /grant:r "${currentUser}:(OI)(CI)(F)" '*S-1-5-18:(OI)(CI)(F)' '*S-1-5-32-544:(OI)(CI)(F)' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Failed to prepare the install directory for deployment.' }

# Older versions removed inheritance recursively and could leave a child file with an
# empty ACL. Re-enable inheritance from the now-writable deployment directory first.
$installedItems = @(Get-ChildItem -LiteralPath $installDir -Force -Recurse -ErrorAction Stop |
    Sort-Object { $_.FullName.Length })
foreach ($item in $installedItems) {
    & icacls.exe $item.FullName /inheritance:e | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to restore permissions for '$($item.FullName)'." }
}

New-Item -ItemType Directory -Path $logsDir -Force | Out-Null

function Copy-DeploymentFile {
    param(
        [Parameter(Mandatory)] [string]$Source,
        [Parameter(Mandatory)] [string]$Destination
    )

    $lastError = $null
    for ($attempt = 1; $attempt -le 10; $attempt++) {
        try {
            Copy-Item -LiteralPath $Source -Destination $Destination -Force
            return
        } catch {
            $lastError = $_
            if ($attempt -lt 10) { Start-Sleep -Milliseconds 500 }
        }
    }

    throw "Failed to copy '$Source' to '$Destination' after 10 attempts. Last error: $($lastError.Exception.Message)"
}

Copy-DeploymentFile -Source $sourceExecutable -Destination $installedExecutable
Copy-DeploymentFile -Source $sourceConfig -Destination $installedConfig

# Restrict the deployment directory. SYSTEM needs write access for logs.
& icacls.exe $installDir /inheritance:r /grant:r "${currentUser}:(OI)(CI)(RX)" '*S-1-5-18:(OI)(CI)(F)' '*S-1-5-32-544:(OI)(CI)(F)' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Failed to restrict access to the install directory.' }

# Reset child ACLs to the protected directory defaults. This both removes stale explicit
# entries and guarantees that the executable and logs remain readable after deployment.
$installedItems = @(Get-ChildItem -LiteralPath $installDir -Force -Recurse -ErrorAction Stop |
    Sort-Object { $_.FullName.Length })
foreach ($item in $installedItems) {
    & icacls.exe $item.FullName /reset | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to restrict permissions for '$($item.FullName)'." }
}

& icacls.exe $installedConfig /grant:r "${currentUser}:(M)" '*S-1-5-18:(F)' '*S-1-5-32-544:(F)' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Failed to restrict access to the installed config.' }

$action = New-ScheduledTaskAction `
    -Execute $installedExecutable `
    -Argument ('--config "{0}"' -f $installedConfig) `
    -WorkingDirectory $installDir

# Run once immediately after registration. The repeating trigger starts 30 minutes
# later and omits Duration, which Task Scheduler defines as repeating indefinitely.
$firstRun = (Get-Date).AddMinutes(30)
$continuous = New-ScheduledTaskTrigger -Once -At $firstRun `
    -RepetitionInterval (New-TimeSpan -Minutes 30)
$startup = New-ScheduledTaskTrigger -AtStartup
$startup.Delay = 'PT1M'

$settings = New-ScheduledTaskSettingsSet `
    -StartWhenAvailable `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -MultipleInstances IgnoreNew `
    -RestartCount 5 `
    -RestartInterval (New-TimeSpan -Minutes 2) `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 5)

$principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest

Register-ScheduledTask `
    -TaskName $taskName `
    -Action $action `
    -Trigger @($continuous, $startup) `
    -Settings $settings `
    -Principal $principal `
    -Description 'Check and restore SHU Ethernet connectivity every 30 minutes and after Windows starts.' `
    -Force `
    -ErrorAction Stop | Out-Null

Enable-ScheduledTask -TaskName $taskName -ErrorAction Stop | Out-Null
Start-ScheduledTask -TaskName $taskName -ErrorAction Stop

Write-Host "Scheduled task installed: $taskName"
Write-Host "Install directory: $installDir"
Write-Host "Executable: $installedExecutable"
Write-Host "Config: $installedConfig"
Write-Host "Logs: $logsDir"
Write-Host 'Initial network check: started immediately'
Write-Host "Next scheduled run: $firstRun"
Write-Host 'Repeat interval: 30 minutes (indefinitely)'
Write-Host 'Run this command to start another check immediately:'
Write-Host "Start-ScheduledTask -TaskName '$taskName'"

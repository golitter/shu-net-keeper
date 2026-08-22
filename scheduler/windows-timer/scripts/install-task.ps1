#Requires -RunAsAdministrator
[CmdletBinding()]
param(
    [string]$Executable,
    [string]$Config
)

$ErrorActionPreference = 'Stop'
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

# Stop an existing task before replacing its executable, and wait for the process to exit
# so the file copy below does not race with a still-running instance.
$existingTask = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
if ($existingTask -and $existingTask.State -eq 'Running') {
    Stop-ScheduledTask -TaskName $taskName
    $deadline = (Get-Date).AddSeconds(10)
    while ((Get-Date) -lt $deadline) {
        $state = (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue).State
        if ($state -ne 'Running') { break }
        Start-Sleep -Milliseconds 200
    }
    if ((Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue).State -eq 'Running') {
        throw "Scheduled task '$taskName' is still running; cannot replace its executable."
    }
}

New-Item -ItemType Directory -Path $installDir -Force | Out-Null
Copy-Item -LiteralPath $sourceExecutable -Destination $installedExecutable -Force
Copy-Item -LiteralPath $sourceConfig -Destination $installedConfig -Force

# Restrict the deployment directory. SYSTEM needs write access for logs.
$currentUser = "${env:USERDOMAIN}\${env:USERNAME}"
& icacls.exe $installDir /inheritance:r /grant:r "${currentUser}:(OI)(CI)(RX)" '*S-1-5-18:(OI)(CI)(F)' '*S-1-5-32-544:(OI)(CI)(F)' /T | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Failed to restrict access to the install directory.' }
& icacls.exe $installedConfig /grant:r "${currentUser}:(M)" '*S-1-5-18:(F)' '*S-1-5-32-544:(F)' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Failed to restrict access to the installed config.' }

$action = New-ScheduledTaskAction `
    -Execute $installedExecutable `
    -Argument ('--config "{0}"' -f $installedConfig) `
    -WorkingDirectory $installDir

# Start one minute after installation, then repeat every 30 minutes forever.
$firstRun = (Get-Date).AddMinutes(1)
$continuous = New-ScheduledTaskTrigger -Once -At $firstRun `
    -RepetitionInterval (New-TimeSpan -Minutes 30) `
    -RepetitionDuration ([TimeSpan]::MaxValue)
$startup = New-ScheduledTaskTrigger -AtStartup
$startup.Delay = 'PT1M'

$settings = New-ScheduledTaskSettingsSet `
    -StartWhenAvailable `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
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
    -Force | Out-Null

Write-Host "Scheduled task installed: $taskName"
Write-Host "Install directory: $installDir"
Write-Host "Executable: $installedExecutable"
Write-Host "Config: $installedConfig"
Write-Host "Logs: $(Join-Path $installDir 'logs')"
Write-Host "First scheduled run: $firstRun"
Write-Host 'Repeat interval: 30 minutes (indefinitely)'
Write-Host 'Run this command to start it immediately:'
Write-Host "Start-ScheduledTask -TaskName '$taskName'"

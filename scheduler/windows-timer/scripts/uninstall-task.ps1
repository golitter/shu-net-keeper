#Requires -RunAsAdministrator
[CmdletBinding()]
param(
    [switch]$RemoveFiles
)

$ErrorActionPreference = 'Stop'
$taskName = 'SHU Net Keeper (Ethernet)'
$programData = [Environment]::GetFolderPath('CommonApplicationData')
$installDir = Join-Path $programData 'SHUNetTimer'

if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) {
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    Write-Host "Scheduled task removed: $taskName"
} else {
    Write-Host "Scheduled task does not exist: $taskName"
}

if ($RemoveFiles) {
    $expectedPath = [IO.Path]::GetFullPath((Join-Path $programData 'SHUNetTimer'))
    $resolvedInstallPath = [IO.Path]::GetFullPath($installDir)
    if ($resolvedInstallPath -ne $expectedPath) {
        throw "Refusing to remove unexpected path: $resolvedInstallPath"
    }
    if (Test-Path -LiteralPath $resolvedInstallPath) {
        Remove-Item -LiteralPath $resolvedInstallPath -Recurse -Force
        Write-Host "Installed files and logs removed: $resolvedInstallPath"
    } else {
        Write-Host "Install directory does not exist: $resolvedInstallPath"
    }
} else {
    Write-Host "Installed files were kept: $installDir"
    Write-Host 'Run uninstall-task.ps1 -RemoveFiles to remove the executable, config and logs.'
}

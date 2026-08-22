[CmdletBinding()]
param(
    [string]$Executable,
    [string]$Config
)

$ErrorActionPreference = 'Stop'
$projectDir = Split-Path $PSScriptRoot -Parent
$manifest = Join-Path $projectDir 'Cargo.toml'
if (-not $Executable) {
    $Executable = Join-Path $projectDir 'target\release\shu-net-timer.exe'
}
if (-not $Config) {
    $Config = Join-Path $projectDir 'config.toml'
}

if (-not (Test-Path -LiteralPath $Config -PathType Leaf)) {
    Write-Host 'config.toml was not found. Run these commands first:' -ForegroundColor Yellow
    Write-Host "  Copy-Item '$projectDir\config.example.toml' '$projectDir\config.toml'"
    Write-Host "  notepad '$projectDir\config.toml'"
    exit 2
}

if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    Write-Host 'Release executable was not found. Building it now...' -ForegroundColor Cyan
    & cargo.exe build --manifest-path $manifest --release
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Cargo build failed with exit code $LASTEXITCODE." -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

$executablePath = (Resolve-Path -LiteralPath $Executable).Path
$configPath = (Resolve-Path -LiteralPath $Config).Path

Write-Host 'Running one network check now. Scheduled tasks will not be changed.' -ForegroundColor Cyan
Write-Host "Executable: $executablePath"
Write-Host "Config: $configPath"
Write-Host ''

& $executablePath --config $configPath
$programExitCode = $LASTEXITCODE

Write-Host ''
if ($programExitCode -eq 0) {
    Write-Host 'Test passed: Ethernet is online.' -ForegroundColor Green
} else {
    Write-Host "Test failed with exit code $programExitCode. Check the error above and the log." -ForegroundColor Red
}

$logDir = Join-Path (Split-Path $executablePath -Parent) 'logs'
Write-Host "Log directory: $logDir"
exit $programExitCode

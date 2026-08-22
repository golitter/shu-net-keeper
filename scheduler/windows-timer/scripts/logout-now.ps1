[CmdletBinding()]
param(
    [string]$Config,
    [switch]$CheckOnly,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$projectDir = Split-Path $PSScriptRoot -Parent
if (-not $Config) {
    $Config = Join-Path $projectDir 'config.toml'
}
if (-not (Test-Path -LiteralPath $Config -PathType Leaf)) {
    throw "config.toml was not found: $Config"
}

$configPath = (Resolve-Path -LiteralPath $Config).Path
$configText = Get-Content -LiteralPath $configPath -Raw -Encoding UTF8
$adapterMatch = [regex]::Match($configText, '(?m)^\s*adapter_name\s*=\s*"([^"]+)"')
if (-not $adapterMatch.Success) {
    throw 'adapter_name was not found in config.toml.'
}
$adapterName = $adapterMatch.Groups[1].Value

$adapter = Get-NetAdapter -Name $adapterName -ErrorAction Stop
if ($adapter.Status -ne 'Up') {
    throw "Adapter is not connected: $adapterName"
}
$localIp = Get-NetIPAddress -InterfaceIndex $adapter.ifIndex -AddressFamily IPv4 |
    Where-Object { $_.IPAddress -notlike '169.254.*' } |
    Select-Object -First 1 -ExpandProperty IPAddress
if (-not $localIp) {
    throw "Adapter has no usable IPv4 address: $adapterName"
}

$systemRoot = [Environment]::GetFolderPath('Windows')
$curl = Join-Path $systemRoot 'System32\curl.exe'
if (-not (Test-Path -LiteralPath $curl -PathType Leaf)) {
    throw "System curl.exe was not found: $curl"
}

$infoUrl = 'http://10.10.9.9/eportal/InterFace.do?method=getOnlineUserInfo'
$logoutUrl = 'http://10.10.9.9/eportal/InterFace.do?method=logout'
$commonArgs = @(
    '--disable',
    '--silent',
    '--show-error',
    '--fail',
    '--noproxy', '*',
    '--interface', $localIp,
    '--connect-timeout', '10',
    '--max-time', '10'
)

function Invoke-PortalJson {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Url,
        [string[]]$ExtraArguments = @()
    )

    # Windows PowerShell 5.1 decodes native stdout with the current code page.
    # The portal returns UTF-8 JSON, so preserve the raw bytes in a file and
    # decode them explicitly as UTF-8 before calling ConvertFrom-Json.
    $responseFile = Join-Path ([IO.Path]::GetTempPath()) ('shu-net-timer-response-' + [guid]::NewGuid().ToString('N') + '.json')
    try {
        $arguments = @($commonArgs) + @($ExtraArguments) + @('--output', $responseFile, $Url)
        & $curl @arguments
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            throw "Portal request failed. curl exit code: $exitCode"
        }
        $responseText = [IO.File]::ReadAllText($responseFile, [Text.Encoding]::UTF8)
        return $responseText | ConvertFrom-Json
    } finally {
        if (Test-Path -LiteralPath $responseFile) {
            Remove-Item -LiteralPath $responseFile -Force
        }
    }
}

$online = Invoke-PortalJson -Url $infoUrl
if (-not $online.userIndex) {
    Write-Host 'No active SHU portal session was found.' -ForegroundColor Yellow
    exit 3
}

Write-Host "Adapter: $adapterName"
Write-Host "Ethernet IPv4: $localIp"
Write-Host 'An active SHU portal session was found.'
if ($CheckOnly) {
    Write-Host 'Check-only mode: no logout request was sent.' -ForegroundColor Green
    exit 0
}
if (-not $Force) {
    $answer = Read-Host 'This will disconnect the current campus network session. Type YES to continue'
    if ($answer -cne 'YES') {
        Write-Host 'Logout cancelled.'
        exit 0
    }
}

$form = 'userIndex=' + [uri]::EscapeDataString([string]$online.userIndex)
$formFile = Join-Path ([IO.Path]::GetTempPath()) ('shu-net-timer-logout-' + [guid]::NewGuid().ToString('N') + '.txt')
try {
    [IO.File]::WriteAllText($formFile, $form, [Text.Encoding]::ASCII)
    $logout = Invoke-PortalJson -Url $logoutUrl -ExtraArguments @(
        '--header', 'Content-Type: application/x-www-form-urlencoded',
        '--data-binary', ("@$formFile")
    )
} finally {
    if (Test-Path -LiteralPath $formFile) {
        Remove-Item -LiteralPath $formFile -Force
    }
}

if ($logout.result -ne 'success') {
    $message = if ($logout.message) { [string]$logout.message } else { 'Unknown portal error' }
    throw "Portal rejected the logout request: $message"
}

Start-Sleep -Seconds 1
$verify = Invoke-PortalJson -Url $infoUrl
if ($verify.userIndex -and $verify.result -ne 'fail') {
    throw 'Portal returned success, but the session still appears to be active.'
}

Write-Host 'Logout succeeded. The Ethernet link remains connected, but portal authentication is now offline.' -ForegroundColor Green
Write-Host 'Run scripts\test-now.ps1 to test automatic login.'

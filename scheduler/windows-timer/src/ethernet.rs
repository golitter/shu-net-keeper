use std::net::Ipv4Addr;
use std::process::Command;

const INTERNET_TEST_URL: &str = "http://www.msftconnecttest.com/connecttest.txt";
const INTERNET_TEST_BODY: &str = "Microsoft Connect Test";

pub fn ipv4_for_adapter(adapter_name: &str) -> Result<Ipv4Addr, Box<dyn std::error::Error>> {
    if !cfg!(windows) {
        return Err("此程序只支持 Windows".into());
    }

    let escaped = adapter_name.replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; \
         [Console]::OutputEncoding=[Text.Encoding]::UTF8; \
         $a=Get-NetAdapter -Name '{escaped}'; \
         if($a.Status -ne 'Up'){{throw '指定的以太网适配器未连接'}}; \
         $ip=Get-NetIPAddress -InterfaceIndex $a.ifIndex -AddressFamily IPv4 | \
             Where-Object {{$_.IPAddress -notlike '169.254.*'}} | \
             Select-Object -First 1 -ExpandProperty IPAddress; \
         if(-not $ip){{throw '指定的以太网适配器没有可用 IPv4 地址'}}; \
         Write-Output $ip"
    );

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("无法启动 PowerShell: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!("无法使用适配器“{adapter_name}”: {stderr}").into());
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    value
        .parse::<Ipv4Addr>()
        .map_err(|e| format!("适配器返回了无效 IPv4“{value}”: {e}").into())
}

pub fn internet_available(
    local_ip: Ipv4Addr,
    timeout_seconds: u64,
) -> Result<bool, Box<dyn std::error::Error>> {
    let output = Command::new("curl.exe")
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--noproxy",
            "*",
            "--interface",
            &local_ip.to_string(),
            "--connect-timeout",
            &timeout_seconds.to_string(),
            "--max-time",
            &timeout_seconds.to_string(),
            INTERNET_TEST_URL,
        ])
        .output()
        .map_err(|e| format!("无法启动 Windows curl.exe: {e}"))?;

    if !output.status.success() {
        return Ok(false);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim() == INTERNET_TEST_BODY)
}

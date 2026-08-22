use crate::rsa;
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::net::Ipv4Addr;
use std::process::Command;

const GATEWAY: &str = "http://10.10.9.9";
const LOGIN_URL: &str = "http://10.10.9.9/eportal/InterFace.do?method=login";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124 Safari/537.36";

#[derive(Deserialize)]
struct LoginResponse {
    result: String,
    message: Option<String>,
}

pub fn login(
    username: &str,
    password: &str,
    local_ip: Ipv4Addr,
    timeout_seconds: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let cookie_file =
        std::env::temp_dir().join(format!("shu-net-timer-cookie-{}.txt", std::process::id()));
    let result = login_inner(username, password, local_ip, timeout_seconds, &cookie_file);
    let _ = fs::remove_file(cookie_file);
    result
}

fn login_inner(
    username: &str,
    password: &str,
    local_ip: Ipv4Addr,
    timeout_seconds: u64,
    cookie_file: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let html = curl_get(GATEWAY, local_ip, timeout_seconds, cookie_file)?;
    let query = extract_query_string(&html)?;
    let mac = extract_parameter(&query, "mac")?;
    let encrypted = rsa::encrypt_password(&format!("{password}>{mac}"))?;
    let encoded_query = urlencoding::encode(&query).into_owned();
    let cookie_path = cookie_file.to_string_lossy();
    let referer = format!("http://10.10.9.9/eportal/index.jsp?{query}");

    let output = Command::new("curl.exe")
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--fail",
            "--noproxy",
            "*",
            "--interface",
            &local_ip.to_string(),
            "--connect-timeout",
            &timeout_seconds.to_string(),
            "--max-time",
            &timeout_seconds.to_string(),
            "--user-agent",
            USER_AGENT,
            "--cookie",
            cookie_path.as_ref(),
            "--referer",
            &referer,
            "--header",
            "Accept: */*",
            "--data-urlencode",
            &format!("userId={username}"),
            "--data-urlencode",
            &format!("password={encrypted}"),
            "--data-urlencode",
            "service=shu",
            "--data-urlencode",
            "passwordEncrypt=true",
            "--data-urlencode",
            "operatorPwd=",
            "--data-urlencode",
            "operatorUserId=",
            "--data-urlencode",
            "validcode=",
            "--data-urlencode",
            &format!("queryString={encoded_query}"),
            LOGIN_URL,
        ])
        .output()
        .map_err(|e| format!("无法启动 Windows curl.exe: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "登录请求失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let response: LoginResponse =
        serde_json::from_str(&body).map_err(|e| format!("无法解析登录响应: {e}; 响应={body}"))?;
    if response.result == "success" {
        Ok(())
    } else {
        Err(response
            .message
            .unwrap_or_else(|| "校园网返回登录失败".into())
            .into())
    }
}

fn curl_get(
    url: &str,
    local_ip: Ipv4Addr,
    timeout_seconds: u64,
    cookie_file: &std::path::Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let cookie_path = cookie_file.to_string_lossy();
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
            "--user-agent",
            USER_AGENT,
            "--cookie-jar",
            cookie_path.as_ref(),
            url,
        ])
        .output()
        .map_err(|e| format!("无法启动 Windows curl.exe: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "访问校园网关失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn extract_query_string(html: &str) -> Result<String, Box<dyn std::error::Error>> {
    let re = Regex::new(r#"location\.href\s*=\s*['\"]([^'\"]+)['\"]"#)?;
    let url = re
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|m| m.as_str())
        .ok_or("网关页面中没有找到 ePortal 登录地址")?;
    url.split_once('?')
        .map(|(_, query)| query.to_owned())
        .ok_or_else(|| "ePortal 登录地址中没有 queryString".into())
}

fn extract_parameter(query: &str, key: &str) -> Result<String, Box<dyn std::error::Error>> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value.to_owned())
        .ok_or_else(|| format!("queryString 中缺少 {key}").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_redirect_and_mac() {
        let html = r#"<script>location.href='http://10.10.9.9/eportal/index.jsp?wlanuserip=1.2.3.4&mac=aabbcc'</script>"#;
        let query = extract_query_string(html).unwrap();
        assert_eq!(extract_parameter(&query, "mac").unwrap(), "aabbcc");
    }
}

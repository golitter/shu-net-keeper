use crate::curl;
use crate::rsa;
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::io::Write as _;
use std::net::Ipv4Addr;
use std::process::Stdio;
use std::sync::OnceLock;
use std::thread;

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
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let cookie_file = std::env::temp_dir().join(format!(
        "shu-net-timer-cookie-{}-{nonce}.txt",
        std::process::id()
    ));
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

    // 通过 stdin 传递表单数据，避免凭据出现在 curl 的命令行里（进程命令行对其他进程可见）。
    let body = format!(
        "userId={}&password={}&service=shu&passwordEncrypt=true&operatorPwd=&operatorUserId=&validcode=&queryString={}",
        urlencoding::encode(username),
        urlencoding::encode(&encrypted),
        // 原先由 curl --data-urlencode 再编码一次，保持线上格式不变
        urlencoding::encode(&encoded_query)
    );
    let mut child = curl::command()
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
            "--header",
            "Content-Type: application/x-www-form-urlencoded",
            "--data-binary",
            "@-",
            LOGIN_URL,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法启动 Windows curl.exe: {e}"))?;
    // 在独立线程写 stdin：若 curl 在读完 stdin 前写满 stdout/stderr 管道，
    // 同步写会和 wait_with_output 的读取互相等待造成死锁。
    let mut stdin_handle = child.stdin.take().expect("stdin 已被 piped");
    let writer = thread::spawn(move || stdin_handle.write_all(body.as_bytes()));
    let output = child
        .wait_with_output()
        .map_err(|e| format!("等待 curl.exe 退出失败: {e}"))?;
    // curl 可能提前退出（如 --fail 命中 HTTP 错误）导致 stdin 管道断裂，
    // 因此优先报告 curl 自身的退出状态，stdin 写入失败只作兜底。
    let stdin_error = writer
        .join()
        .expect("写 stdin 的线程不应 panic")
        .err()
        .map(|error| format!("向 curl.exe 写入表单数据失败: {error}"));

    if !output.status.success() {
        return Err(format!(
            "登录请求失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    if let Some(error) = stdin_error {
        return Err(error.into());
    }

    let response_text = String::from_utf8_lossy(&output.stdout);
    let response: LoginResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("无法解析登录响应: {e}; 响应={response_text}"))?;
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
    let output = curl::command()
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
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"location\.href\s*=\s*['\"]([^'\"]+)['\"]"#).expect("静态正则必然合法")
    });
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

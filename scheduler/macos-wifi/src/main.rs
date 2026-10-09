use regex::Regex;
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
mod direct;
use direct::DirectNetwork;

// Share the existing, tested portal encryption algorithm without the GUI crate.
#[path = "../../../src/constants.rs"]
#[allow(dead_code)]
mod constants;
#[path = "../../../src/rsa.rs"]
mod rsa;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    ssid: String,
    username: String,
    password: String,
    wifi_password: Option<String>,
    attempts: u32,
    retry_seconds: u64,
    #[serde(default = "default_recovery_seconds")]
    recovery_seconds: u64,
    timeout_seconds: u64,
}

fn default_recovery_seconds() -> u64 {
    30
}

impl Config {
    fn load(path: &Path) -> Result<Self> {
        let config: Self = toml::from_str(&fs::read_to_string(path)?)
            .map_err(|_| "配置 TOML 格式错误或包含未知字段（为保护密码，不输出原文）")?;
        if config.ssid.trim().is_empty()
            || config.username.trim().is_empty()
            || config.password.is_empty()
            || config.username == "your_student_id"
            || config.password == "your_campus_password"
        {
            return Err("请填写实际 SSID、学号和校园网密码".into());
        }
        if !(1..=10).contains(&config.attempts)
            || !(1..=60).contains(&config.retry_seconds)
            || !(1..=60).contains(&config.timeout_seconds)
            || config.recovery_seconds > 120
        {
            return Err("attempts 应为 1..10，retry_seconds 和 timeout_seconds 应为 1..60，recovery_seconds 应为 0..120".into());
        }
        if config.wifi_password.as_ref().is_some_and(|p| p.is_empty()) {
            return Err("开放 WiFi 请删除或注释 wifi_password 字段".into());
        }
        Ok(config)
    }
}

struct CookieFile(PathBuf);

impl CookieFile {
    fn new() -> Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("shu-wifi-{}-{nonce}.cookies", std::process::id()));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        Ok(Self(path))
    }
}

impl Drop for CookieFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn response(output: std::process::Output) -> Result<String> {
    if !output.status.success() {
        // Never print HTTP bodies, redirects, or credential-bearing responses.
        return Err(format!("校园网请求失败（curl 退出码 {:?}）", output.status.code()).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn is_online(body: &str) -> Result<bool> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    match value.get("userIp") {
        Some(serde_json::Value::String(ip)) => Ok(!ip.is_empty()),
        Some(serde_json::Value::Null) => Ok(false),
        // ePortal can return an explicit failure instead of a null userIp when offline.
        None if matches!(
            value.get("result").and_then(|v| v.as_str()),
            Some("fail" | "failed")
        ) =>
        {
            Ok(false)
        }
        _ => Err("校园网状态响应缺少有效 userIp，暂不提交凭据".into()),
    }
}

fn online(network: &DirectNetwork, timeout: u64) -> Result<bool> {
    network.verify()?;
    is_online(&response(
        network
            .curl(timeout)
            .arg(constants::ONLINE_INFO_URL)
            .output()?,
    )?)
}

fn query_from_html(html: &str) -> Result<String> {
    let pattern = Regex::new(
        r#"location\.href\s*=\s*['\"]http://10\.10\.9\.9/eportal/index\.jsp\?([^'\"]+)['\"]"#,
    )?;
    pattern
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
        .ok_or_else(|| "网关未返回预期的校园网登录地址".into())
}

fn form(config: &Config, query: &str) -> Result<String> {
    let mac = query
        .split('&')
        .filter_map(|p| p.split_once('='))
        .find(|(name, _)| *name == "mac")
        .map(|(_, value)| value)
        .filter(|value| !value.is_empty())
        .ok_or("登录参数缺少 mac")?;
    let encrypted =
        rsa::PasswordEncryptor::new()?.encrypt_password(&format!("{}>{mac}", config.password))?;
    let encoded_query = urlencoding::encode(query);
    Ok(format!(
        "userId={}&password={}&service=shu&passwordEncrypt=true&operatorPwd=&operatorUserId=&validcode=&queryString={}",
        urlencoding::encode(&config.username),
        urlencoding::encode(&encrypted),
        urlencoding::encode(&encoded_query),
    ))
}

// false denotes a portal authentication rejection: don't retry a wrong password.
fn login(config: &Config, network: &DirectNetwork) -> Result<bool> {
    if online(network, config.timeout_seconds)? {
        println!("已登录校园网，无需重复认证");
        return Ok(true);
    }
    let cookie = CookieFile::new()?;
    let html = response(
        network
            .curl(config.timeout_seconds)
            .args(["--location", "--max-redirs", "5"])
            .arg("--cookie-jar")
            .arg(&cookie.0)
            .arg(constants::CAMPUS_GATEWAY)
            .output()?,
    )?;
    let query = query_from_html(&html)?;
    let body = form(config, &query)?;
    network.verify()?;
    let mut child = network
        .curl(config.timeout_seconds)
        .arg("--cookie")
        .arg(&cookie.0)
        .arg("--referer")
        .arg(format!("{}?{query}", constants::LOGIN_INDEX))
        .args([
            "--header",
            "Content-Type: application/x-www-form-urlencoded",
            "--data-binary",
            "@-",
            constants::LOGIN_URL,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("无法取得 curl stdin")?;
    let writer = std::thread::spawn(move || stdin.write_all(body.as_bytes()));
    let output = child.wait_with_output()?;
    let write_result = writer.join().map_err(|_| "表单写入线程退出")?;
    let text = response(output)?;
    write_result?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    match value.get("result").and_then(|v| v.as_str()) {
        Some("success") => {}
        Some("fail" | "failed") => {
            eprintln!("校园网拒绝认证，请检查账号、密码或账号状态；本次停止重试");
            return Ok(false);
        }
        _ => return Err("校园网返回未知认证结果".into()),
    }
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        if online(network, config.timeout_seconds)? {
            println!("校园网登录成功，在线状态已验证");
            return Ok(true);
        }
    }
    Err("认证请求成功，但尚未确认在线状态".into())
}

fn run() -> Result<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--refresh-route"] {
        direct::request_route_refresh()?;
        println!("已通知路由维护器检查当前 WiFi 网关（不提交认证请求）");
        return Ok(0);
    }
    if args.len() == 1 && args[0] == "--help" {
        println!(
            "shu-wifi-login --check-config CONFIG.toml\nshu-wifi-login --config CONFIG.toml --ip WIFI_IPV4 --interface WIFI_INTERFACE"
        );
        return Ok(0);
    }
    if args.len() == 2 && args[0] == "--check-config" {
        Config::load(Path::new(&args[1]))?;
        println!("配置验证通过");
        return Ok(0);
    }
    // Internal pipe protocol with the Swift parent, not an on-disk JSON config.
    // Never forward this output to the service log: wifi_password may be present.
    if args.len() == 2 && args[0] == "--settings" {
        let config = Config::load(Path::new(&args[1]))?;
        println!(
            "{}",
            serde_json::json!({
                "ssid": config.ssid, "wifi_password": config.wifi_password,
                "attempts": config.attempts, "retry_seconds": config.retry_seconds,
                "recovery_seconds": config.recovery_seconds,
                "timeout_seconds": config.timeout_seconds,
            })
        );
        return Ok(0);
    }
    if args.len() != 6 || args[0] != "--config" || args[2] != "--ip" || args[4] != "--interface" {
        return Err(
            "用法：shu-wifi-login --config CONFIG --ip WIFI_IPV4 --interface WIFI_INTERFACE".into(),
        );
    }
    let config = Config::load(Path::new(&args[1]))?;
    let ip: Ipv4Addr = args[3].parse()?;
    let network = DirectNetwork::new(&args[5], ip)?;
    println!(
        "校园网请求已隔离代理：绑定 WiFi 接口 {}，仅使用专用直连路由",
        network.interface
    );
    Ok(if login(&config, &network)? { 0 } else { 2 })
}

fn main() {
    // The Swift service can cancel this worker and all its curl descendants together.
    unsafe {
        // The parent uses DispatchSourceSignal, which installs SIG_IGN handlers.
        // Restore defaults so cancellation also terminates this worker's curl child.
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::setpgid(0, 0);
    }
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_explicit_portal_state() {
        assert!(is_online(r#"{"userIp":"10.1.2.3"}"#).unwrap());
        assert!(!is_online(r#"{"userIp":null}"#).unwrap());
        assert!(!is_online(r#"{"result":"fail"}"#).unwrap());
        assert!(is_online(r#"{"message":"error"}"#).is_err());
        assert!(is_online("<html>redirect</html>").is_err());
    }

    #[test]
    fn rejects_unexpected_login_origin_and_missing_mac() {
        assert!(query_from_html("location.href='http://evil.example/?mac=abc'").is_err());
        let query =
            query_from_html("location.href = 'http://10.10.9.9/eportal/index.jsp?mac=abc&x=1'")
                .unwrap();
        let config: Config = toml::from_str(include_str!("../config.example.toml")).unwrap();
        let body = form(&config, &query).unwrap();
        assert!(body.contains("queryString=mac%253Dabc%2526x%253D1"));
        assert!(!body.contains(&config.password));
        assert!(form(&config, "x=1").is_err());
    }

    #[test]
    fn config_errors_do_not_echo_credentials() {
        let cookie = CookieFile::new().unwrap();
        fs::write(&cookie.0, "password = TOP_SECRET_BROKEN_TOML").unwrap();
        let error = Config::load(&cookie.0).err().unwrap().to_string();
        assert!(!error.contains("TOP_SECRET"));
        fs::write(&cookie.0, include_str!("../config.example.toml")).unwrap();
        assert!(Config::load(&cookie.0).is_err());
        let valid = include_str!("../config.example.toml")
            .replace("your_student_id", "12345678")
            .replace(
                "your_campus_password",
                "test-password-not-a-real-credential",
            );
        fs::write(&cookie.0, &valid).unwrap();
        assert!(Config::load(&cookie.0).is_ok());
        let legacy = valid.replace("recovery_seconds = 30", "");
        fs::write(&cookie.0, legacy).unwrap();
        assert_eq!(Config::load(&cookie.0).unwrap().recovery_seconds, 30);
        fs::write(
            &cookie.0,
            valid.replace("recovery_seconds = 30", "recovery_seconds = 121"),
        )
        .unwrap();
        assert!(Config::load(&cookie.0).is_err());
        fs::write(&cookie.0, valid.replace("attempts = 4", "attempts = 0")).unwrap();
        assert!(Config::load(&cookie.0).is_err());
    }
}

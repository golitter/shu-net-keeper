mod config;
mod ethernet;
mod login;
mod rsa;

use chrono::Local;
use config::Config;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const POST_LOGIN_RETRY_DELAYS: [u64; 3] = [2, 3, 5];

fn main() {
    if let Err(error) = run() {
        log_line(&format!("失败: {error}"));
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = config_path()?;
    let config = Config::load(&config_path)?;
    log_line(&format!("开始单次检测，适配器：{}", config.adapter_name));

    let local_ip = ethernet::ipv4_for_adapter(&config.adapter_name)?;
    log_line(&format!("以太网 IPv4：{local_ip}"));

    if ethernet::internet_available(local_ip, config.timeout_seconds)? {
        log_line("网络正常，无需登录");
        return Ok(());
    }

    log_line("以太网无法访问公网，开始登录 Shu(ForAll)");
    login::login(
        &config.username,
        &config.password,
        local_ip,
        config.timeout_seconds,
    )?;

    log_line("登录请求成功，等待校园网认证状态生效");
    for (index, delay_seconds) in POST_LOGIN_RETRY_DELAYS.iter().enumerate() {
        thread::sleep(Duration::from_secs(*delay_seconds));
        let attempt = index + 1;
        if ethernet::internet_available(local_ip, config.timeout_seconds)? {
            log_line(&format!("第 {attempt} 次联网验证成功，公网连接已经恢复"));
            return Ok(());
        }
        log_line(&format!("第 {attempt} 次联网验证失败"));
    }

    Err("登录请求成功，但三次验证后以太网仍无法访问公网".into())
}

fn config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--config") {
        let value = args.get(index + 1).ok_or("--config 后缺少文件路径")?;
        return Ok(PathBuf::from(value));
    }
    let current = env::current_dir()?.join("config.toml");
    if current.exists() {
        return Ok(current);
    }
    let exe = env::current_exe()?;
    Ok(exe.parent().unwrap_or(Path::new(".")).join("config.toml"))
}

fn log_line(message: &str) {
    let timestamp = Local::now();
    let line = format!("[{}] {message}", timestamp.format("%Y-%m-%d %H:%M:%S"));
    println!("{line}");

    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let log_dir = dir.join("logs");
        if fs::create_dir_all(&log_dir).is_ok() {
            let path = log_dir.join(format!("{}.log", timestamp.format("%Y-%m-%d")));
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{line}");
            }
        }
    }
}

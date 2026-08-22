use std::path::PathBuf;
use std::process::Command;

/// 固定使用系统自带的 curl.exe（System32），避免 PATH 中被植入同名程序截获登录凭据。
pub fn command() -> Command {
    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    Command::new(PathBuf::from(system_root).join("System32").join("curl.exe"))
}

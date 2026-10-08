use std::net::Ipv4Addr;
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct DirectNetwork {
    pub interface: String,
    pub ip: Ipv4Addr,
}

impl DirectNetwork {
    pub fn new(interface: &str, ip: Ipv4Addr) -> Result<Self> {
        let suffix = interface
            .strip_prefix("en")
            .ok_or("认证必须使用物理 WiFi 接口")?;
        if suffix.is_empty() || !suffix.bytes().all(|c| c.is_ascii_digit()) {
            return Err("无效的 WiFi 接口名称".into());
        }
        if ip.is_unspecified()
            || ip.is_loopback()
            || ip.is_link_local()
            || ip.is_multicast()
            || ip.is_broadcast()
        {
            return Err("WiFi 尚未取得有效 IPv4".into());
        }
        Ok(Self {
            interface: interface.to_owned(),
            ip,
        })
    }

    pub fn verify(&self) -> Result<()> {
        let output = Command::new("/usr/sbin/ipconfig")
            .args(["getifaddr", &self.interface])
            .output()?;
        let current = String::from_utf8(output.stdout)?;
        if !output.status.success() || current.trim() != self.ip.to_string() {
            return Err("WiFi IP 已变化，等待下一次检测".into());
        }
        let output = Command::new("/sbin/route")
            .args(["-n", "get", "-ifscope", &self.interface, "10.10.9.9"])
            .output()?;
        // macOS route can exit 0 even when no scoped route exists; inspect its fields.
        if !output.status.success()
            || !route_uses_interface(&String::from_utf8(output.stdout)?, &self.interface)
        {
            return Err("缺少校园网关的 WiFi 专用路由，未提交认证请求；请安装 scripts/install-direct-route.sh 路由维护器".into());
        }
        Ok(())
    }

    pub fn curl(&self, timeout: u64) -> Command {
        let mut command = Command::new("/usr/bin/curl");
        for name in [
            "http_proxy",
            "HTTP_PROXY",
            "https_proxy",
            "HTTPS_PROXY",
            "all_proxy",
            "ALL_PROXY",
            "no_proxy",
            "NO_PROXY",
        ] {
            command.env_remove(name);
        }
        command.args([
            "--disable",
            "--silent",
            "--show-error",
            "--fail",
            "--proxy",
            "",
            "--noproxy",
            "*",
            "--ipv4",
            "--interface",
            &format!("if!{}", self.interface),
            "--proto",
            "=http",
            "--proto-redir",
            "=http",
            "--connect-timeout",
            &timeout.to_string(),
            "--max-time",
            &timeout.to_string(),
        ]);
        command
    }
}

fn route_uses_interface(route: &str, expected: &str) -> bool {
    route
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .any(|(key, value)| key == "interface" && value.trim() == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_tunnel_and_missing_routes() {
        let ip = "10.98.172.58".parse().unwrap();
        assert!(DirectNetwork::new("utun4", ip).is_err());
        assert!(DirectNetwork::new("en0;evil", ip).is_err());
        assert!(DirectNetwork::new("en0", "169.254.1.2".parse().unwrap()).is_err());
        assert!(!route_uses_interface("interface: utun4", "en0"));
        assert!(!route_uses_interface("route: not in table", "en0"));
        assert!(route_uses_interface("    interface: en0\n", "en0"));
    }

    #[test]
    fn every_request_disables_proxy_and_binds_physical_interface() {
        let direct = DirectNetwork::new("en0", "10.98.172.58".parse().unwrap()).unwrap();
        let command = direct.curl(10);
        let args: Vec<_> = command.get_args().map(|s| s.to_str().unwrap()).collect();
        assert!(args.windows(2).any(|p| p == ["--proxy", ""]));
        assert!(args.windows(2).any(|p| p == ["--interface", "if!en0"]));
        assert!(args.windows(2).any(|p| p == ["--noproxy", "*"]));
        assert!(command.get_envs().all(|(_, value)| value.is_none()));
        assert_eq!(command.get_envs().count(), 8);
    }
}

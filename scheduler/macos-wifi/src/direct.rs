use std::io::Write;
use std::net::Ipv4Addr;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut requested_gateway = None;
        loop {
            let address = self.dhcp_value(&["getifaddr", &self.interface])?;
            if address != Some(self.ip) {
                return Err("WiFi IP 已变化，等待下一次检测".into());
            }
            let gateway = self.dhcp_value(&["getoption", &self.interface, "router"])?;
            let output = Command::new("/sbin/route")
                .args(["-n", "get", "-ifscope", &self.interface, "10.10.9.9"])
                .output()?;
            // Wait briefly for the privileged helper to restore a route after wake.
            // Never fall back to the VPN while waiting.
            let route = String::from_utf8(output.stdout)?;
            if let Some(gateway) = gateway
                && output.status.success()
                && route_matches_dhcp(&route, &self.interface, gateway)
            {
                // Read both DHCP values again: a lease can change during route lookup.
                let stable_ip = self.dhcp_value(&["getifaddr", &self.interface])?;
                let stable_gateway = self.dhcp_value(&["getoption", &self.interface, "router"])?;
                if stable_ip == Some(self.ip) && stable_gateway == Some(gateway) {
                    return Ok(());
                }
            }
            if let Some(gateway) = gateway
                && requested_gateway != Some(gateway)
            {
                requested_gateway = Some(gateway);
                match request_route_refresh() {
                    Ok(()) => println!("专用路由尚未匹配当前 DHCP 网关，已通知维护器立即刷新"),
                    Err(_) => {
                        eprintln!("路由即时刷新通知不可用，请更新路由维护器；继续等待定时检查")
                    }
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(
                    "WiFi DHCP 网关与专用路由尚未就绪或不一致，未提交认证请求；请检查路由维护器"
                        .into(),
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    fn dhcp_value(&self, arguments: &[&str]) -> Result<Option<Ipv4Addr>> {
        let output = Command::new("/usr/sbin/ipconfig")
            .args(arguments)
            .output()?;
        if !output.status.success() {
            return Ok(None);
        }
        let address = String::from_utf8(output.stdout)?
            .trim()
            .parse::<Ipv4Addr>()
            .ok();
        Ok(address.filter(|ip| {
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_multicast()
                && !ip.is_broadcast()
        }))
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
            &timeout.min(2).to_string(),
            "--max-time",
            &timeout.to_string(),
        ]);
        command
    }
}

pub fn request_route_refresh() -> std::io::Result<()> {
    let directory = std::path::Path::new("/Library/Application Support/SHUWiFiKeeper");
    let parent = std::fs::symlink_metadata(directory)?;
    if !parent.is_dir() || parent.uid() != 0 || parent.mode() & 0o022 != 0 {
        return Err(std::io::Error::other("不可信的通知目录"));
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join("route-request"))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.nlink() != 1 {
        return Err(std::io::Error::other("不可信的通知文件"));
    }
    // Fixed notification, not a privileged request payload. Never create a file.
    file.set_len(0)?;
    file.write_all(b"refresh\n")
}

fn route_field<'a>(route: &'a str, name: &str) -> Option<&'a str> {
    route
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.trim())
}

fn route_matches_dhcp(route: &str, interface: &str, gateway: Ipv4Addr) -> bool {
    route_field(route, "interface") == Some(interface)
        && route_field(route, "gateway").and_then(|value| value.parse::<Ipv4Addr>().ok())
            == Some(gateway)
        && !route_field(route, "flags")
            .is_some_and(|flags| flags.contains("REJECT") || flags.contains("BLACKHOLE"))
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
        let gateway = "10.88.64.1".parse().unwrap();
        assert!(!route_matches_dhcp(
            "interface: utun4\ngateway: 10.88.64.1",
            "en0",
            gateway
        ));
        assert!(!route_matches_dhcp("route: not in table", "en0", gateway));
    }

    #[test]
    fn rejects_old_dhcp_gateway_even_on_the_right_interface() {
        let gateway = "10.88.64.1".parse().unwrap();
        assert!(!route_matches_dhcp(
            "interface: en0\ngateway: 10.98.128.1",
            "en0",
            gateway
        ));
        assert!(!route_matches_dhcp("interface: en0", "en0", gateway));
        assert!(route_matches_dhcp(
            " interface: en0\n gateway: 10.88.64.1\n flags: <UP,GATEWAY,IFSCOPE>",
            "en0",
            gateway
        ));
        assert!(!route_matches_dhcp(
            "interface: en0\ngateway: 10.88.64.1\nflags: <UP,REJECT>",
            "en0",
            gateway
        ));
    }

    #[test]
    fn every_request_disables_proxy_and_binds_physical_interface() {
        let direct = DirectNetwork::new("en0", "10.98.172.58".parse().unwrap()).unwrap();
        let command = direct.curl(10);
        let args: Vec<_> = command.get_args().map(|s| s.to_str().unwrap()).collect();
        assert!(args.windows(2).any(|p| p == ["--proxy", ""]));
        assert!(args.windows(2).any(|p| p == ["--interface", "if!en0"]));
        assert!(args.windows(2).any(|p| p == ["--noproxy", "*"]));
        assert!(args.windows(2).any(|p| p == ["--connect-timeout", "2"]));
        assert!(command.get_envs().all(|(_, value)| value.is_none()));
        assert_eq!(command.get_envs().count(), 8);
    }
}

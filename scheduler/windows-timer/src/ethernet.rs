use crate::curl;
use std::net::Ipv4Addr;

#[cfg(windows)]
use std::mem::size_of;
#[cfg(windows)]
use std::slice;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
#[cfg(windows)]
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
    IP_ADAPTER_ADDRESSES_LH,
};
#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

const INTERNET_TEST_URL: &str = "http://www.msftconnecttest.com/connecttest.txt";
const INTERNET_TEST_BODY: &str = "Microsoft Connect Test";

pub fn ipv4_for_adapter(adapter_name: &str) -> Result<Ipv4Addr, Box<dyn std::error::Error>> {
    ipv4_for_adapter_impl(adapter_name)
}

#[cfg(not(windows))]
fn ipv4_for_adapter_impl(_adapter_name: &str) -> Result<Ipv4Addr, Box<dyn std::error::Error>> {
    Err("此程序只支持 Windows".into())
}

#[cfg(windows)]
fn ipv4_for_adapter_impl(adapter_name: &str) -> Result<Ipv4Addr, Box<dyn std::error::Error>> {
    const INITIAL_BUFFER_BYTES: usize = 15_000;
    const IF_OPER_STATUS_UP: i32 = 1;
    const IF_TYPE_ETHERNET_CSMACD: u32 = 6;

    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut buffer = vec![0usize; INITIAL_BUFFER_BYTES.div_ceil(size_of::<usize>())];

    loop {
        let mut buffer_bytes = (buffer.len() * size_of::<usize>()) as u32;
        // SAFETY: `buffer` is pointer-aligned storage whose byte size is passed in
        // `buffer_bytes`. The API writes a linked list entirely inside this buffer.
        let result = unsafe {
            GetAdaptersAddresses(
                AF_INET as u32,
                flags,
                std::ptr::null(),
                buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
                &mut buffer_bytes,
            )
        };

        if result == ERROR_BUFFER_OVERFLOW {
            buffer.resize((buffer_bytes as usize).div_ceil(size_of::<usize>()), 0);
            continue;
        }
        if result != NO_ERROR {
            return Err(format!("Windows GetAdaptersAddresses 调用失败，错误码 {result}").into());
        }

        let mut adapter = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        while !adapter.is_null() {
            // SAFETY: every adapter pointer is part of the linked list returned in
            // the live `buffer`; FriendlyName is a Windows-owned NUL-terminated UTF-16 string.
            let current = unsafe { &*adapter };
            let friendly_name = unsafe { wide_string(current.FriendlyName) };
            if friendly_name == adapter_name {
                if current.IfType != IF_TYPE_ETHERNET_CSMACD {
                    return Err(format!("适配器“{adapter_name}”不是物理以太网接口").into());
                }
                if current.OperStatus != IF_OPER_STATUS_UP {
                    return Err(format!("指定的以太网适配器“{adapter_name}”未连接").into());
                }

                let mut unicast = current.FirstUnicastAddress;
                while !unicast.is_null() {
                    // SAFETY: the unicast node and its sockaddr belong to the same
                    // API buffer and remain valid until this function returns.
                    let address = unsafe { &*unicast };
                    let sockaddr = address.Address.lpSockaddr;
                    if !sockaddr.is_null() && unsafe { (*sockaddr).sa_family } == AF_INET {
                        let sockaddr_v4 = sockaddr.cast::<SOCKADDR_IN>();
                        let octets = unsafe {
                            std::ptr::read_unaligned(
                                std::ptr::addr_of!((*sockaddr_v4).sin_addr).cast::<[u8; 4]>(),
                            )
                        };
                        let ip = Ipv4Addr::from(octets);
                        if !ip.is_link_local() && !ip.is_unspecified() {
                            return Ok(ip);
                        }
                    }
                    unicast = address.Next;
                }
                return Err(format!("指定的以太网适配器“{adapter_name}”没有可用 IPv4 地址").into());
            }
            adapter = current.Next;
        }

        return Err(format!("找不到名为“{adapter_name}”的以太网适配器").into());
    }
}

#[cfg(windows)]
unsafe fn wide_string(pointer: *const u16) -> String {
    if pointer.is_null() {
        return String::new();
    }
    let mut length = 0usize;
    // SAFETY: callers pass a NUL-terminated UTF-16 string returned by Windows.
    while unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the loop above found the terminator, so `length` code units are valid.
    String::from_utf16_lossy(unsafe { slice::from_raw_parts(pointer, length) })
}

pub fn internet_available(
    local_ip: Ipv4Addr,
    timeout_seconds: u64,
) -> Result<bool, Box<dyn std::error::Error>> {
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
            INTERNET_TEST_URL,
        ])
        .output()
        .map_err(|e| format!("无法启动 Windows curl.exe: {e}"))?;

    if !output.status.success() {
        return Ok(false);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim() == INTERNET_TEST_BODY)
}

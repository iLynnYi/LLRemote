use std::net::Ipv4Addr;
#[cfg(not(target_os = "windows"))]
use std::process::Command;

use super::{LocalAddress, LocalAddressKind};

pub fn local_address_candidates() -> Vec<LocalAddress> {
    let mut addresses = platform_lan_candidates();

    addresses.push(LocalAddress {
        label: "Loopback".to_string(),
        address: "127.0.0.1".to_string(),
        kind: LocalAddressKind::Loopback,
    });

    dedupe_addresses(addresses)
}

fn platform_lan_candidates() -> Vec<LocalAddress> {
    #[cfg(target_os = "windows")]
    return windows_lan_candidates();

    #[cfg(not(target_os = "windows"))]
    {
        let output = Command::new("ifconfig")
            .output()
            .or_else(|_| Command::new("/sbin/ifconfig").output())
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok());

        output
            .as_deref()
            .map(parse_ifconfig_addresses)
            .unwrap_or_default()
    }
}

/// Status polling runs in the interactive Session as well as the service
/// worker. Enumerate in-process so it never creates console child processes.
#[cfg(target_os = "windows")]
fn windows_lan_candidates() -> Vec<LocalAddress> {
    use std::ptr::null;
    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

    let mut size = 15 * 1024_u32;
    for _ in 0..3 {
        if size == 0 || size > 1024 * 1024 {
            return Vec::new();
        }
        // The adapter structures contain u64 unions and require aligned storage.
        let mut buffer = vec![0_u64; (size as usize).div_ceil(8)];
        let adapters = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let result = unsafe {
            GetAdaptersAddresses(
                AF_INET as u32,
                GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER,
                null(),
                adapters,
                &mut size,
            )
        };
        if result == ERROR_BUFFER_OVERFLOW {
            continue;
        }
        if result != ERROR_SUCCESS {
            return Vec::new();
        }
        let mut addresses = Vec::new();
        let mut current = adapters;
        while !current.is_null() {
            // All linked records and strings belong to the live API buffer.
            let adapter = unsafe { &*current };
            let name = adapter.FriendlyName;
            let label = if name.is_null() {
                "Network".into()
            } else {
                let mut length = 0;
                while length < 1024 && unsafe { *name.add(length) } != 0 {
                    length += 1;
                }
                String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(name, length) })
            };
            let mut unicast = adapter.FirstUnicastAddress;
            while !unicast.is_null() {
                let record = unsafe { &*unicast };
                let socket = record.Address.lpSockaddr;
                if !socket.is_null()
                    && record.Address.iSockaddrLength as usize >= std::mem::size_of::<SOCKADDR_IN>()
                    && unsafe { (*socket).sa_family } == AF_INET
                {
                    let ipv4 = unsafe { &*socket.cast::<SOCKADDR_IN>() };
                    let candidate =
                        Ipv4Addr::from(unsafe { ipv4.sin_addr.S_un.S_addr }.to_ne_bytes())
                            .to_string();
                    if is_shareable_ipv4(&candidate) {
                        addresses.push(LocalAddress {
                            label: label.clone(),
                            address: candidate,
                            kind: LocalAddressKind::LanCandidate,
                        });
                    }
                }
                unicast = record.Next;
            }
            current = adapter.Next;
        }
        return dedupe_addresses(addresses);
    }
    Vec::new()
}

#[cfg(any(not(target_os = "windows"), test))]
fn parse_ifconfig_addresses(output: &str) -> Vec<LocalAddress> {
    let mut addresses = Vec::new();
    let mut current_label = String::new();

    for line in output.lines() {
        if !line.starts_with(char::is_whitespace) && line.contains(':') {
            current_label = line
                .split(':')
                .next()
                .unwrap_or("Network")
                .trim()
                .to_string();
        }

        let trimmed = line.trim();
        if !trimmed.starts_with("inet ") {
            continue;
        }

        let Some(candidate) = trimmed.split_whitespace().nth(1) else {
            continue;
        };

        if is_shareable_ipv4(candidate) {
            addresses.push(LocalAddress {
                label: current_label.clone(),
                address: candidate.to_string(),
                kind: LocalAddressKind::LanCandidate,
            });
        }
    }

    dedupe_addresses(addresses)
}

fn is_shareable_ipv4(value: &str) -> bool {
    let Ok(address) = value.parse::<Ipv4Addr>() else {
        return false;
    };

    !(address.is_loopback()
        || address.is_unspecified()
        || address.is_link_local()
        || address.octets()[0] >= 224)
}

fn dedupe_addresses(addresses: Vec<LocalAddress>) -> Vec<LocalAddress> {
    let mut deduped = Vec::new();

    for address in addresses {
        if deduped
            .iter()
            .any(|existing: &LocalAddress| existing.address == address.address)
        {
            continue;
        }

        deduped.push(address);
    }

    deduped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ifconfig_addresses_returns_lan_candidates_only() {
        let output = r#"
lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384
    inet 127.0.0.1 netmask 0xff000000
en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
    inet 192.168.2.24 netmask 0xffffff00 broadcast 192.168.2.255
awdl0: flags=8943<UP,BROADCAST,RUNNING,PROMISC,SIMPLEX,MULTICAST> mtu 1484
    inet 169.254.13.20 netmask 0xffff0000 broadcast 169.254.255.255
utun4: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1380
    inet 10.10.0.2 --> 10.10.0.2 netmask 0xffffffff
"#;

        let addresses = parse_ifconfig_addresses(output);

        assert_eq!(
            addresses,
            vec![
                LocalAddress {
                    label: "en0".to_string(),
                    address: "192.168.2.24".to_string(),
                    kind: LocalAddressKind::LanCandidate,
                },
                LocalAddress {
                    label: "utun4".to_string(),
                    address: "10.10.0.2".to_string(),
                    kind: LocalAddressKind::LanCandidate,
                },
            ]
        );
    }

    #[test]
    fn local_address_candidates_keeps_loopback_as_fallback() {
        let addresses = local_address_candidates();

        assert!(addresses
            .iter()
            .any(|address| address.kind == LocalAddressKind::Loopback
                && address.address == "127.0.0.1"));
    }
}

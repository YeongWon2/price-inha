//! (결정 34) Ollama 주소가 닿는 곳: 내 컴퓨터(루프백) · 같은 집 다른 기기(사설망) · 그 밖.
//!
//! 사설망은 IPv4 `10/8` · `172.16/12` · `192.168/16` · `169.254/16`, IPv6 `fc00::/7` · `fe80::/10`, 이름 `*.local`뿐이다.
//! 공인 주소 · 그 밖의 도메인은 [`OllamaReach::Outside`]이고 공급자를 만들 때 거부한다(V23).
//! `*.local` 이름은 연결할 때 풀어 본 주소 가운데 사설망 · 루프백 것만 쓴다(공인 주소로 풀리면 연결하지 않는다).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

/// Ollama 주소가 닿는 곳.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OllamaReach {
    /// 내 컴퓨터(`127.0.0.1` · `::1` · `localhost`).
    Loopback,
    /// 같은 집 다른 기기(사설망 주소 · `*.local`). `host`는 주소의 호스트 글자 그대로다.
    PrivateNetwork {
        /// 호스트(예: `192.168.0.7`, `nas.local`, `[fe80::1]`).
        host: String,
    },
    /// 그 밖(공인 주소 · 도메인 · 읽을 수 없는 주소 · `http`/`https`가 아닌 주소).
    Outside,
}

/// `base_url`이 닿는 곳. `http` · `https`가 아니거나 읽을 수 없으면 [`OllamaReach::Outside`].
pub fn ollama_reach(base_url: &str) -> OllamaReach {
    if !base_url.is_empty() {
        return OllamaReach::Outside; // red 뼈대
    }
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return OllamaReach::Outside;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return OllamaReach::Outside;
    }
    if crate::http::is_loopback_host(&url) {
        return OllamaReach::Loopback;
    }
    let Some(host) = url.host_str() else {
        return OllamaReach::Outside;
    };
    if is_private_host(host) {
        OllamaReach::PrivateNetwork {
            host: host.to_string(),
        }
    } else {
        OllamaReach::Outside
    }
}

/// 주소의 호스트 글자(IPv6은 `[…]`)가 사설망인가.
fn is_private_host(host: &str) -> bool {
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return inner.parse::<Ipv6Addr>().is_ok_and(is_private_v6);
    }
    match host.parse::<Ipv4Addr>() {
        Ok(ip) => is_private_v4(ip),
        Err(_) => is_local_name(host),
    }
}

/// `*.local`(mDNS) 이름인가. 마지막 조각만 본다(`local` 하나만은 이름이 아니다).
fn is_local_name(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    name.len() > ".local".len() && name.ends_with(".local")
}

fn is_private_v4(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_link_local()
}

fn is_private_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    // fc00::/7(고유 지역) · fe80::/10(링크 지역).
    (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
}

/// 풀어 본 주소가 쓸 수 있는 곳(사설망 · 루프백)인가.
pub(crate) fn is_home_addr(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || is_private_v4(ip),
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || is_private_v6(ip)
                || ip.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback() || is_private_v4(v4))
        }
    }
}

/// 이름을 운영체제에 풀게 하고, 사설망 · 루프백 주소만 남기는 풀이기(`*.local`이 공인 주소로 풀리면 연결하지 않는다).
pub(crate) struct HomeOnlyResolver;

impl Resolve for HomeOnlyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let found = tokio::task::spawn_blocking(move || {
                (host.as_str(), 0)
                    .to_socket_addrs()
                    .map(|addrs| addrs.collect::<Vec<SocketAddr>>())
            })
            .await??;
            let kept: Vec<SocketAddr> = found
                .into_iter()
                .filter(|addr| is_home_addr(addr.ip()))
                .collect();
            if kept.is_empty() {
                return Err("사설망 · 루프백 주소로 풀리지 않는 이름입니다".into());
            }
            let addrs: Addrs = Box::new(kept.into_iter());
            Ok(addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 사설망_주소와_local_이름만_같은_집이다() {
        for inside in [
            "http://10.1.2.3:11434",
            "http://172.16.0.9:11434",
            "http://172.31.255.1:11434",
            "http://192.168.0.7:11434",
            "http://169.254.10.20:11434",
            "http://[fd12::5]:11434",
            "http://[fe80::1]:11434",
            "http://nas.local:11434",
        ] {
            assert!(
                matches!(ollama_reach(inside), OllamaReach::PrivateNetwork { .. }),
                "{inside}"
            );
        }
        for outside in [
            "http://172.32.0.1:11434",
            "http://8.8.8.8:11434",
            "https://ollama.example:11434",
            "http://local:11434",
            "ftp://192.168.0.7",
            "http://[2001:db8::1]:11434",
        ] {
            assert_eq!(ollama_reach(outside), OllamaReach::Outside, "{outside}");
        }
        assert_eq!(ollama_reach("http://127.0.0.1:11434"), OllamaReach::Loopback);
    }

    #[test]
    fn 풀린_주소는_사설망과_루프백만_쓴다() {
        assert!(is_home_addr("192.168.1.2".parse().unwrap()));
        assert!(is_home_addr("127.0.0.1".parse().unwrap()));
        assert!(is_home_addr("::ffff:10.0.0.1".parse().unwrap()));
        assert!(!is_home_addr("1.1.1.1".parse().unwrap()));
        assert!(!is_home_addr("2001:db8::1".parse().unwrap()));
    }
}

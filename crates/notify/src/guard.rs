//! 보낼 때 내부망 막기(SSRF). 구독 주소의 이름을 한 번 해석해 검사하고, 검사한 IP로만 접속한다.

use std::net::{IpAddr, SocketAddr};

use async_trait::async_trait;

use crate::Error;

/// 이름 해석 자리. 실제로는 [`SystemResolver`], 시험은 로컬 가짜 해석기다.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// `host`(IP 글자가 아닌 이름)를 해석해 `port`를 붙인 주소들을 돌려준다.
    /// 해석하지 못하면 [`Error::Transport`].
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, Error>;
}

/// 운영체제의 이름 해석(tokio `lookup_host`)을 쓰는 실제 해석기.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemResolver;

#[async_trait]
impl Resolver for SystemResolver {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, Error> {
        let _ = (host, port);
        todo!()
    }
}

/// 검사를 마친 보낼 곳: 접속할 이름과, 접속해도 되는 주소들.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedTarget {
    /// 구독 주소의 이름(IP 글자면 그 글자, IPv6는 대괄호 없이).
    pub host: String,
    /// 검사한 주소들(해석기가 준 순서 그대로, 포트는 구독 주소의 포트, 없으면 443).
    /// 접속은 이 주소로만 한다(다시 해석하지 않는다: DNS 재바인딩 막기).
    pub addrs: Vec<SocketAddr>,
}

/// 내부망 주소인가: 루프백 · 사설(10/8, 172.16/12, 192.168/16) · 링크 로컬(169.254/16, fe80::/10)
/// · 공유 주소(100.64/10) · 지정 안 됨(0.0.0.0, ::) · 브로드캐스트 · 멀티캐스트 · ULA(fc00::/7).
/// IPv4를 담은 IPv6(`::ffff:a.b.c.d`)는 안의 IPv4로 본다.
pub fn is_internal_ip(ip: IpAddr) -> bool {
    let _ = ip;
    todo!()
}

/// 구독 주소를 보내기 전에 검사한다.
///
/// - 이름이 IP 글자면 해석기를 부르지 않고 그 IP를 검사한다.
/// - 아니면 해석기를 **한 번** 불러(포트는 주소의 포트, 없으면 443) 나온 주소를 모두 검사한다.
/// - 하나라도 내부망이면 [`Error::InternalAddress`]. 해석 결과가 비었거나 해석이 실패하면
///   [`Error::Transport`]. 주소 모양이 아니면 [`Error::Transport`].
pub async fn pin_endpoint(endpoint: &str, resolver: &dyn Resolver) -> Result<PinnedTarget, Error> {
    let _ = (endpoint, resolver);
    todo!()
}

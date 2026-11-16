//! 보낼 때 내부망 막기(SSRF, notify 2차): 구독 주소의 이름을 한 번 해석해 하나라도 내부망이면
//! 보내지 않고, 검사한 IP로만 접속한다. 해석기는 로컬 가짜다. 바깥으로 접속하지 않는다
//! (내부망이면 접속하기 전에 멈추므로, 가짜 해석기는 내부망 주소만 HttpTransport에 준다).

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pi_notify::{
    AttemptOutcome, Error, HttpTransport, Notifier, NotifierConfig, PushRequest, PushTransport,
    Resolver, VapidKey, is_internal_ip, pin_endpoint,
};
use pi_store::Store;

use crate::common::{FakeClock, MaskSecret, SUBJECT, Subscriber, T0, notice};

/// 정해 둔 답을 주고, 불린 (이름, 포트)를 적어 두는 가짜 해석기.
struct FakeResolver {
    answer: Result<Vec<IpAddr>, String>,
    calls: Mutex<Vec<(String, u16)>>,
}

impl FakeResolver {
    fn answering(ips: &[&str]) -> Arc<FakeResolver> {
        Arc::new(FakeResolver {
            answer: Ok(ips.iter().map(|ip| ip.parse().unwrap()).collect()),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn failing() -> Arc<FakeResolver> {
        Arc::new(FakeResolver {
            answer: Err("이름을 찾지 못했다".to_owned()),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<(String, u16)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl Resolver for FakeResolver {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, Error> {
        self.calls.lock().unwrap().push((host.to_owned(), port));
        match &self.answer {
            Ok(ips) => Ok(ips.iter().map(|ip| SocketAddr::new(*ip, port)).collect()),
            Err(message) => Err(Error::Transport(message.clone())),
        }
    }
}

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

fn request(endpoint: &str) -> PushRequest {
    PushRequest {
        endpoint: endpoint.to_owned(),
        headers: vec![("TTL".to_owned(), "60".to_owned())],
        body: b"x".to_vec(),
    }
}

// 검증: E38
#[test]
fn 루프백_사설_링크_로컬_ula_같은_내부망_주소를_가려낸다() {
    for text in [
        "127.0.0.1",
        "127.9.9.9",
        "10.0.0.1",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "255.255.255.255",
        "224.0.0.1",
        "::1",
        "::",
        "fc00::1",
        "fd12:3456::1",
        "fe80::1",
        "ff02::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "::ffff:169.254.169.254",
        // IPv4를 담는 다른 IPv6 형식(NAT64 · IPv4-compatible · 6to4)도 안의 IPv4로 본다.
        "64:ff9b::a00:1",
        "64:ff9b::7f00:1",
        "64:ff9b:1::a00:1",
        "::7f00:1",
        "::a9fe:a9fe",
        "2002:a00:1::",
        "2002:7f00:1::1",
        // IPv4 예약 대역.
        "240.0.0.1",
        "198.18.0.1",
        "198.19.255.255",
        "192.0.0.1",
    ] {
        assert!(is_internal_ip(ip(text)), "{text}는 내부망이다");
    }
}

// 검증: E38
#[test]
fn 공인_주소와_사설_경계_바로_밖은_내부망이_아니다() {
    for text in [
        "9.9.9.9",
        "172.15.255.255",
        "172.32.0.1",
        "100.63.255.255",
        "100.128.0.1",
        "192.169.0.1",
        "2606:4700::1111",
        "::ffff:9.9.9.9",
        "64:ff9b::909:909",
        "2002:909:909::",
        "198.17.255.255",
        "198.20.0.1",
        "192.0.1.1",
        "223.255.255.254",
    ] {
        assert!(!is_internal_ip(ip(text)), "{text}는 내부망이 아니다");
    }
}

// 검증: E38
#[tokio::test]
async fn 공인_주소로만_풀리면_해석한_주소_그대로_고정하고_해석은_한_번이다() {
    let resolver = FakeResolver::answering(&["9.9.9.9", "2606:4700::1111"]);
    let pinned = pin_endpoint("https://push.invalid/send/abc", resolver.as_ref())
        .await
        .unwrap();
    assert_eq!(pinned.host, "push.invalid");
    assert_eq!(
        pinned.addrs,
        vec![
            SocketAddr::new(ip("9.9.9.9"), 443),
            SocketAddr::new(ip("2606:4700::1111"), 443),
        ]
    );
    assert_eq!(resolver.calls(), vec![("push.invalid".to_owned(), 443)]);
}

// 검증: E38
#[tokio::test]
async fn 주소에_포트가_있으면_그_포트로_해석하고_고정한다() {
    let resolver = FakeResolver::answering(&["9.9.9.9"]);
    let pinned = pin_endpoint("https://push.invalid:8443/send/abc", resolver.as_ref())
        .await
        .unwrap();
    assert_eq!(pinned.addrs, vec![SocketAddr::new(ip("9.9.9.9"), 8443)]);
    assert_eq!(resolver.calls(), vec![("push.invalid".to_owned(), 8443)]);
}

// 검증: E38
#[tokio::test]
async fn 해석된_주소가_하나라도_내부망이면_막는다() {
    for answer in [
        vec!["10.0.0.7"],
        vec!["9.9.9.9", "127.0.0.1"],
        vec!["::1"],
        vec!["2606:4700::1111", "fd00::5"],
    ] {
        let resolver = FakeResolver::answering(&answer);
        let result = pin_endpoint("https://push.invalid/send/abc", resolver.as_ref()).await;
        assert!(
            matches!(result, Err(Error::InternalAddress(_))),
            "{answer:?}는 막아야 한다: {result:?}"
        );
    }
}

// 검증: E38
#[tokio::test]
async fn 이름이_ip_글자면_해석하지_않고_그_ip를_검사한다() {
    for endpoint in [
        "https://127.0.0.1/send",
        "https://10.1.2.3:8443/send",
        "https://169.254.169.254/latest",
        "https://[::1]/send",
        "https://[fe80::1]/send",
    ] {
        let resolver = FakeResolver::answering(&["9.9.9.9"]);
        let result = pin_endpoint(endpoint, resolver.as_ref()).await;
        assert!(
            matches!(result, Err(Error::InternalAddress(_))),
            "{endpoint}는 막아야 한다: {result:?}"
        );
        assert!(
            resolver.calls().is_empty(),
            "{endpoint}: IP 글자는 해석하지 않는다"
        );
    }

    let resolver = FakeResolver::answering(&["127.0.0.1"]);
    let pinned = pin_endpoint("https://[2606:4700::1111]/send", resolver.as_ref())
        .await
        .unwrap();
    assert_eq!(pinned.host, "2606:4700::1111");
    assert_eq!(
        pinned.addrs,
        vec![SocketAddr::new(ip("2606:4700::1111"), 443)]
    );
    assert!(resolver.calls().is_empty());
}

// 검증: E38
#[tokio::test]
async fn localhost_이름도_해석한_주소로_막는다() {
    let resolver = FakeResolver::answering(&["127.0.0.1", "::1"]);
    let result = pin_endpoint("https://localhost/send", resolver.as_ref()).await;
    assert!(matches!(result, Err(Error::InternalAddress(_))));
}

// 검증: E38
#[tokio::test]
async fn 해석이_실패하거나_비었거나_주소_모양이_아니면_보내기_오류다() {
    let failing = FakeResolver::failing();
    assert!(matches!(
        pin_endpoint("https://push.invalid/send", failing.as_ref()).await,
        Err(Error::Transport(_))
    ));
    let empty = FakeResolver::answering(&[]);
    assert!(matches!(
        pin_endpoint("https://push.invalid/send", empty.as_ref()).await,
        Err(Error::Transport(_))
    ));
    let unused = FakeResolver::answering(&["9.9.9.9"]);
    assert!(matches!(
        pin_endpoint("주소가 아니다", unused.as_ref()).await,
        Err(Error::Transport(_))
    ));
    assert!(unused.calls().is_empty());
}

// 검증: E38
#[tokio::test]
async fn 실제_보내는_길도_내부망으로_풀리면_접속하지_않고_막는다() {
    let resolver = FakeResolver::answering(&["192.168.0.10"]);
    let transport = HttpTransport::with_resolver(Duration::from_secs(5), resolver.clone()).unwrap();
    let result = transport
        .post(request("https://push.invalid/send/abc"))
        .await;
    assert!(
        matches!(result, Err(Error::InternalAddress(_))),
        "{result:?}"
    );
    assert_eq!(resolver.calls(), vec![("push.invalid".to_owned(), 443)]);

    let literal = FakeResolver::answering(&["9.9.9.9"]);
    let transport = HttpTransport::with_resolver(Duration::from_secs(5), literal.clone()).unwrap();
    let result = transport.post(request("https://127.0.0.1:9/send")).await;
    assert!(matches!(result, Err(Error::InternalAddress(_))));
    assert!(literal.calls().is_empty());
}

// 검증: E38 N5
#[tokio::test]
async fn 내부망_구독은_실패로_알리고_구독은_남고_이벤트는_쌓인다() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("pi.sqlite")).unwrap();
    let resolver = FakeResolver::answering(&["10.0.0.7"]);
    let transport = HttpTransport::with_resolver(Duration::from_secs(5), resolver.clone()).unwrap();
    let notifier = Notifier::new(NotifierConfig {
        store: store.clone(),
        vapid: VapidKey::generate(),
        subject: SUBJECT.to_owned(),
        transport: Arc::new(transport),
        clock: FakeClock::at(T0),
        redactor: Arc::new(MaskSecret),
        quiet_hours: None,
        attempt_timeout: Duration::from_secs(2),
    });
    let subscriber = Subscriber::new(1);
    notifier.subscribe(subscriber.new_subscription()).unwrap();

    let delivery = notifier
        .notify(notice(pi_notify::NoticeKind::Anomaly, T0))
        .await
        .unwrap();
    assert!(delivery.seq > 0);
    assert_eq!(delivery.attempts.len(), 1);
    match &delivery.attempts[0].outcome {
        AttemptOutcome::Failed(message) => {
            assert!(message.contains("내부망"), "{message}");
        }
        other => panic!("내부망이면 실패여야 한다: {other:?}"),
    }
    assert_eq!(store.push_subscriptions().unwrap().len(), 1);
    assert_eq!(resolver.calls().len(), 1);
}

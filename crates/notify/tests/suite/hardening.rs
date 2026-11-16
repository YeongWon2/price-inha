//! 굳히기(notify 4차): 내부망 판정에 Teredo · site-local, 키 파일이 링크일 때, 동시에 처음 켜기.

use std::fs;
use std::net::IpAddr;
use std::sync::{Arc, Barrier};

use pi_notify::{Error, VAPID_KEY_FILE, VapidKey, is_internal_ip};

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

// 검증: E38
#[test]
fn teredo와_폐기된_site_local_주소는_내부망이다() {
    for text in [
        "2001::1",
        "2001:0:4136:e378:8000:63bf:3fff:fdd2",
        "2001:0:ffff:ffff:ffff:ffff:ffff:ffff",
        "fec0::1",
        "feff:ffff::1",
    ] {
        assert!(is_internal_ip(ip(text)), "{text}는 내부망이다");
    }
}

// 검증: E38
#[test]
fn teredo와_인접한_공개_주소는_내부망이_아니다() {
    for text in [
        "2001:1::1",
        "2001:4860:4860::8888",
        "2606:4700::1111",
        "fe00::1",
    ] {
        assert!(!is_internal_ip(ip(text)), "{text}는 내부망이 아니다");
    }
}

// 검증: N5
#[cfg(unix)]
#[test]
fn 키_파일이_링크이면_따라가지_않고_멈추며_가리킨_파일을_건드리지_않는다() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let dir = tempfile::tempdir().unwrap();
    let secret_dir = dir.path().join("secrets");
    fs::create_dir_all(&secret_dir).unwrap();
    let original = dir.path().join("elsewhere.key");
    fs::write(&original, "x\n").unwrap();
    fs::set_permissions(&original, fs::Permissions::from_mode(0o644)).unwrap();
    symlink(&original, secret_dir.join(VAPID_KEY_FILE)).unwrap();

    let result = VapidKey::load_or_create(&secret_dir);
    assert!(matches!(result, Err(Error::KeyFile(_))), "{result:?}");
    let mode = fs::metadata(&original).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644, "가리킨 파일의 권한을 바꾸면 안 된다");
    assert_eq!(fs::read_to_string(&original).unwrap(), "x\n");
}

// 검증: N5
#[test]
fn 여러_스레드가_같은_폴더에서_동시에_처음_켜도_모두_같은_키를_받는다() {
    for round in 0..10 {
        let dir = tempfile::tempdir().unwrap();
        let secret_dir = Arc::new(dir.path().join(format!("secrets-{round}")));
        let gate = Arc::new(Barrier::new(12));
        let workers: Vec<_> = (0..12)
            .map(|_| {
                let (secret_dir, gate) = (secret_dir.clone(), gate.clone());
                std::thread::spawn(move || {
                    gate.wait();
                    VapidKey::load_or_create(&secret_dir).map(|key| key.public_key_b64url())
                })
            })
            .collect();
        let keys: Vec<String> = workers
            .into_iter()
            .map(|w| w.join().unwrap().expect("동시에 켜도 오류가 없어야 한다"))
            .collect();
        assert!(keys.windows(2).all(|w| w[0] == w[1]), "키가 서로 다르다");
        let saved = VapidKey::load_or_create(&secret_dir).unwrap();
        assert_eq!(saved.public_key_b64url(), keys[0]);
    }
}

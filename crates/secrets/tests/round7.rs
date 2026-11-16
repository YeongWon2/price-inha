//! pi-secrets 7차 작업 테스트(후보 일괄 처리). 기준: `docs/dev/modules/secrets.md`의 "7차 후보" 1~3번.
//!
//! - 손으로 짧게 고친 VAPID 값은 "없음"으로 보고 덮어쓰지 않고 `Corrupt`로 알린다.
//! - 여러 스레드가 처음 켜도 VAPID는 한 번만 만든다. 깨진 VAPID 줄도 `Corrupt`이고 덮어쓰지 않는다.
//! - `contains_registered`는 큰 바이트(수십 MB)도 빠르게 훑는다.
//!
//! 보관 폴더는 모두 임시 폴더다. 비밀 값은 진짜 키처럼 보이지 않는 글자만 쓴다.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use pi_secrets::{ENV_FILE, SecretStore, SecretString, SecretsError};
use tempfile::tempdir;

const VAPID: &str = "시험용-브이에이피드-비밀-값-0001";
const VAPID_ENV: &str = "PRICE_INHA_VAPID_PRIVATE_KEY";

fn open(dir: &Path) -> SecretStore {
    SecretStore::open(dir).unwrap()
}

/// 키 파일을 손으로 쓴다.
fn write_env(dir: &Path, text: &str) {
    fs::write(dir.join(ENV_FILE), text).unwrap();
}

fn read_env(dir: &Path) -> String {
    fs::read_to_string(dir.join(ENV_FILE)).unwrap()
}

// 검증: E39
#[test]
fn 손으로_짧게_고친_vapid_값은_덮어쓰지_않고_깨졌다고_알린다() {
    let dir = tempdir().unwrap();
    let hand = format!("{VAPID_ENV}=짧은값\n");
    write_env(dir.path(), &hand);
    let store = open(dir.path());
    let made = AtomicUsize::new(0);

    let result = store.vapid_private_key(|| {
        made.fetch_add(1, Ordering::SeqCst);
        SecretString::new(VAPID)
    });

    assert!(
        matches!(result, Err(SecretsError::Corrupt { file }) if file == ENV_FILE),
        "{result:?}"
    );
    assert_eq!(made.load(Ordering::SeqCst), 0, "만들지 않는다");
    assert_eq!(read_env(dir.path()), hand, "손으로 쓴 줄을 덮어쓰지 않는다");
}

// 검증: E39
#[test]
fn 비어_있는_vapid_줄은_없음으로_보고_새로_만든다() {
    let dir = tempdir().unwrap();
    write_env(dir.path(), &format!("{VAPID_ENV}=\nOTHER_NAME=keep\n"));
    let store = open(dir.path());

    let key = store
        .vapid_private_key(|| SecretString::new(VAPID))
        .unwrap();

    assert_eq!(key.expose(), VAPID);
    assert!(read_env(dir.path()).contains("OTHER_NAME=keep"));
}

// 검증: E39
#[test]
fn 깨진_vapid_줄은_덮어쓰지_않고_깨졌다고_알린다() {
    let dir = tempdir().unwrap();
    let broken = format!("{VAPID_ENV}=\"닫히지 않음\nOTHER_NAME=keep\n");
    write_env(dir.path(), &broken);
    let store = open(dir.path());
    let made = AtomicUsize::new(0);

    let result = store.vapid_private_key(|| {
        made.fetch_add(1, Ordering::SeqCst);
        SecretString::new(VAPID)
    });

    assert!(matches!(result, Err(SecretsError::Corrupt { .. })));
    assert_eq!(made.load(Ordering::SeqCst), 0);
    assert_eq!(read_env(dir.path()), broken);
}

// 검증: E39
#[test]
fn 여러_스레드가_함께_처음_켜도_vapid는_한_번만_만든다() {
    const THREADS: usize = 8;
    let dir = tempdir().unwrap();
    let store = Arc::new(open(dir.path()));
    let made = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|n| {
            let (store, made, barrier) = (store.clone(), made.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                store
                    .vapid_private_key(|| {
                        made.fetch_add(1, Ordering::SeqCst);
                        SecretString::new(format!("시험용-브이에이피드-스레드-{n:04}"))
                    })
                    .unwrap()
                    .expose()
                    .to_owned()
            })
        })
        .collect();
    let values: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    assert_eq!(made.load(Ordering::SeqCst), 1, "값을 한 번만 만든다");
    assert!(values.iter().all(|value| value == &values[0]), "{values:?}");
    assert_eq!(
        read_env(dir.path()).matches(VAPID_ENV).count(),
        1,
        "파일에도 한 줄뿐이다"
    );
}

// 검증: E39 N8
#[test]
fn 수십_메가_바이트도_빠르게_훑고_맨_끝의_값도_찾는다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    store
        .vapid_private_key(|| SecretString::new(VAPID))
        .unwrap();

    // 등록된 값의 첫 글자와 `%`가 자주 나오는 글(가장 나쁜 경우에 가깝다).
    let filler = "시험용-% 시험%2 용-시험 ";
    let mut bytes: Vec<u8> = filler.as_bytes().repeat(40 * 1024 * 1024 / filler.len());
    let started = Instant::now();
    assert!(!store.contains_registered(&bytes));
    bytes.extend_from_slice(VAPID.as_bytes());
    assert!(store.contains_registered(&bytes));
    let took = started.elapsed();

    assert!(took < Duration::from_secs(12), "{took:?}");
}

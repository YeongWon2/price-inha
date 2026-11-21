//! 터미널의 접속 키: 한 번만 보여 주고, 저장은 해시(화면 경로와 같은 저장소 · 같은 규칙),
//! 두 번째는 `--rotate` 없이 거부, 답 밖(파일 · 기록)에 키 글자 0번.

use pi_store::Store;
use price_inha::{AccessKeyRequest, CliError, DB_FILE, access_key};
use sha2::{Digest, Sha256};

use crate::common::{files_containing, settings};

fn salted(salt: &[u8], key: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(key.as_bytes());
    hasher.finalize().into()
}

// 검증: D2 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 한_번만_보여_주고_해시만_저장한다() {
    let home = tempfile::tempdir().unwrap();
    let shown = access_key(
        &settings(home.path(), "127.0.0.1"),
        &AccessKeyRequest::default(),
    )
    .await
    .expect("처음은 만든다");
    let key = shown.key().to_string();
    assert!(key.len() >= 16, "충분히 긴 키");
    assert_eq!(shown.generation(), 1);
    assert_eq!(shown.text().matches(&key).count(), 1, "글에 한 번만");
    assert!(!format!("{shown:?}").contains(&key), "Debug에 키가 없다");

    let store = Store::open(home.path().join(DB_FILE)).unwrap();
    let record = store
        .access_key()
        .unwrap()
        .expect("화면과 같은 저장소에 둔다");
    assert_eq!(record.generation, 1);
    assert_eq!(
        record.sha256,
        salted(&record.salt, &key),
        "소금 친 SHA-256(화면 경로와 같은 규칙)"
    );
    assert!(
        !store.access_log().unwrap().is_empty(),
        "바꾼 것을 기록한다"
    );
    drop(store);
    assert!(
        files_containing(home.path(), &key).is_empty(),
        "파일 · 기록에 키 글자가 없다"
    );
}

// 검증: D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 두_번째는_rotate_없이는_거부하고_rotate면_새_세대() {
    let home = tempfile::tempdir().unwrap();
    let settings = settings(home.path(), "127.0.0.1");
    let first = access_key(&settings, &AccessKeyRequest::default())
        .await
        .unwrap();
    let refused = access_key(&settings, &AccessKeyRequest::default()).await;
    assert!(
        matches!(refused, Err(CliError::AccessKeyExists)),
        "{refused:?}"
    );
    let store = Store::open(home.path().join(DB_FILE)).unwrap();
    assert_eq!(
        store.access_key().unwrap().unwrap().generation,
        1,
        "거부하면 그대로"
    );
    drop(store);

    let rotate = AccessKeyRequest {
        rotate: true,
        remote_name: None,
    };
    let second = access_key(&settings, &rotate).await.expect("바꾼다");
    assert_eq!(second.generation(), 2);
    assert_ne!(second.key(), first.key());
    let store = Store::open(home.path().join(DB_FILE)).unwrap();
    let record = store.access_key().unwrap().unwrap();
    assert_eq!(record.sha256, salted(&record.salt, second.key()));
    drop(store);
    for key in [first.key(), second.key()] {
        assert!(files_containing(home.path(), key).is_empty());
    }
}

// 검증: D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 원격_이름이_있으면_https_주소를_함께_안내한다() {
    let home = tempfile::tempdir().unwrap();
    let settings = settings(home.path(), "127.0.0.1");
    let with_remote = AccessKeyRequest {
        rotate: false,
        remote_name: Some("mini-pc.example.test".to_string()),
    };
    let shown = access_key(&settings, &with_remote).await.unwrap();
    assert!(
        shown.text().contains("https://mini-pc.example.test/"),
        "{}",
        shown.text().replace(shown.key(), "<키>")
    );

    let plain_home = tempfile::tempdir().unwrap();
    let shown = access_key(
        &crate::common::settings(plain_home.path(), "127.0.0.1"),
        &AccessKeyRequest::default(),
    )
    .await
    .unwrap();
    assert!(
        !shown.text().contains("https://"),
        "원격 이름이 없으면 안내하지 않는다"
    );
}

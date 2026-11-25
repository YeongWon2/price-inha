//! 진짜면 지금처럼: 이 보관 폴더의 진짜 서버에는 `open --print` · `access-key`가 지금처럼 되고(37차 시험도
//! 그대로 통과한다), 증표를 아는 도우미(0600 증표 파일의 증표로 맞게 답한다)에게도 `open`이 주소를 낸다 —
//! CLI는 매번 새 nonce로 묻는다(38차 3번).

use price_inha::{WHOAMI_NONCE_MAX_LEN, WHOAMI_NONCE_MIN_LEN};

use crate::common::{Impostor, Root, TOKEN_HOME, Whoami, close, launch, write_proof};
use crate::run::{Env, run_binary, strings};

fn open_args(root: &Root, port: u16) -> Vec<String> {
    strings(&[
        "open",
        "--print",
        "--home",
        root.home_str(),
        "--port",
        &port.to_string(),
    ])
}

// 검증: D1 V41
#[tokio::test(flavor = "multi_thread")]
async fn 진짜_서버면_open과_access_key가_지금처럼_된다() {
    let root = Root::new();
    let app = launch(&root.home).await;
    let port = app.local_addr().port();

    let ran = run_binary(Env::new(&root), open_args(&root, port), None).await;
    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert!(
        ran.stdout.contains(&format!("http://127.0.0.1:{port}/")),
        "{}",
        ran.all()
    );

    let ran = run_binary(
        Env::new(&root),
        strings(&[
            "access-key",
            "--home",
            root.home_str(),
            "--port",
            &port.to_string(),
        ]),
        None,
    )
    .await;
    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert!(
        ran.stdout.contains("도는 서버가 만들었습니다"),
        "도는 서버가 키를 만들었다: {}",
        ran.all()
    );
    let store = app.store();
    assert_eq!(
        store.access_key().unwrap().map(|record| record.generation),
        Some(1),
        "도는 서버의 저장소에 첫 세대가 생겼다"
    );
    close(app).await;
}

// 검증: D1 V46
#[tokio::test(flavor = "multi_thread")]
async fn 증표를_아는_서버에는_open이_주소를_내고_nonce는_매번_새것이다() {
    let root = Root::new();
    write_proof(&root, TOKEN_HOME, 0o600);
    let helper = Impostor::start(Whoami::Proves(TOKEN_HOME.to_string())).await;
    let port = helper.port();
    for _ in 0..2 {
        let ran = run_binary(Env::new(&root), open_args(&root, port), None).await;
        assert_eq!(ran.code, Some(0), "{}", ran.all());
        assert!(
            ran.stdout.contains(&format!("http://127.0.0.1:{port}/")),
            "{}",
            ran.all()
        );
    }
    let nonces = helper.nonces().await;
    assert!(nonces.len() >= 2, "열 때마다 확인한다: {nonces:?}");
    for nonce in &nonces {
        assert!(
            (WHOAMI_NONCE_MIN_LEN..=WHOAMI_NONCE_MAX_LEN).contains(&nonce.len())
                && nonce.bytes().all(|b| b.is_ascii_hexdigit()),
            "nonce는 계약의 모양이다: {nonce}"
        );
    }
    let mut distinct = nonces.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), nonces.len(), "nonce는 매번 새것이다");
}

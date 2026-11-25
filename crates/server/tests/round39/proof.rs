//! 서버 쪽 실행 증표: 켤 때 보관 폴더에 0600으로 쓰고 끌 때 지운다 · 켤 때마다 다르다 · `whoami`는 그 컴퓨터의
//! 세션에만 증표와 nonce로 만든 요약값을 준다 · nonce가 틀리면 400 · 증표 글자는 응답 · 기록 · 다른 파일
//! 어디에도 없다(38차 1 · 2번).

use std::time::{Duration, Instant};

use price_inha::{
    RUN_PROOF_DOMAIN, SETUP_PATH, WHOAMI_NONCE_FIELD, WHOAMI_NONCE_MAX_LEN, WHOAMI_NONCE_MIN_LEN,
    WHOAMI_PROOF_FIELD, run_proof,
};

use crate::common::{
    Root, STEP, TOKEN_HOME, TOKEN_OTHER, close, enter, expected_proof, free_port, is_token,
    issue_on_screen, launch, local_session, nonce_body, other_files_with, proof_path, read_proof,
    whoami, write_proof,
};
use crate::run::{Env, Wizard};

/// 16진수 nonce(시험이 정한 글자).
fn nonce(len: usize, seed: char) -> String {
    std::iter::repeat_n(seed, len).collect()
}

// 검증: D2 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 서버를_켜면_증표_파일이_0600으로_생기고_끄면_없어진다() {
    let root = Root::new();
    let app = launch(&root.home).await;
    let (token, mode) = read_proof(&root.home).expect("켜면 보관 폴더에 증표 파일이 있다");
    assert_eq!(mode, 0o600, "증표 파일은 0600이다");
    assert!(
        is_token(&token),
        "증표는 소문자 16진수 64글자뿐이다(32바이트): {} 글자",
        token.len()
    );
    close(app).await;
    assert!(
        !proof_path(&root.home).exists(),
        "끄면 증표 파일이 없어진다"
    );
}

// 검증: D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 켤_때마다_증표가_다르고_남은_파일은_새_증표_0600으로_바꾼다() {
    let root = Root::new();
    // 끝내 지우지 못하고 죽은 서버가 남긴 넓은 파일.
    write_proof(&root, TOKEN_OTHER, 0o644);
    let app = launch(&root.home).await;
    let (first, mode) = read_proof(&root.home).expect("증표 파일");
    assert_eq!(mode, 0o600, "남은 파일도 0600으로 바꿔 쓴다");
    assert_ne!(first, TOKEN_OTHER, "남은 증표를 그대로 쓰지 않는다");
    assert!(is_token(&first));
    close(app).await;

    let app = launch(&root.home).await;
    let (second, _) = read_proof(&root.home).expect("다시 켜면 증표 파일");
    assert!(is_token(&second));
    assert_ne!(first, second, "켤 때마다 새 증표다");
    close(app).await;
}

// 검증: C15 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn whoami는_증표와_nonce로_만든_요약값을_준다() {
    let root = Root::new();
    let app = launch(&root.home).await;
    let addr = app.local_addr();
    let (token, _) = read_proof(&root.home).expect("증표 파일");
    let cookie = local_session(addr).await;

    let mut seen = Vec::new();
    for asked in [
        nonce(WHOAMI_NONCE_MIN_LEN, 'a'),
        nonce(64, '7'),
        "0f1E2d3C4b5A69780f1E2d3C4b5A6978".to_string(),
        nonce(WHOAMI_NONCE_MAX_LEN, 'f'),
    ] {
        let reply = whoami(addr, Some(&cookie), &nonce_body(&asked), false).await;
        assert_eq!(reply.status, 200, "맞는 nonce에는 답한다: {}", reply.text);
        let proof = reply.body[WHOAMI_PROOF_FIELD]
            .as_str()
            .expect("답에 요약값이 있다")
            .to_string();
        assert_eq!(
            proof,
            expected_proof(&token, &asked),
            "요약값은 계약의 식(증표 · nonce)이다"
        );
        assert_eq!(proof, run_proof(&token, &asked));
        assert!(!reply.text.contains(&token), "답에 증표 글자가 없다");
        seen.push(proof);
    }
    seen.dedup();
    assert_eq!(seen.len(), 4, "nonce가 다르면 요약값도 다르다");

    // 다른 응답에도 증표가 없다.
    let origin = format!("http://127.0.0.1:{}", addr.port());
    let status = reqwest::Client::new()
        .get(format!("{origin}{SETUP_PATH}"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!status.contains(&token), "설정 상태에 증표 글자가 없다");
    close(app).await;
}

// 검증: C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonce가_없거나_짧거나_길거나_모양이_틀리면_400이고_요약값이_없다() {
    let root = Root::new();
    let app = launch(&root.home).await;
    let addr = app.local_addr();
    let cookie = local_session(addr).await;
    let bad = [
        ("짧다", nonce_body(&nonce(WHOAMI_NONCE_MIN_LEN - 1, 'a'))),
        ("길다", nonce_body(&nonce(WHOAMI_NONCE_MAX_LEN + 1, 'a'))),
        (
            "16진수가 아니다",
            nonce_body(&nonce(WHOAMI_NONCE_MIN_LEN, 'z')),
        ),
        (
            "공백이 섞였다",
            nonce_body(&format!("{} ", nonce(WHOAMI_NONCE_MIN_LEN, 'a'))),
        ),
        ("빈 글", nonce_body("")),
        ("칸이 없다", "{}".to_string()),
        (
            "글이 아니다",
            serde_json::json!({ WHOAMI_NONCE_FIELD: 12345 }).to_string(),
        ),
        ("JSON이 아니다", "nonce=abc".to_string()),
    ];
    for (why, body) in bad {
        let reply = whoami(addr, Some(&cookie), &body, false).await;
        assert_eq!(reply.status, 400, "{why}: {}", reply.text);
        assert!(
            reply.body.get(WHOAMI_PROOF_FIELD).is_none(),
            "{why}: 요약값을 싣지 않는다"
        );
    }
    close(app).await;
}

// 검증: C1 C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 원격_상대에게는_요약값을_주지_않는다() {
    let root = Root::new();
    let app = launch(&root.home).await;
    let addr = app.local_addr();
    let body = nonce_body(&nonce(WHOAMI_NONCE_MIN_LEN, 'b'));

    // 같은 컴퓨터에서는 답한다(이 시험이 경로 없음으로 통과하지 않게).
    let cookie = local_session(addr).await;
    assert_eq!(whoami(addr, Some(&cookie), &body, false).await.status, 200);

    // 접속 키로 들어온 원격 세션.
    let key = issue_on_screen(addr).await;
    let remote = enter(addr, Some(&key), true)
        .await
        .cookie
        .expect("맞는 키로 원격 세션을 받는다");
    for (why, reply) in [
        ("원격 세션", whoami(addr, Some(&remote), &body, true).await),
        (
            "그 컴퓨터 세션에 전달 헤더",
            whoami(addr, Some(&cookie), &body, true).await,
        ),
        ("세션 없음", whoami(addr, None, &body, true).await),
    ] {
        assert_ne!(reply.status, 200, "{why}: 답하지 않는다");
        assert!(
            reply.body.get(WHOAMI_PROOF_FIELD).is_none() && !reply.text.contains("proof"),
            "{why}: 요약값이 없다: {}",
            reply.text
        );
    }
    close(app).await;
}

// 검증: N8
#[test]
fn 요약값은_계약의_식이고_증표를_담지_않는다() {
    let asked = nonce(WHOAMI_NONCE_MIN_LEN, 'c');
    let proof = run_proof(TOKEN_HOME, &asked);
    assert_eq!(
        proof,
        expected_proof(TOKEN_HOME, &asked),
        "앞머리 {RUN_PROOF_DOMAIN}"
    );
    assert!(is_token(&proof), "소문자 16진수 64글자");
    assert!(!proof.contains(TOKEN_HOME));
    assert_ne!(
        proof,
        run_proof(TOKEN_OTHER, &asked),
        "증표가 다르면 요약값도 다르다"
    );
    assert_ne!(
        proof,
        run_proof(TOKEN_HOME, &nonce(WHOAMI_NONCE_MIN_LEN, 'd')),
        "nonce가 다르면 요약값도 다르다"
    );
    // nonce 글자는 바꾸지 않고 쓴다(대소문자 그대로).
    let upper = asked.to_uppercase();
    assert_eq!(
        run_proof(TOKEN_HOME, &upper),
        expected_proof(TOKEN_HOME, &upper)
    );
    assert_ne!(run_proof(TOKEN_HOME, &upper), proof);
}

// 검증: N8 E38 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실행_파일로_띄운_서버는_기록과_보관_폴더에_증표를_남기지_않고_끄기_신호에_파일을_지운다() {
    let root = Root::new();
    let port = free_port();
    let log = root.user.join("server.log");
    let env = Env::new(&root);
    let args = [
        "run".to_string(),
        "--home".to_string(),
        root.home_str().to_string(),
        "--port".to_string(),
        port.to_string(),
        "--log-file".to_string(),
        log.to_str().unwrap().to_string(),
    ];
    let server = tokio::task::spawn_blocking(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        Wizard::spawn(&env, &args, false)
    })
    .await
    .unwrap();

    // 세션이 답할 때까지(서버가 뜰 때까지) 기다린다.
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + STEP;
    let cookie = loop {
        let origin = format!("http://127.0.0.1:{port}");
        let asked = reqwest::Client::new()
            .post(format!("{origin}{}", pi_gate::SESSION_PATH))
            .header("Origin", &origin)
            .send()
            .await;
        if let Ok(reply) = asked
            && reply.status().as_u16() == 204
        {
            break local_session(addr).await;
        }
        assert!(Instant::now() < deadline, "실행 파일의 서버가 떠야 한다");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let token = loop {
        if let Some((token, mode)) = read_proof(&root.home) {
            assert_eq!(mode, 0o600);
            break token;
        }
        assert!(
            Instant::now() < deadline,
            "실행 파일의 서버도 증표 파일을 쓴다"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let asked = nonce(WHOAMI_NONCE_MIN_LEN, 'e');
    let reply = whoami(addr, Some(&cookie), &nonce_body(&asked), false).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        reply.body[WHOAMI_PROOF_FIELD].as_str(),
        Some(expected_proof(&token, &asked).as_str())
    );

    // 끄기 신호(SIGTERM).
    let pid = server.id().to_string();
    let signalled = std::process::Command::new("/bin/kill")
        .args(["-TERM", &pid])
        .status()
        .unwrap();
    assert!(signalled.success());
    let ran = tokio::task::spawn_blocking(move || server.finish())
        .await
        .unwrap();
    assert!(
        !proof_path(&root.home).exists(),
        "끄기 신호로 끝나면 증표 파일을 지운다: {}",
        ran.all()
    );
    assert!(!ran.all().contains(&token), "출력에 증표 글자가 없다");
    let written = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(!written.is_empty(), "기록 파일에 무엇인가 남았다");
    assert!(!written.contains(&token), "기록 파일에 증표 글자가 없다");
    assert!(
        other_files_with(&root.home, &token).is_empty(),
        "보관 폴더의 다른 파일(저장소 · 이벤트 포함)에 증표 글자가 없다"
    );
}

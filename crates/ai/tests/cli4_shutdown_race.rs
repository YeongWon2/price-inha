//! pi-ai 4차 1번: 호출이 띄우는 중에 `shutdown()`이 와도 자식이 남지 않는다.
//!
//! 자리표를 받은 뒤 명부에 적히기 전(시스템 글 파일 쓰기 · 띄우기 사이)에 끝내기가 오면,
//! 끝내기는 그 호출을 몰라 기다리지 않는다. 그때는 그 호출이 돌아가기 전에 자식을 끝내고
//! 거둬야 한다(또는 끝내기가 그 호출을 세어 기다려야 한다). 어느 쪽이든 **끝내기와 호출이 모두
//! 돌아온 뒤에는 이 프로세스의 자식이 하나도 없다**(살아 있는 것도, 거두지 않은 좀비도).
//!
//! 이 프로세스의 자식 전부를 세므로 이 파일에는 테스트를 하나만 둔다(다른 테스트와 같은 프로세스에서
//! 돌면 그 자식이 섞인다). 실제 `claude`를 부르지 않고 임시 폴더의 가짜 셸 스크립트만 띄운다.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_ai::{AiProvider, ClaudeCliConfig, ClaudeCliProvider, CompletionRequest, Message, Role};

/// 이 프로세스의 자식(번호 · 상태). `ps` 자신과, 공급자가 신호를 보내려고 잠깐 띄우는
/// `/bin/kill`은 뺀다(좀비는 이름 없이 `<defunct>`로 보이므로 빠지지 않는다).
fn children() -> Vec<(u32, String)> {
    let me = std::process::id();
    let ps = Command::new("/bin/ps")
        .args(["-ax", "-o", "pid=,ppid=,stat=,comm="])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let ps_pid = ps.id();
    let out = ps.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let pid: u32 = cols.next()?.parse().ok()?;
            let ppid: u32 = cols.next()?.parse().ok()?;
            let stat = cols.next().unwrap_or("").to_string();
            let comm = cols.collect::<Vec<_>>().join(" ");
            let is_kill = comm == "kill" || comm.ends_with("/kill");
            (ppid == me && pid != ps_pid && !is_kill).then_some((pid, format!("{stat} {comm}")))
        })
        .collect()
}

/// 손을 놓고 기다린다(1ms보다 잘게 끝내기 시각을 옮기려고).
fn spin(d: Duration) {
    let started = Instant::now();
    while started.elapsed() < d {
        std::hint::spin_loop();
    }
}

// 검증: A11
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 띄우는_중에_끝내기가_와도_끝내기와_호출이_돌아온_뒤_자식이_남지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let work = root.join("work");
    fs::create_dir(&work).unwrap();
    let bin = root.join("claude");
    // 표준 입력을 다 읽고 곧 끝나는 가짜(결과 줄은 내지 않는다).
    fs::write(&bin, "#!/bin/sh\n/bin/cat > /dev/null\nexit 0\n").unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    // 시스템 글 파일을 쓰는 동안 "자리표는 받았고 아직 명부에 없는" 틈이 넓어진다.
    let system = "가".repeat(1_500_000);
    let mut leftovers = Vec::new();

    for step in 0..60u64 {
        let mut cfg = ClaudeCliConfig::new(
            Some(bin.clone()),
            work.clone(),
            vec![("PATH".to_string(), "/usr/bin:/bin".to_string())],
            Duration::from_secs(10),
        );
        // 유예가 길면, 등록이 거부된 자식을 뒤에서 늦게 치울 때 그 자식이 오래 남아 잘 보인다.
        cfg.kill_grace = Duration::from_secs(2);
        let provider = Arc::new(ClaudeCliProvider::new(cfg));
        let request = CompletionRequest {
            model: "siheom-model-g".to_string(),
            system: Some(system.clone()),
            messages: vec![Message {
                role: Role::User,
                content: "질문".to_string(),
            }],
            max_tokens: 10,
        };

        let running = {
            let provider = provider.clone();
            tokio::spawn(async move { provider.complete(request).await })
        };
        // 끝내기 시각을 0 ~ 약 12ms 사이로 옮겨 가며 띄우는 틈을 노린다.
        spin(Duration::from_micros(step * 200));
        tokio::time::timeout(Duration::from_secs(8), provider.shutdown())
            .await
            .expect("끝내기가 돌아오지 않았다");
        let result = tokio::time::timeout(Duration::from_secs(8), running)
            .await
            .expect("끝낸 호출이 돌아오지 않았다")
            .unwrap();
        assert!(result.is_err(), "끝낸 호출이 성공했다");

        let left = children();
        if !left.is_empty() {
            leftovers.push((step, left));
        }
        // 남은 자식이 다음 차례에 섞이지 않게 치워질 때까지 기다린다(실패는 위에서 이미 적었다).
        let waited = Instant::now();
        while !children().is_empty() && waited.elapsed() < Duration::from_secs(5) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    assert!(
        leftovers.is_empty(),
        "끝내기와 호출이 돌아왔는데 자식이 남았다(차례, [(번호, 상태)]): {leftovers:?}"
    );
}

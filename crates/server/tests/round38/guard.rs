//! 비대화형이면 마법사를 돌리지 않는다(원본 Hermes: 안내 뒤 종료 코드 2). 마법사 · 열기 코드는 다른 프로그램의
//! 토큰 파일 · 키체인을 읽지 않고 OAuth를 하지 않는다(결정 24 · D4) — 읽은 흔적을 볼 길이 없어 코드 검사로 본다.

use std::path::{Path, PathBuf};

use price_inha::{SETUP_NOT_TERMINAL, SETUP_NOT_TERMINAL_EXIT};

use crate::common::{BAIT, FakeCli, Login, Root, files_under};
use crate::run::{Env, run_binary, strings};

// 검증: D1 D4
#[tokio::test(flavor = "multi_thread")]
async fn 비대화형이면_종료_코드_2와_안내만_한다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::In);
    // 표준 입력이 빈 것(터미널이 아니다)이고 `--stdin-answers`도 없다.
    let ran = run_binary(
        Env::new(&root, &cli),
        strings(&["setup", "--home", root.home_str(), "--port", "1"]),
        None,
    )
    .await;
    assert_eq!(
        ran.code,
        Some(i32::from(SETUP_NOT_TERMINAL_EXIT)),
        "{}",
        ran.all()
    );
    assert!(
        ran.all().contains(SETUP_NOT_TERMINAL),
        "안내: {}",
        ran.all()
    );
    assert!(!root.home.exists(), "보관 폴더를 만들지 않는다");
    assert!(cli.calls().is_empty(), "CLI를 찾거나 부르지 않는다");
    ran.assert_absent(BAIT, "미끼 토큰");
}

/// 마법사 · 열기의 소스 파일(`src/cli/setup.rs` · `src/cli/setup/` 아래 · `src/cli/open.rs` · `src/cli/open/` 아래).
fn wizard_sources() -> Vec<PathBuf> {
    let cli = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("cli");
    let mut files = vec![cli.join("setup.rs"), cli.join("open.rs")];
    files.extend(files_under(&cli.join("setup")));
    files.extend(files_under(&cli.join("open")));
    files.into_iter().filter(|file| file.is_file()).collect()
}

// 검증: D4
#[test]
fn 마법사_코드는_다른_프로그램의_토큰_파일을_열지_않는다() {
    // 주석이 아닌 줄에 이 글자가 있으면 다른 프로그램의 자격을 읽거나 OAuth를 하려는 것이다.
    let forbidden = [
        ".credentials.json",
        "Claude Code-credentials",
        ".codex",
        "auth.json",
        "find-generic-password",
        "oauth",
        "OAuth",
        "setup-token",
    ];
    let sources = wizard_sources();
    assert!(
        sources.iter().any(|f| f.ends_with("setup.rs")),
        "마법사 소스가 있어야 한다: {sources:?}"
    );
    for file in sources {
        let text = std::fs::read_to_string(&file).unwrap();
        for (number, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            for word in forbidden {
                assert!(
                    !code.contains(word),
                    "{}:{}에 `{word}`가 있다: {line}",
                    file.display(),
                    number + 1
                );
            }
        }
    }
}

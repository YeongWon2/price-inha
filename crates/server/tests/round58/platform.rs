//! 운영체제마다 다른 자리의 순수 함수(윈도 갈래를 지금 컴퓨터에서 본다).

use price_inha::{
    CLI_ENV_ALLOWLIST, CommandLine, Os, WINDOWS_PERMISSION_NOTE, browser_command,
    claude_file_names, claude_path_candidates, claude_user_location, cli_env_names,
    default_home_text, executable_name, home_permission_note, shared_home_warning,
    terminate_command, this_os,
};

fn line(program: &str, args: &[&str]) -> CommandLine {
    CommandLine {
        program: program.to_string(),
        args: args.iter().map(|a| (*a).to_string()).collect(),
    }
}

// 검증: D1
#[test]
fn 윈도에서는_path를_쌍반점으로_나누고_claude_exe와_claude_cmd를_찾는다() {
    assert_eq!(claude_file_names(Os::Windows), ["claude.exe", "claude.cmd"]);
    let path = r"C:\Users\me\AppData\Roaming\npm;bin;D:\tools\;.";
    assert_eq!(
        claude_path_candidates(Os::Windows, path),
        vec![
            r"C:\Users\me\AppData\Roaming\npm\claude.exe".to_string(),
            r"C:\Users\me\AppData\Roaming\npm\claude.cmd".to_string(),
            r"D:\tools\claude.exe".to_string(),
            r"D:\tools\claude.cmd".to_string(),
        ],
        "상대 경로 항목(bin · .)은 후보가 아니다"
    );
    assert_eq!(claude_user_location(Os::Windows), r".local\bin\claude.exe");
}

// 검증: D1
#[test]
fn 맥과_리눅스에서는_쌍점으로_나누고_claude만_찾는다() {
    for os in [Os::MacOs, Os::Linux] {
        assert_eq!(claude_file_names(os), ["claude"]);
        assert_eq!(
            claude_path_candidates(os, "/opt/homebrew/bin:bin:/usr/local/bin/"),
            vec![
                "/opt/homebrew/bin/claude".to_string(),
                "/usr/local/bin/claude".to_string()
            ]
        );
        assert_eq!(claude_user_location(os), ".local/bin/claude");
    }
}

// 검증: D1
#[test]
fn 브라우저_여는_명령은_운영체제마다_다르다() {
    let url = "http://127.0.0.1:7/office/";
    assert_eq!(
        browser_command(Os::Windows, url),
        line("cmd", &["/c", "start", "", url])
    );
    assert_eq!(browser_command(Os::MacOs, url), line("open", &[url]));
    assert_eq!(browser_command(Os::Linux, url), line("xdg-open", &[url]));
}

// 검증: D1
#[test]
fn 마법사가_띄운_서버를_끄는_명령은_윈도에서_taskkill이다() {
    assert_eq!(
        terminate_command(Os::Windows, 4242),
        line("taskkill", &["/PID", "4242", "/T", "/F"])
    );
    assert_eq!(
        terminate_command(Os::MacOs, 4242),
        line("kill", &["-TERM", "4242"])
    );
}

// 검증: D1
#[test]
fn 실행_파일_이름은_윈도에서_exe가_붙는다() {
    assert_eq!(executable_name(Os::Windows), "price-inha.exe");
    assert_eq!(executable_name(Os::MacOs), "price-inha");
    assert_eq!(executable_name(Os::Linux), "price-inha");
    let expected = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::MacOs
    } else {
        Os::Linux
    };
    assert_eq!(this_os(), expected);
}

// 검증: D4
#[test]
fn 윈도의_cli_자식_환경에는_사용자_폴더와_시스템_폴더_이름이_더해진다() {
    let unix = cli_env_names(Os::MacOs);
    assert_eq!(unix, CLI_ENV_ALLOWLIST.to_vec(), "맥 · 리눅스는 그대로");
    let windows = cli_env_names(Os::Windows);
    for name in CLI_ENV_ALLOWLIST {
        assert!(windows.contains(&name), "{name}");
    }
    for name in [
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "SystemRoot",
        "TEMP",
    ] {
        assert!(
            windows.contains(&name),
            "윈도에서 CLI가 도는 데 필요한 {name}"
        );
    }
    for name in windows {
        assert!(
            !name.contains("KEY") && !name.contains("TOKEN") && !name.contains("SECRET"),
            "비밀 이름은 넘기지 않는다: {name}"
        );
    }
}

// 검증: D1
#[test]
fn 윈도에서는_권한을_검사하지_않는다고_분명히_적는다() {
    assert_eq!(home_permission_note(Os::Windows), WINDOWS_PERMISSION_NOTE);
    assert!(WINDOWS_PERMISSION_NOTE.contains("검사하지 않"));
    assert_ne!(home_permission_note(Os::MacOs), WINDOWS_PERMISSION_NOTE);
}

// 검증: D1
#[test]
fn 윈도에서_공용_폴더에_보관_폴더를_두면_경고한다() {
    for shared in [
        r"C:\Users\Public\price-inha",
        r"C:\ProgramData\price-inha",
        r"C:\price-inha",
        r"c:/users/public/x",
    ] {
        let warning = shared_home_warning(Os::Windows, shared);
        assert!(
            warning.as_deref().is_some_and(|w| w.contains(shared)),
            "{shared}: {warning:?}"
        );
    }
    for private in [r"C:\Users\me\.price-inha", r"D:\Users\me\data\pi"] {
        assert_eq!(shared_home_warning(Os::Windows, private), None, "{private}");
    }
    assert_eq!(shared_home_warning(Os::MacOs, "/tmp/x"), None);
}

// 검증: D1
#[test]
fn 기본_보관_폴더는_사용자_홈_아래_점_price_inha다() {
    assert_eq!(
        default_home_text(Os::Windows, r"C:\Users\me"),
        r"C:\Users\me\.price-inha"
    );
    assert_eq!(
        default_home_text(Os::MacOs, "/Users/me/"),
        "/Users/me/.price-inha"
    );
}

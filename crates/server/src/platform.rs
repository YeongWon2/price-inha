//! (58차) 운영체제마다 다른 자리를 **순수 함수**로 모은다 — 윈도 컴퓨터에서 한 줄로 설치하고 setup까지.
//!
//! 근거: `docs/dev/decisions.md` 결정 24 · 27(윈도를 범위에 넣는다 — 사용자 말 "윈도우 컴퓨터에서 cli를 설치해서
//! 알아서 다 setup이 되게끔"), `docs/research/install-and-cli.md`(원본 OpenClaw · Hermes의 윈도 길).
//! 실제 윈도 컴퓨터 없이도 시험할 수 있게, 운영체제를 인자([`Os`])로 받아 명령 줄 · 경로 글자만 만든다.
//! 부르는 쪽은 [`this_os`]를 넘긴다.

use crate::cli::{CommandLine, Os};

/// Claude Code CLI가 `PATH`에 없을 때의 안내(설치는 사용자가 한다 — 결정 27, 남의 설치 스크립트를 받아 돌리지 않는다).
pub const SETUP_CLI_INSTALL_HINT: &str =
    "Claude Code를 설치한 뒤 `claude auth login`으로 로그인하고 Enter를 누르세요(다시 찾습니다)";
/// Claude Code 공식 문서 주소(`docs/research/cli-backend.md`에 적힌 공식 문서 자리).
pub const CLAUDE_CODE_DOCS_URL: &str = "https://code.claude.com/docs/en/";
/// 윈도에서 보관 폴더 권한을 따지지 않는다는 표시(doctor · setup의 글).
pub const WINDOWS_PERMISSION_NOTE: &str = "윈도에서는 권한을 검사하지 않음";

/// 이 실행 파일의 운영체제. 셋 밖(다른 유닉스)은 Linux로 본다.
pub fn this_os() -> Os {
    Os::current().unwrap_or(Os::Linux)
}

/// `PATH`에서 찾는 CLI 파일 이름(이 순서). 윈도는 공식 설치기의 `claude.exe`, npm 전역 설치의 `claude.cmd`.
pub fn claude_file_names(os: Os) -> &'static [&'static str] {
    match os {
        Os::Windows => &["claude.exe", "claude.cmd"],
        Os::MacOs | Os::Linux => &["claude"],
    }
}

/// `PATH` 값에서 CLI 후보 경로들(절대 경로 항목만, 이 순서). 운영체제의 나눔 글자 · 경로 글자를 쓴다.
/// 상대 경로 항목(`.` · `bin` 등)은 후보가 아니다(서버의 작업 폴더에 따라 다른 파일을 가리키게 된다).
pub fn claude_path_candidates(os: Os, path_var: &str) -> Vec<String> {
    let (split, slash) = separators(os);
    path_var
        .split(split)
        .filter(|dir| is_absolute(os, dir))
        .flat_map(|dir| {
            let dir = dir.trim_end_matches(['/', '\\']).to_string();
            claude_file_names(os)
                .iter()
                .map(move |name| format!("{dir}{slash}{name}"))
        })
        .collect()
}

/// (`PATH` 나눔 글자, 경로 글자).
fn separators(os: Os) -> (char, char) {
    match os {
        Os::Windows => (';', '\\'),
        Os::MacOs | Os::Linux => (':', '/'),
    }
}

/// 그 운영체제의 절대 경로 글자인가(윈도: `C:\` · `C:/` · `\\서버\`).
fn is_absolute(os: Os, text: &str) -> bool {
    match os {
        Os::Windows => {
            let bytes = text.as_bytes();
            let drive = bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && matches!(bytes[2], b'\\' | b'/');
            drive || text.starts_with(r"\\")
        }
        Os::MacOs | Os::Linux => text.starts_with('/'),
    }
}

/// `PATH`에 없을 때 사용자 홈 아래에서 찾는 자리(홈에 이어 붙인다). 공식 설치기가 놓는 곳.
pub fn claude_user_location(os: Os) -> &'static str {
    match os {
        Os::Windows => r".local\bin\claude.exe",
        Os::MacOs | Os::Linux => ".local/bin/claude",
    }
}

/// 주소를 기본 브라우저로 여는 명령 줄(macOS `open`, 윈도 `cmd /c start "" <주소>`, 그 밖은 `xdg-open`).
/// `start`의 첫 따옴표 인자는 창 제목이다(비워 두어야 주소를 제목으로 먹지 않는다).
pub fn browser_command(os: Os, url: &str) -> CommandLine {
    let (program, mut args): (&str, Vec<String>) = match os {
        Os::MacOs => ("open", Vec::new()),
        Os::Windows => ("cmd", vec!["/c".into(), "start".into(), String::new()]),
        Os::Linux => ("xdg-open", Vec::new()),
    };
    args.push(url.to_string());
    CommandLine {
        program: program.to_string(),
        args,
    }
}

/// 마법사가 띄운 서버를 끄는 명령 줄. 유닉스는 `kill -TERM`(서버가 받아 깨끗이 끈다), 윈도는 신호가 없어
/// `taskkill /T /F`(나무째, 강제 — 저장소는 쓰기 도중 끊겨도 깨지지 않는다).
pub fn terminate_command(os: Os, pid: u32) -> CommandLine {
    let pid = pid.to_string();
    let (program, args): (&str, Vec<String>) = match os {
        Os::Windows => (
            "taskkill",
            vec!["/PID".into(), pid, "/T".into(), "/F".into()],
        ),
        Os::MacOs | Os::Linux => ("kill", vec!["-TERM".into(), pid]),
    };
    CommandLine {
        program: program.to_string(),
        args,
    }
}

/// 실행 파일 이름.
pub fn executable_name(os: Os) -> &'static str {
    match os {
        Os::Windows => "price-inha.exe",
        Os::MacOs | Os::Linux => "price-inha",
    }
}

/// 윈도에서 CLI(node 등)가 도는 데 더 필요한 환경 이름(사용자 폴더 · 시스템 폴더 · 임시 폴더). 비밀이 아니다.
const WINDOWS_CLI_ENV: [&str; 13] = [
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "HOMEDRIVE",
    "HOMEPATH",
    "USERNAME",
    "SystemRoot",
    "SystemDrive",
    "windir",
    "ComSpec",
    "PATHEXT",
    "TEMP",
    "TMP",
];

/// CLI 자식에 넘기는 환경 이름들(비밀이 아닌 것만). 맥 · 리눅스는 [`crate::CLI_ENV_ALLOWLIST`] 그대로,
/// 윈도는 거기에 사용자 폴더 · 시스템 폴더 이름을 더한다.
pub fn cli_env_names(os: Os) -> Vec<&'static str> {
    let mut names = crate::CLI_ENV_ALLOWLIST.to_vec();
    if os == Os::Windows {
        names.extend(WINDOWS_CLI_ENV);
    }
    names
}

/// 보관 폴더 권한 확인의 표시 글(윈도에는 0700 · 0600 비트가 없다 — 검사하지 않는다고 분명히 적는다).
pub fn home_permission_note(os: Os) -> &'static str {
    match os {
        Os::Windows => WINDOWS_PERMISSION_NOTE,
        Os::MacOs | Os::Linux => "권한 좁음",
    }
}

/// 보관 폴더가 다른 사용자도 읽을 수 있는 곳이면 경고 글. 윈도만 본다: 사용자 프로필(`\Users\<이름>\`) 밖이거나
/// 공용(`Public`) · 기본(`Default`) 프로필, `ProgramData` 아래면 경고한다(드라이브 바로 아래 폴더는 기본 권한이
/// 다른 사용자에게도 열려 있다). 유닉스는 권한 비트로 따로 본다(`None`).
pub fn shared_home_warning(os: Os, home: &str) -> Option<String> {
    if os != Os::Windows {
        return None;
    }
    let lower = home.to_lowercase().replace('/', "\\");
    let parts: Vec<&str> = lower.split('\\').filter(|p| !p.is_empty()).collect();
    let user = parts
        .iter()
        .position(|p| *p == "users")
        .and_then(|i| parts.get(i + 1));
    let private = user.is_some_and(|name| !matches!(*name, "public" | "default" | "all users"));
    let program_data = parts.get(1) == Some(&"programdata");
    (!private || program_data).then(|| {
        format!(
            "보관 폴더가 다른 사용자도 읽을 수 있는 곳일 수 있습니다: {home} — \
사용자 폴더(%USERPROFILE%) 아래에 두기를 권합니다(기본: %USERPROFILE%\\.price-inha)"
        )
    })
}

/// 기본 보관 폴더(사용자 홈 아래 `.price-inha` — 윈도는 `%USERPROFILE%` 아래)의 글자.
pub fn default_home_text(os: Os, user_home: &str) -> String {
    let (_, slash) = separators(os);
    format!(
        "{}{slash}{}",
        user_home.trim_end_matches(['/', '\\']),
        pi_secrets::DEFAULT_DIR_NAME
    )
}

/// doctor의 "보관 폴더가 괜찮다" 글(권한을 검사했는지 운영체제대로 적는다).
pub(crate) fn home_ok_line(home: &std::path::Path) -> String {
    format!(
        "보관 폴더 {} ({})",
        home.display(),
        home_permission_note(this_os())
    )
}

/// doctor의 윈도 보관 폴더 문제(무엇, 고치기): 공용 폴더에 두었으면.
#[cfg(not(unix))]
pub(crate) fn shared_home_problem(home: &std::path::Path) -> Option<(String, String)> {
    shared_home_warning(this_os(), &home.display().to_string()).map(|what| {
        let fix = "--home 으로 %USERPROFILE% 아래 폴더를 준다".to_string();
        (what, fix)
    })
}

/// 마법사 0단계의 윈도 보관 폴더 글(권한은 검사하지 않음, 공용 폴더면 경고를 다음 줄에).
#[cfg(not(unix))]
pub(crate) fn windows_home_line(home: &std::path::Path) -> String {
    let mut line = format!("보관 폴더: {} ({WINDOWS_PERMISSION_NOTE})", home.display());
    if let Some(warning) = shared_home_warning(Os::Windows, &home.display().to_string()) {
        line.push('\n');
        line.push_str(&warning);
    }
    line
}

/// 본인만 여는 폴더를 만든다(유닉스 0700). 윈도에는 그 비트가 없어 그냥 만든다 — 보관 폴더를 사용자 프로필
/// 아래에 두는 것으로 대신한다([`shared_home_warning`]).
pub(crate) fn create_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::DirBuilder::new().create(path)
    }
}

/// 이미 있는 폴더의 권한을 본인만으로 좁힌다(유닉스 0700). 윈도에서는 하지 않는다([`WINDOWS_PERMISSION_NOTE`]).
pub(crate) fn narrow_private_dir(
    path: &std::path::Path,
    meta: &std::fs::Metadata,
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o777 != 0o700 {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    #[cfg(not(unix))]
    let _ = (path, meta);
    Ok(())
}

/// 실행할 수 있는 보통 파일인가(열지 않는다). 유닉스는 실행 비트, 윈도는 확장자(`.exe` · `.cmd` · `.bat`).
pub(crate) fn is_runnable_file(path: &std::path::Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "exe" | "cmd" | "bat"))
    }
}

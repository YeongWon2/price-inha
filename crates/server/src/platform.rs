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
    todo!()
}

/// `PATH`에서 찾는 CLI 파일 이름(이 순서).
pub fn claude_file_names(_os: Os) -> &'static [&'static str] {
    todo!()
}

/// `PATH` 값에서 CLI 후보 경로들(절대 경로 항목만, 이 순서). 운영체제의 나눔 글자 · 경로 글자를 쓴다.
pub fn claude_path_candidates(_os: Os, _path_var: &str) -> Vec<String> {
    todo!()
}

/// `PATH`에 없을 때 사용자 홈 아래에서 찾는 자리(홈에 이어 붙인다).
pub fn claude_user_location(_os: Os) -> &'static str {
    todo!()
}

/// 주소를 기본 브라우저로 여는 명령 줄.
pub fn browser_command(_os: Os, _url: &str) -> CommandLine {
    todo!()
}

/// 마법사가 띄운 서버를 끄는 명령 줄.
pub fn terminate_command(_os: Os, _pid: u32) -> CommandLine {
    todo!()
}

/// 실행 파일 이름.
pub fn executable_name(_os: Os) -> &'static str {
    todo!()
}

/// CLI 자식에 넘기는 환경 이름들(비밀이 아닌 것만).
pub fn cli_env_names(_os: Os) -> Vec<&'static str> {
    todo!()
}

/// 보관 폴더 권한 확인의 표시 글.
pub fn home_permission_note(_os: Os) -> &'static str {
    todo!()
}

/// 보관 폴더가 다른 사용자도 읽을 수 있는 곳(윈도의 공용 폴더 등)이면 경고 글.
pub fn shared_home_warning(_os: Os, _home: &str) -> Option<String> {
    todo!()
}

/// 기본 보관 폴더(사용자 홈 아래 `.price-inha`)의 글자.
pub fn default_home_text(_os: Os, _user_home: &str) -> String {
    todo!()
}

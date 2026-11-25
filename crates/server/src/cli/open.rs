//! (37차) `price-inha open [화면] [--print]`: 도는 서버의 대시보드를 브라우저로 연다.
//!
//! 지시서 `docs/dev/modules/server.md` "37차" 8번, 결정 24. 원본 동작 근거: `docs/research/onboarding-messenger.md`
//! 2.1(OpenClaw Quick start 7단계 "브라우저 대시보드를 연다").
//!
//! # 계약
//!
//! - 도는 서버를 `access-key`와 같은 차례로 찾는다(설정의 포트의 루프백 → 이 홈을 가리키는 서비스 정의의 포트 →
//!   설정의 여는 주소). 찾은 서버의 주소는 `http://127.0.0.1:<포트><화면 경로>`이다. 주소에 비밀값(접속 키 ·
//!   세션 · 토큰)을 싣지 않는다.
//! - `--print`면 브라우저를 열지 않고 주소 한 줄만 표준 출력에 쓰고 0으로 끝난다(시험 · 원격 터미널용).
//!   아니면 주소를 보이고 운영체제의 여는 명령(`open` · `xdg-open` · `cmd /c start`)을 [`CommandRunner`]로 부른다.
//! - 도는 서버가 없으면 까닭과 [`OPEN_NO_SERVER`](`price-inha run` 안내)를 말하고 [`OPEN_NO_SERVER_EXIT`](1)로
//!   끝난다. 보관 폴더를 만들지 않는다.
//! - 모르는 화면 이름은 명령 읽기에서 거부한다(설정 오류와 같은 종료 코드 2).

use std::io::Write;

use super::access::{Located, locate, places_for};
use super::runner::{CommandLine, CommandRunner};
use super::service::{Service, ServiceSetup};
use crate::Settings;
use crate::page::{ACCOUNT_PAGE_PATH, HISTORY_PAGE_PATH, RULES_PAGE_PATH, TEMPLATES_PAGE_PATH};

/// 열 화면.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenScreen {
    /// 첫 화면 `/`(기본).
    #[default]
    Home,
    /// 사무실 `/office/`.
    Office,
    /// 계좌 `/account/`.
    Account,
    /// 템플릿 `/templates/`.
    Templates,
    /// 규칙 `/rules/`.
    Rules,
    /// 히스토리 `/history/`.
    History,
}

/// 열 수 있는 화면들(명령 줄 이름 차례).
pub const OPEN_SCREENS: [OpenScreen; 6] = [
    OpenScreen::Home,
    OpenScreen::Office,
    OpenScreen::Account,
    OpenScreen::Templates,
    OpenScreen::Rules,
    OpenScreen::History,
];

impl OpenScreen {
    /// 명령 줄 이름(`home`은 인자를 주지 않은 것과 같다).
    pub const fn as_str(self) -> &'static str {
        match self {
            OpenScreen::Home => "home",
            OpenScreen::Office => "office",
            OpenScreen::Account => "account",
            OpenScreen::Templates => "templates",
            OpenScreen::Rules => "rules",
            OpenScreen::History => "history",
        }
    }

    /// 서버 안의 경로.
    pub const fn path(self) -> &'static str {
        match self {
            OpenScreen::Home => "/",
            OpenScreen::Office => "/office/",
            OpenScreen::Account => ACCOUNT_PAGE_PATH,
            OpenScreen::Templates => TEMPLATES_PAGE_PATH,
            OpenScreen::Rules => RULES_PAGE_PATH,
            OpenScreen::History => HISTORY_PAGE_PATH,
        }
    }

    /// 이름 → 화면. `/`도 첫 화면이다. 모르는 이름이면 `None`.
    pub fn parse(name: &str) -> Option<OpenScreen> {
        if name == "/" {
            return Some(OpenScreen::Home);
        }
        OPEN_SCREENS
            .into_iter()
            .find(|screen| screen.as_str() == name)
    }
}

/// 도는 서버가 없을 때의 종료 코드.
pub const OPEN_NO_SERVER_EXIT: u8 = 1;
/// 도는 서버가 없을 때의 안내.
pub const OPEN_NO_SERVER: &str = "도는 서버를 찾지 못했습니다. `price-inha run`으로 먼저 켜세요";

/// `open`의 선택.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OpenRequest {
    /// 열 화면.
    pub screen: OpenScreen,
    /// 브라우저를 열지 않고 주소만 쓴다.
    pub print: bool,
}

/// 대시보드를 연다(모듈 문서의 계약). 종료 코드를 돌려준다.
pub async fn open(
    settings: &Settings,
    request: &OpenRequest,
    output: &mut dyn Write,
    runner: &dyn CommandRunner,
) -> u8 {
    let service = ServiceSetup::for_this_computer(&settings.home, Vec::new())
        .ok()
        .map(|setup| Service::new(setup, runner));
    let port = match locate(places_for(settings, service.as_ref())).await {
        Located::Ours(_, port) => port,
        Located::NotOurs(port) => {
            let _ = writeln!(
                output,
                "127.0.0.1:{port}에서 이 서버가 아닌 것이 답합니다.\n{OPEN_NO_SERVER}"
            );
            return OPEN_NO_SERVER_EXIT;
        }
        Located::Nowhere => {
            let _ = writeln!(
                output,
                "127.0.0.1:{}에서 답하는 서버가 없습니다.\n{OPEN_NO_SERVER}",
                settings.port
            );
            return OPEN_NO_SERVER_EXIT;
        }
    };
    let url = screen_url(port, request.screen);
    let _ = writeln!(output, "{url}");
    if request.print {
        return 0;
    }
    if !open_browser(runner, &url) {
        let _ = writeln!(output, "브라우저를 열지 못했습니다. 위 주소를 직접 여세요");
    }
    0
}

/// 화면 주소 `http://127.0.0.1:<포트><화면 경로>`. 비밀값(접속 키 · 세션 · 토큰)을 싣지 않는다.
pub(crate) fn screen_url(port: u16, screen: OpenScreen) -> String {
    format!("http://127.0.0.1:{port}{}", screen.path())
}

/// 운영체제의 여는 명령으로 주소를 연다(macOS `open`, 윈도 `cmd /c start`, 그 밖은 `xdg-open`). 열었는가.
pub(crate) fn open_browser(runner: &dyn CommandRunner, url: &str) -> bool {
    runner
        .run(&browser_command(url))
        .is_ok_and(|output| output.success)
}

fn browser_command(url: &str) -> CommandLine {
    let (program, mut args): (&str, Vec<String>) = if cfg!(target_os = "macos") {
        ("open", Vec::new())
    } else if cfg!(windows) {
        (
            "cmd",
            vec!["/c".to_string(), "start".to_string(), String::new()],
        )
    } else {
        ("xdg-open", Vec::new())
    };
    args.push(url.to_string());
    CommandLine {
        program: program.to_string(),
        args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 주소에는_화면_경로만_있다() {
        assert_eq!(screen_url(7, OpenScreen::Home), "http://127.0.0.1:7/");
        assert_eq!(
            screen_url(7, OpenScreen::Office),
            "http://127.0.0.1:7/office/"
        );
        let command = browser_command("http://127.0.0.1:7/");
        assert_eq!(
            command.args.last().map(String::as_str),
            Some("http://127.0.0.1:7/")
        );
    }
}

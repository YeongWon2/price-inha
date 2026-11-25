//! 하위 명령 읽기: `setup [구역] [--stdin-answers] [--dry-run] [설정 인자들]` · `open [화면] [--print] [설정 인자들]`.

use price_inha::{
    ACCOUNT_PAGE_PATH, CliError, Command, HISTORY_PAGE_PATH, OPEN_SCREENS, OpenScreen,
    RULES_PAGE_PATH, SETUP_SECTIONS, SetupSection, TEMPLATES_PAGE_PATH,
};

use crate::run::strings;

// 검증: D1 D4
#[test]
fn setup은_구역과_숨은_길을_읽는다() {
    assert_eq!(
        Command::parse(&strings(&["setup"])).unwrap(),
        Command::Setup {
            section: None,
            stdin_answers: false,
            dry_run: false,
            args: Vec::new(),
        },
        "구역을 주지 않으면 모두 차례로"
    );
    assert_eq!(
        Command::parse(&strings(&[
            "setup",
            "ai",
            "--stdin-answers",
            "--home",
            "/x"
        ]))
        .unwrap(),
        Command::Setup {
            section: Some(SetupSection::Ai),
            stdin_answers: true,
            dry_run: false,
            args: strings(&["--home", "/x"]),
        }
    );
    assert_eq!(
        Command::parse(&strings(&["setup", "--dry-run", "--port", "9"])).unwrap(),
        Command::Setup {
            section: None,
            stdin_answers: false,
            dry_run: true,
            args: strings(&["--port", "9"]),
        },
        "플래그만 주면 구역은 없다"
    );
    let names: Vec<&str> = SETUP_SECTIONS.iter().map(|s| s.as_str()).collect();
    assert_eq!(names, ["ai", "broker", "symbols", "service", "open"]);
    for section in SETUP_SECTIONS {
        assert_eq!(SetupSection::parse(section.as_str()), Some(section));
    }
}

// 검증: D1
#[test]
fn setup의_모르는_구역은_거부한다() {
    let unknown = Command::parse(&strings(&["setup", "rockets"]));
    assert!(
        matches!(unknown, Err(CliError::UnknownCommand(_))),
        "{unknown:?}"
    );
}

// 검증: D1
#[test]
fn open은_화면과_print를_읽는다() {
    assert_eq!(
        Command::parse(&strings(&["open"])).unwrap(),
        Command::Open {
            screen: OpenScreen::Home,
            print: false,
            args: Vec::new(),
        }
    );
    assert_eq!(
        Command::parse(&strings(&["open", "office", "--print", "--port", "9"])).unwrap(),
        Command::Open {
            screen: OpenScreen::Office,
            print: true,
            args: strings(&["--port", "9"]),
        }
    );
    assert_eq!(OpenScreen::parse("/"), Some(OpenScreen::Home));
    let paths: Vec<&str> = OPEN_SCREENS.iter().map(|s| s.path()).collect();
    assert_eq!(
        paths,
        [
            "/",
            "/office/",
            ACCOUNT_PAGE_PATH,
            TEMPLATES_PAGE_PATH,
            RULES_PAGE_PATH,
            HISTORY_PAGE_PATH
        ]
    );
    for screen in OPEN_SCREENS {
        assert_eq!(OpenScreen::parse(screen.as_str()), Some(screen));
    }
}

// 검증: D1
#[test]
fn open의_모르는_화면은_거부한다() {
    let unknown = Command::parse(&strings(&["open", "nowhere"]));
    assert!(
        matches!(unknown, Err(CliError::UnknownCommand(_))),
        "{unknown:?}"
    );
}

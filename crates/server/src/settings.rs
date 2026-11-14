//! 설정: 환경 변수와 인자에서 읽는다. 비밀 값은 여기로 오지 않는다.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pi_market::{Market, Symbol};

use crate::SettingsError;

/// 보관 폴더 환경 변수. `pi-secrets`의 `HOME_ENV`와 같은 이름이다.
pub const ENV_HOME: &str = "PRICE_INHA_HOME";
/// 여는 주소 환경 변수.
pub const ENV_BIND: &str = "PRICE_INHA_BIND";
/// 여는 포트 환경 변수.
pub const ENV_PORT: &str = "PRICE_INHA_PORT";
/// 지켜볼 종목 환경 변수(쉼표로 나눈다).
pub const ENV_SYMBOLS: &str = "PRICE_INHA_SYMBOLS";
/// 시장 환경 변수(`kr` 또는 `us`).
pub const ENV_MARKET: &str = "PRICE_INHA_MARKET";
/// 시세 간격 환경 변수(밀리초).
pub const ENV_QUOTE_INTERVAL_MS: &str = "PRICE_INHA_QUOTE_INTERVAL_MS";
/// 시세 간격의 기본값.
pub const DEFAULT_QUOTE_INTERVAL: Duration = Duration::from_secs(1);

/// 실행 설정.
///
/// | 값 | 인자 | 환경 변수 | 기본값 |
/// |---|---|---|---|
/// | 보관 폴더 | `--home <폴더>` | [`ENV_HOME`] | `pi_secrets::resolve_dir(없음, 사용자 홈)` |
/// | 여는 주소 | `--bind <IP>` | [`ENV_BIND`] | `127.0.0.1` (D2) |
/// | 여는 포트 | `--port <수>` | [`ENV_PORT`] | `pi_gate::DEFAULT_PORT` |
/// | 지켜볼 종목 | `--symbols <a,b,c>` | [`ENV_SYMBOLS`] | 없음 |
/// | 시장 | `--market <kr\|us>` | [`ENV_MARKET`] | `kr` |
/// | 시세 간격 | `--interval-ms <수>` | [`ENV_QUOTE_INTERVAL_MS`] | [`DEFAULT_QUOTE_INTERVAL`] |
/// | 시연 | `--demo` | 없음 | 끔 |
///
/// 인자가 환경 변수를 이긴다. 비밀 값(키 · 토큰)을 받는 인자와 환경 변수는 없다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// 보관 폴더. 키 파일(`pi-secrets`)과 저장소 파일([`crate::DB_FILE`])이 여기에 있다.
    pub home: PathBuf,
    /// 여는 주소. 분명히 주지 않으면 `127.0.0.1`이다.
    pub bind: IpAddr,
    /// 여는 포트. 0이면 빈 포트를 고른다.
    pub port: u16,
    /// 지켜볼 종목. 준 순서 그대로이고 빈 칸은 뺀다.
    pub symbols: Vec<Symbol>,
    /// 지켜볼 종목의 시장.
    pub market: Market,
    /// 시세를 받는 간격. 0은 받지 않는다([`SettingsError::InvalidValue`]).
    pub quote_interval: Duration,
    /// 시연 모드: 가짜 어댑터로 시세를 흘린다.
    pub demo: bool,
}

impl Settings {
    /// 인자와 환경에서 설정을 읽는다. 프로세스 환경을 직접 읽지 않고 받은 것으로만 정한다.
    ///
    /// - `args`: 프로그램 이름을 뺀 인자들. `--이름 값` 모양만 받는다.
    /// - `env`: 환경 변수 이름을 주면 값을 돌려주는 함수.
    /// - `user_home`: 사용자 홈 폴더(보관 폴더의 기본값을 정할 때만 쓴다).
    ///
    /// 모르는 인자는 [`SettingsError::UnknownArgument`]다(조용히 넘기지 않는다).
    pub fn parse(
        args: &[String],
        env: &dyn Fn(&str) -> Option<String>,
        user_home: Option<&Path>,
    ) -> Result<Settings, SettingsError> {
        todo!()
    }

    /// 프로세스의 인자 · 환경 변수 · 홈 폴더로 [`Settings::parse`]를 부른다.
    pub fn from_process() -> Result<Settings, SettingsError> {
        todo!()
    }
}

//! 설정: 환경 변수와 인자에서 읽는다. 비밀 값은 여기로 오지 않는다.

use std::ffi::OsStr;
use std::net::{IpAddr, Ipv4Addr};
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

/// 다시 시도하는 첫 간격 환경 변수(밀리초).
pub const ENV_RETRY_INITIAL_MS: &str = "PRICE_INHA_RETRY_INITIAL_MS";
/// 다시 시도하는 간격의 상한 환경 변수(밀리초).
pub const ENV_RETRY_MAX_MS: &str = "PRICE_INHA_RETRY_MAX_MS";
/// 다시 시도하는 첫 간격의 기본값.
pub const DEFAULT_RETRY_INITIAL: Duration = Duration::from_secs(1);
/// 다시 시도하는 간격 상한의 기본값.
pub const DEFAULT_RETRY_MAX: Duration = Duration::from_secs(60);

/// 달력을 받지 못했을 때 다시 받기까지의 간격 환경 변수(밀리초).
pub const ENV_CALENDAR_RETRY_MS: &str = "PRICE_INHA_CALENDAR_RETRY_MS";
/// 달력을 다시 받는 간격의 기본값.
pub const DEFAULT_CALENDAR_RETRY: Duration = Duration::from_secs(30);
/// 키 등록 시도 횟수 환경 변수.
pub const ENV_REGISTER_ATTEMPTS: &str = "PRICE_INHA_REGISTER_ATTEMPTS";
/// 키 등록 시도를 세는 시간 환경 변수(밀리초).
pub const ENV_REGISTER_WINDOW_MS: &str = "PRICE_INHA_REGISTER_WINDOW_MS";
/// 키 등록 시도 횟수의 기본값([`DEFAULT_REGISTER_WINDOW`] 동안).
pub const DEFAULT_REGISTER_ATTEMPTS: u32 = 5;
/// 키 등록 시도를 세는 시간의 기본값.
pub const DEFAULT_REGISTER_WINDOW: Duration = Duration::from_secs(60);
/// 종목 등록 시도 횟수 환경 변수. (4차)
pub const ENV_SYMBOLS_ATTEMPTS: &str = "PRICE_INHA_SYMBOLS_ATTEMPTS";
/// 종목 등록 시도를 세는 시간 환경 변수(밀리초). (4차)
pub const ENV_SYMBOLS_WINDOW_MS: &str = "PRICE_INHA_SYMBOLS_WINDOW_MS";
/// 종목 등록 시도 횟수의 기본값([`DEFAULT_SYMBOLS_WINDOW`] 동안). (4차)
pub const DEFAULT_SYMBOLS_ATTEMPTS: u32 = 10;
/// 종목 등록 시도를 세는 시간의 기본값. (4차)
pub const DEFAULT_SYMBOLS_WINDOW: Duration = Duration::from_secs(60);

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
/// | 다시 시도 첫 간격 | `--retry-initial-ms <수>` | [`ENV_RETRY_INITIAL_MS`] | [`DEFAULT_RETRY_INITIAL`] |
/// | 다시 시도 간격 상한 | `--retry-max-ms <수>` | [`ENV_RETRY_MAX_MS`] | [`DEFAULT_RETRY_MAX`] |
/// | 달력 다시 받는 간격 | `--calendar-retry-ms <수>` | [`ENV_CALENDAR_RETRY_MS`] | [`DEFAULT_CALENDAR_RETRY`] |
/// | 키 등록 시도 횟수 | `--register-attempts <수>` | [`ENV_REGISTER_ATTEMPTS`] | [`DEFAULT_REGISTER_ATTEMPTS`] |
/// | 키 등록 시도를 세는 시간 | `--register-window-ms <수>` | [`ENV_REGISTER_WINDOW_MS`] | [`DEFAULT_REGISTER_WINDOW`] |
/// | 종목 등록 시도 횟수 (4차) | `--symbols-attempts <수>` | [`ENV_SYMBOLS_ATTEMPTS`] | [`DEFAULT_SYMBOLS_ATTEMPTS`] |
/// | 종목 등록 시도를 세는 시간 (4차) | `--symbols-window-ms <수>` | [`ENV_SYMBOLS_WINDOW_MS`] | [`DEFAULT_SYMBOLS_WINDOW`] |
/// | 시연 | `--demo` | 없음 | 끔 |
///
/// 인자가 환경 변수를 이긴다. 비밀 값(키 · 토큰)을 받는 인자와 환경 변수는 없다.
///
/// (6차) CLI에 넘길 환경([`Settings::cli_env`])은 받은 환경에서 `crate::CLI_ENV_ALLOWLIST`의 이름만 고른 것이고,
/// [`Settings::user_home`]은 받은 사용자 홈 폴더다(CLI 실행 파일을 찾을 때 쓴다).
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
    /// 증권사 요청이 실패한 뒤 다시 시도하기까지의 첫 간격. 실패가 이어지면 두 배씩 늘린다.
    /// 0은 받지 않는다([`SettingsError::InvalidValue`]).
    pub retry_initial: Duration,
    /// 다시 시도하는 간격의 상한. 이보다 길게 기다리지 않는다.
    /// 0은 받지 않는다([`SettingsError::InvalidValue`]).
    pub retry_max: Duration,
    /// 달력을 받지 못했을 때(인증 실패가 아닌 실패) 다시 받기까지의 간격.
    /// 0은 받지 않는다([`SettingsError::InvalidValue`]).
    pub calendar_retry: Duration,
    /// `register_window` 동안 받는 키 등록 시도 횟수. 넘으면 확인기를 부르지 않고 거부한다.
    /// 0은 받지 않는다([`SettingsError::InvalidValue`]).
    pub register_attempts: u32,
    /// 키 등록 시도를 세는 시간. 0은 받지 않는다([`SettingsError::InvalidValue`]).
    pub register_window: Duration,
    /// `symbols_window` 동안 받는 종목 등록 시도 횟수. 넘으면 저장하지 않고 거부한다(429
    /// `too_many_attempts`). 입력 검사를 지난 시도만 센다. 키 등록과 따로 센다.
    /// 0은 받지 않는다([`SettingsError::InvalidValue`]). (4차)
    pub symbols_attempts: u32,
    /// 종목 등록 시도를 세는 시간. 0은 받지 않는다([`SettingsError::InvalidValue`]). (4차)
    pub symbols_window: Duration,
    /// 시연 모드: 가짜 어댑터로 시세를 흘린다. 필수 설정 없이 바로 시작한다.
    pub demo: bool,
    /// (6차) 사용자 홈 폴더. CLI 실행 파일을 `~/.local/bin/claude`에서 찾을 때 쓴다.
    pub user_home: Option<PathBuf>,
    /// (6차) CLI 자식에게 줄 환경: 받은 환경에서 `crate::CLI_ENV_ALLOWLIST`의 이름만 그 순서로 고른 것.
    /// 서버의 다른 환경 변수(비밀이 있을 수 있는 것)는 들어가지 않는다.
    pub cli_env: Vec<(String, String)>,
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
        let given = Given::from_args(args)?;
        let pick = |slot: &Option<String>, arg: &'static str, var: &'static str| {
            slot.clone()
                .map(|value| (value, arg))
                .or_else(|| env(var).map(|value| (value, var)))
        };
        let (home, bind, port) = place(
            &pick(&given.home, ARG_HOME, ENV_HOME),
            &pick(&given.bind, ARG_BIND, ENV_BIND),
            &pick(&given.port, ARG_PORT, ENV_PORT),
            user_home,
        )?;
        let symbols = match pick(&given.symbols, ARG_SYMBOLS, ENV_SYMBOLS) {
            Some((value, name)) => symbols(&value, name)?,
            None => Vec::new(),
        };
        let market = match pick(&given.market, ARG_MARKET, ENV_MARKET) {
            Some((value, name)) => market(&value, name)?,
            None => Market::Kr,
        };
        let quote_interval = millis(
            pick(&given.interval_ms, ARG_INTERVAL_MS, ENV_QUOTE_INTERVAL_MS),
            DEFAULT_QUOTE_INTERVAL,
        )?;
        let retry_initial = millis(
            pick(
                &given.retry_initial_ms,
                ARG_RETRY_INITIAL_MS,
                ENV_RETRY_INITIAL_MS,
            ),
            DEFAULT_RETRY_INITIAL,
        )?;
        let retry_max = millis(
            pick(&given.retry_max_ms, ARG_RETRY_MAX_MS, ENV_RETRY_MAX_MS),
            DEFAULT_RETRY_MAX,
        )?;
        let calendar_retry = millis(
            pick(
                &given.calendar_retry_ms,
                ARG_CALENDAR_RETRY_MS,
                ENV_CALENDAR_RETRY_MS,
            ),
            DEFAULT_CALENDAR_RETRY,
        )?;
        let limits = attempt_limits(&given, &pick)?;
        Ok(Settings {
            home,
            bind,
            port,
            symbols,
            market,
            quote_interval,
            retry_initial,
            retry_max,
            calendar_retry,
            register_attempts: limits.register_attempts,
            register_window: limits.register_window,
            symbols_attempts: limits.symbols_attempts,
            symbols_window: limits.symbols_window,
            demo: given.demo,
            user_home: user_home.map(Path::to_path_buf),
            cli_env: crate::cli_base_env(env),
        })
    }

    /// 프로세스의 인자 · 환경 변수 · 홈 폴더로 [`Settings::parse`]를 부른다.
    pub fn from_process() -> Result<Settings, SettingsError> {
        // 글자로 읽을 수 없는 인자 · 환경 변수는 말없이 버리지 않고 거부한다.
        let args = std::env::args_os()
            .skip(1)
            .map(|arg| {
                arg.into_string()
                    .map_err(|_| SettingsError::UnknownArgument)
            })
            .collect::<Result<Vec<String>, SettingsError>>()?;
        Settings::from_process_env(&args)
    }

    /// (9차) 받은 인자(하위 명령 이름을 뺀 것)와 프로세스의 환경 변수 · 홈 폴더로 [`Settings::parse`]를 부른다.
    pub fn from_process_env(args: &[String]) -> Result<Settings, SettingsError> {
        for name in ENV_NAMES {
            if std::env::var_os(name).is_some_and(|value| value.to_str().is_none()) {
                return Err(SettingsError::InvalidValue { name });
            }
        }
        let env = |name: &str| std::env::var(name).ok();
        Settings::parse(args, &env, std::env::home_dir().as_deref())
    }
}

/// 인자나 환경 변수에서 고른 값과 그것을 준 이름.
type Picked = Option<(String, &'static str)>;

/// 보관 폴더 · 여는 주소 · 여는 포트.
fn place(
    home: &Picked,
    bind: &Picked,
    port: &Picked,
    user_home: Option<&Path>,
) -> Result<(PathBuf, IpAddr, u16), SettingsError> {
    let home = match home {
        // 준 값이 빈 글자 · 상대 경로면 말없이 기본 폴더로 바꾸지 않고 거부한다.
        Some((value, name)) => pi_secrets::resolve_dir(Some(OsStr::new(value)), None)
            .map_err(|_| SettingsError::InvalidValue { name })?,
        None => {
            pi_secrets::resolve_dir(None, user_home).map_err(|_| SettingsError::HomeNotFound)?
        }
    };
    let bind = match bind {
        Some((value, name)) => parsed::<IpAddr>(value, name)?,
        None => IpAddr::V4(Ipv4Addr::LOCALHOST),
    };
    let port = match port {
        Some((value, name)) => parsed::<u16>(value, name)?,
        None => pi_gate::DEFAULT_PORT,
    };
    Ok((home, bind, port))
}

/// 간격: 밀리초로 받고 0은 받지 않는다. 주지 않았으면 기본값이다.
fn millis(picked: Picked, default: Duration) -> Result<Duration, SettingsError> {
    match picked {
        Some((value, name)) => match parsed::<u64>(&value, name)? {
            0 => Err(SettingsError::InvalidValue { name }),
            millis => Ok(Duration::from_millis(millis)),
        },
        None => Ok(default),
    }
}

/// 시도 횟수: 0은 받지 않는다. 주지 않았으면 기본값이다.
fn attempts(picked: Picked, default: u32) -> Result<u32, SettingsError> {
    match picked {
        Some((value, name)) => match parsed::<u32>(&value, name)? {
            0 => Err(SettingsError::InvalidValue { name }),
            attempts => Ok(attempts),
        },
        None => Ok(default),
    }
}

/// 등록 시도 제한들(키 등록과 종목 등록은 따로 센다).
struct Limits {
    register_attempts: u32,
    register_window: Duration,
    symbols_attempts: u32,
    symbols_window: Duration,
}

/// 인자 · 환경 변수에서 고르는 함수: (인자로 준 값, 인자 이름, 환경 변수 이름).
type Pick<'a> = dyn Fn(&Option<String>, &'static str, &'static str) -> Picked + 'a;

fn attempt_limits(given: &Given, pick: &Pick<'_>) -> Result<Limits, SettingsError> {
    Ok(Limits {
        register_window: millis(
            pick(
                &given.register_window_ms,
                ARG_REGISTER_WINDOW_MS,
                ENV_REGISTER_WINDOW_MS,
            ),
            DEFAULT_REGISTER_WINDOW,
        )?,
        register_attempts: attempts(
            pick(
                &given.register_attempts,
                ARG_REGISTER_ATTEMPTS,
                ENV_REGISTER_ATTEMPTS,
            ),
            DEFAULT_REGISTER_ATTEMPTS,
        )?,
        symbols_window: millis(
            pick(
                &given.symbols_window_ms,
                ARG_SYMBOLS_WINDOW_MS,
                ENV_SYMBOLS_WINDOW_MS,
            ),
            DEFAULT_SYMBOLS_WINDOW,
        )?,
        symbols_attempts: attempts(
            pick(
                &given.symbols_attempts,
                ARG_SYMBOLS_ATTEMPTS,
                ENV_SYMBOLS_ATTEMPTS,
            ),
            DEFAULT_SYMBOLS_ATTEMPTS,
        )?,
    })
}

const ARG_HOME: &str = "--home";
const ARG_BIND: &str = "--bind";
const ARG_PORT: &str = "--port";
const ARG_SYMBOLS: &str = "--symbols";
const ARG_MARKET: &str = "--market";
const ARG_INTERVAL_MS: &str = "--interval-ms";
const ARG_RETRY_INITIAL_MS: &str = "--retry-initial-ms";
const ARG_RETRY_MAX_MS: &str = "--retry-max-ms";
const ARG_CALENDAR_RETRY_MS: &str = "--calendar-retry-ms";
const ARG_REGISTER_ATTEMPTS: &str = "--register-attempts";
const ARG_REGISTER_WINDOW_MS: &str = "--register-window-ms";
const ARG_SYMBOLS_ATTEMPTS: &str = "--symbols-attempts";
const ARG_SYMBOLS_WINDOW_MS: &str = "--symbols-window-ms";
const ARG_DEMO: &str = "--demo";

/// 읽는 환경 변수 전부.
const ENV_NAMES: [&str; 13] = [
    ENV_HOME,
    ENV_BIND,
    ENV_PORT,
    ENV_SYMBOLS,
    ENV_MARKET,
    ENV_QUOTE_INTERVAL_MS,
    ENV_RETRY_INITIAL_MS,
    ENV_RETRY_MAX_MS,
    ENV_CALENDAR_RETRY_MS,
    ENV_REGISTER_ATTEMPTS,
    ENV_REGISTER_WINDOW_MS,
    ENV_SYMBOLS_ATTEMPTS,
    ENV_SYMBOLS_WINDOW_MS,
];

/// 인자로 준 것들(값은 아직 글자 그대로). 같은 인자를 두 번 주면 뒤의 것을 쓴다.
#[derive(Default)]
struct Given {
    home: Option<String>,
    bind: Option<String>,
    port: Option<String>,
    symbols: Option<String>,
    market: Option<String>,
    interval_ms: Option<String>,
    retry_initial_ms: Option<String>,
    retry_max_ms: Option<String>,
    calendar_retry_ms: Option<String>,
    register_attempts: Option<String>,
    register_window_ms: Option<String>,
    symbols_attempts: Option<String>,
    symbols_window_ms: Option<String>,
    demo: bool,
}

impl Given {
    fn from_args(args: &[String]) -> Result<Self, SettingsError> {
        let mut given = Self::default();
        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            let (slot, name) = match arg.as_str() {
                ARG_DEMO => {
                    given.demo = true;
                    continue;
                }
                ARG_HOME => (&mut given.home, ARG_HOME),
                ARG_BIND => (&mut given.bind, ARG_BIND),
                ARG_PORT => (&mut given.port, ARG_PORT),
                ARG_SYMBOLS => (&mut given.symbols, ARG_SYMBOLS),
                ARG_MARKET => (&mut given.market, ARG_MARKET),
                ARG_INTERVAL_MS => (&mut given.interval_ms, ARG_INTERVAL_MS),
                ARG_RETRY_INITIAL_MS => (&mut given.retry_initial_ms, ARG_RETRY_INITIAL_MS),
                ARG_RETRY_MAX_MS => (&mut given.retry_max_ms, ARG_RETRY_MAX_MS),
                ARG_CALENDAR_RETRY_MS => (&mut given.calendar_retry_ms, ARG_CALENDAR_RETRY_MS),
                ARG_REGISTER_ATTEMPTS => (&mut given.register_attempts, ARG_REGISTER_ATTEMPTS),
                ARG_REGISTER_WINDOW_MS => (&mut given.register_window_ms, ARG_REGISTER_WINDOW_MS),
                ARG_SYMBOLS_ATTEMPTS => (&mut given.symbols_attempts, ARG_SYMBOLS_ATTEMPTS),
                ARG_SYMBOLS_WINDOW_MS => (&mut given.symbols_window_ms, ARG_SYMBOLS_WINDOW_MS),
                // 모르는 인자의 이름도 값도 오류에 싣지 않는다.
                _ => return Err(SettingsError::UnknownArgument),
            };
            // 다음 것이 또 다른 인자면 값이 빠진 것이다.
            match rest.next() {
                Some(value) if !value.starts_with("--") => *slot = Some(value.clone()),
                _ => return Err(SettingsError::MissingValue { name }),
            }
        }
        Ok(given)
    }
}

fn parsed<T: std::str::FromStr>(value: &str, name: &'static str) -> Result<T, SettingsError> {
    value
        .trim()
        .parse()
        .map_err(|_| SettingsError::InvalidValue { name })
}

/// 쉼표로 나눈 종목들. 빈 칸은 빼고, 종목 코드로 볼 수 없는 글자가 있으면 거부한다.
fn symbols(value: &str, name: &'static str) -> Result<Vec<Symbol>, SettingsError> {
    value
        .split(',')
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(|code| symbol(code).ok_or(SettingsError::InvalidValue { name }))
        .collect()
}

/// 종목 코드 하나. 글자 규칙은 `pi_market::Symbol::parse`(market 2차)에 맡긴다(7차 4번):
/// 비었거나, 너무 길거나, 영문 · 숫자 · `.` · `-` 밖의 글자가 있으면 `None`.
/// 더해서 점이 둘 이어지면(`..`, 경로처럼 읽힐 수 있다) `None`. 점 하나(`BRK.B`)는 받는다.
/// 인자(`--symbols`) · 화면의 종목 등록 · 저장된 종목 파일이 모두 이 검사를 쓴다.
pub(crate) fn symbol(code: &str) -> Option<Symbol> {
    if code.contains("..") {
        return None;
    }
    Symbol::parse(code).ok()
}

fn market(value: &str, name: &'static str) -> Result<Market, SettingsError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "kr" => Ok(Market::Kr),
        "us" => Ok(Market::Us),
        _ => Err(SettingsError::InvalidValue { name }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn none(_name: &str) -> Option<String> {
        None
    }

    #[test]
    fn 보관_폴더는_절대_경로만_받는다() {
        for bad in ["", "relative/dir", "."] {
            assert_eq!(
                Settings::parse(&args(&["--home", bad]), &none, Some(Path::new("/home/u"))),
                Err(SettingsError::InvalidValue { name: "--home" }),
                "{bad:?}"
            );
        }
        let empty_env = |name: &str| (name == ENV_HOME).then(String::new);
        assert_eq!(
            Settings::parse(&[], &empty_env, Some(Path::new("/home/u"))),
            Err(SettingsError::InvalidValue { name: ENV_HOME }),
            "빈 환경 변수를 말없이 기본 폴더로 바꾸지 않는다"
        );
    }

    #[test]
    fn 값_자리에_다른_인자가_오면_값이_빠진_것이다() {
        assert_eq!(
            Settings::parse(&args(&["--home", "/h", "--port", "--demo"]), &none, None),
            Err(SettingsError::MissingValue { name: "--port" })
        );
    }

    #[test]
    fn 같은_인자를_두_번_주면_뒤의_것을_쓴다() {
        let got = Settings::parse(
            &args(&["--home", "/h", "--port", "1", "--port", "2"]),
            &none,
            None,
        );
        assert_eq!(got.map(|s| s.port), Ok(2));
    }

    #[test]
    fn 종목_코드로_볼_수_없는_글자는_거부한다() {
        for bad in ["005930&x=1", "AAPL MSFT", "삼성전자", "a/b"] {
            assert_eq!(
                Settings::parse(&args(&["--home", "/h", "--symbols", bad]), &none, None),
                Err(SettingsError::InvalidValue { name: "--symbols" }),
                "{bad}"
            );
        }
        let got = Settings::parse(
            &args(&["--home", "/h", "--symbols", "BRK.B, 005930"]),
            &none,
            None,
        );
        assert_eq!(
            got.map(|s| s.symbols),
            Ok(vec![Symbol::new("BRK.B"), Symbol::new("005930")])
        );
    }

    #[test]
    fn 간격과_시장은_앞뒤_공백과_대소문자를_봐준다() {
        let got = Settings::parse(
            &args(&["--home", "/h", "--market", "US", "--interval-ms", " 250 "]),
            &none,
            None,
        );
        assert_eq!(
            got.map(|s| (s.market, s.quote_interval)),
            Ok((Market::Us, Duration::from_millis(250)))
        );
    }
}

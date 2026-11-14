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
        let given = Given::from_args(args)?;
        let pick = |slot: &Option<String>, arg: &'static str, var: &'static str| {
            slot.clone()
                .map(|value| (value, arg))
                .or_else(|| env(var).map(|value| (value, var)))
        };
        let home = match pick(&given.home, ARG_HOME, ENV_HOME) {
            // 준 값이 빈 글자 · 상대 경로면 말없이 기본 폴더로 바꾸지 않고 거부한다.
            Some((value, name)) => pi_secrets::resolve_dir(Some(OsStr::new(&value)), None)
                .map_err(|_| SettingsError::InvalidValue { name })?,
            None => {
                pi_secrets::resolve_dir(None, user_home).map_err(|_| SettingsError::HomeNotFound)?
            }
        };
        let bind = match pick(&given.bind, ARG_BIND, ENV_BIND) {
            Some((value, name)) => parsed::<IpAddr>(&value, name)?,
            None => IpAddr::V4(Ipv4Addr::LOCALHOST),
        };
        let port = match pick(&given.port, ARG_PORT, ENV_PORT) {
            Some((value, name)) => parsed::<u16>(&value, name)?,
            None => pi_gate::DEFAULT_PORT,
        };
        let symbols = match pick(&given.symbols, ARG_SYMBOLS, ENV_SYMBOLS) {
            Some((value, name)) => symbols(&value, name)?,
            None => Vec::new(),
        };
        let market = match pick(&given.market, ARG_MARKET, ENV_MARKET) {
            Some((value, name)) => market(&value, name)?,
            None => Market::Kr,
        };
        let quote_interval = match pick(&given.interval_ms, ARG_INTERVAL_MS, ENV_QUOTE_INTERVAL_MS)
        {
            Some((value, name)) => match parsed::<u64>(&value, name)? {
                0 => return Err(SettingsError::InvalidValue { name }),
                millis => Duration::from_millis(millis),
            },
            None => DEFAULT_QUOTE_INTERVAL,
        };
        Ok(Settings {
            home,
            bind,
            port,
            symbols,
            market,
            quote_interval,
            demo: given.demo,
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
        for name in ENV_NAMES {
            if std::env::var_os(name).is_some_and(|value| value.to_str().is_none()) {
                return Err(SettingsError::InvalidValue { name });
            }
        }
        let env = |name: &str| std::env::var(name).ok();
        Settings::parse(&args, &env, std::env::home_dir().as_deref())
    }
}

const ARG_HOME: &str = "--home";
const ARG_BIND: &str = "--bind";
const ARG_PORT: &str = "--port";
const ARG_SYMBOLS: &str = "--symbols";
const ARG_MARKET: &str = "--market";
const ARG_INTERVAL_MS: &str = "--interval-ms";
const ARG_DEMO: &str = "--demo";

/// 읽는 환경 변수 전부.
const ENV_NAMES: [&str; 6] = [
    ENV_HOME,
    ENV_BIND,
    ENV_PORT,
    ENV_SYMBOLS,
    ENV_MARKET,
    ENV_QUOTE_INTERVAL_MS,
];

/// 종목 코드 하나의 가장 긴 길이.
const MAX_SYMBOL_LEN: usize = 32;

/// 인자로 준 것들(값은 아직 글자 그대로). 같은 인자를 두 번 주면 뒤의 것을 쓴다.
#[derive(Default)]
struct Given {
    home: Option<String>,
    bind: Option<String>,
    port: Option<String>,
    symbols: Option<String>,
    market: Option<String>,
    interval_ms: Option<String>,
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
        .map(|code| {
            let plain = code.len() <= MAX_SYMBOL_LEN
                && code
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
            if plain {
                Ok(Symbol::new(code))
            } else {
                Err(SettingsError::InvalidValue { name })
            }
        })
        .collect()
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

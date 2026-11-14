//! 설정 책상: 필수 설정의 지금 상태를 한곳에서 알고, 키 · 종목 등록을 받아 시세 흐름을 깨운다.
//!
//! 화면 쪽 경로([`crate::setup_http`])와 시세 흐름([`crate::feed`])이 함께 쓴다.
//! 받은 키 값은 `pi-secrets`로만 넘기고 어디에도 남기지 않는다(기록 · 오류 · 응답).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use pi_market::Symbol;
use pi_secrets::{KeyKind, SecretStore, SecretsError, VerifyFailure};
use tokio::sync::Notify;

use crate::keys::KeyPresence;
use crate::setup::{
    AttemptLimit, ITEM_AI, ITEM_BROKER, ITEM_DATA, ITEM_SYMBOLS, KeyRequest, SETUP_ITEMS,
    SetupItem, SetupStatus, SymbolsSource, parse_symbols, save_watchlist,
};
use crate::{KeyVerifier, SetupRefusal};

/// 키 확인 하나가 끝나기를 기다리는 가장 긴 시간. 넘으면 "닿지 않음"이고 저장하지 않는다.
/// (`pi-ai` · `pi-market`의 요청마다 시간 제한이 따로 있다. 이것은 그 바깥의 끝이다.)
const VERIFY_TIMEOUT: Duration = Duration::from_secs(30);

/// 지금 지켜보는 종목과 그 출처.
struct Watching {
    symbols: Vec<Symbol>,
    source: SymbolsSource,
}

/// 설정 책상을 만드는 데 드는 것.
pub(crate) struct DeskParts {
    pub(crate) secrets: Arc<SecretStore>,
    pub(crate) verifier: KeyVerifier,
    pub(crate) home: PathBuf,
    pub(crate) demo: bool,
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) source: SymbolsSource,
    pub(crate) limit: AttemptLimit,
    pub(crate) symbols_limit: AttemptLimit,
}

/// 설정 책상.
pub(crate) struct Desk {
    secrets: Arc<SecretStore>,
    presence: KeyPresence,
    verifier: KeyVerifier,
    home: PathBuf,
    demo: bool,
    watching: Mutex<Watching>,
    /// 키 등록 시도의 횟수 제한(모든 종류가 함께 쓴다).
    limit: Mutex<AttemptLimit>,
    /// 종목 등록 시도의 횟수 제한(키 등록과 따로 센다).
    symbols_limit: Mutex<AttemptLimit>,
    /// 시세 받기를 시작했는가(시세 흐름이 적는다).
    started: AtomicBool,
    /// 등록이 시세 흐름을 깨우는 알림.
    wake: Notify,
}

impl Desk {
    pub(crate) fn new(parts: DeskParts) -> Self {
        let desk = Self {
            presence: KeyPresence::new(Arc::clone(&parts.secrets)),
            secrets: parts.secrets,
            verifier: parts.verifier,
            home: parts.home,
            demo: parts.demo,
            watching: Mutex::new(Watching {
                symbols: parts.symbols,
                source: parts.source,
            }),
            limit: Mutex::new(parts.limit),
            symbols_limit: Mutex::new(parts.symbols_limit),
            started: AtomicBool::new(false),
            wake: Notify::new(),
        };
        // 띄우자마자 읽는 설정 상태도 사실과 같게 한다(시세 흐름이 곧 이어서 적는다).
        desk.set_started(desk.demo || desk.ready());
        desk
    }

    /// 잠금을 쥔 채 죽은 스레드가 있어도 종목 목록은 온전하다(통째로 바꿔 쓴다).
    fn watching(&self) -> MutexGuard<'_, Watching> {
        self.watching.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 지금 지켜보는 종목.
    pub(crate) fn symbols(&self) -> Vec<Symbol> {
        self.watching().symbols.clone()
    }

    /// AI 키는 API 키와 구독 토큰 가운데 하나라도 있으면 등록된 것이다.
    fn has_ai(&self) -> bool {
        self.presence.has(KeyKind::AiApiKey) || self.presence.has(KeyKind::AiOAuth)
    }

    /// 그 항목이 등록됐는가. 모르는 이름은 등록되지 않은 것이다.
    fn registered(&self, name: &str) -> bool {
        match name {
            ITEM_BROKER => self.presence.has(KeyKind::Broker),
            ITEM_AI => self.has_ai(),
            ITEM_SYMBOLS => !self.watching().symbols.is_empty(),
            ITEM_DATA => self.presence.has(KeyKind::Data),
            _ => false,
        }
    }

    /// 필수 항목이 모두 등록됐는가([`crate::SETUP_ITEMS`]의 필수 항목만 본다).
    pub(crate) fn ready(&self) -> bool {
        SETUP_ITEMS
            .iter()
            .filter(|spec| spec.required)
            .all(|spec| self.registered(spec.name))
    }

    /// 설정 상태. 값 · 값의 일부 · 길이는 없다(V46).
    pub(crate) fn status(&self) -> SetupStatus {
        let items: Vec<SetupItem> = SETUP_ITEMS
            .iter()
            .map(|spec| SetupItem {
                name: spec.name,
                required: spec.required,
                registered: self.registered(spec.name),
            })
            .collect();
        let ready = items.iter().all(|item| !item.required || item.registered);
        let (symbols, symbols_source) = {
            let watching = self.watching();
            (watching.symbols.clone(), watching.source)
        };
        SetupStatus {
            items,
            ready,
            started: self.demo || self.started.load(Ordering::SeqCst),
            demo: self.demo,
            symbols,
            symbols_source,
        }
    }

    /// 시연 모드인가.
    pub(crate) fn demo(&self) -> bool {
        self.demo
    }

    /// 시세 받기를 시작했는지 적는다(시세 흐름이 부른다).
    pub(crate) fn set_started(&self, started: bool) {
        self.started.store(started, Ordering::SeqCst);
    }

    /// 등록이 있을 때까지 기다린다. 기다리기 전에 있었던 등록도 한 번은 알려 준다.
    pub(crate) async fn woken(&self) {
        self.wake.notified().await;
    }

    /// 키를 등록한다: 입력 검사 → 시도 횟수 → 확인 → 저장. 성공하면 시세 흐름을 깨운다.
    ///
    /// 받은 값은 `pi-secrets`로만 넘긴다. 돌려주는 것은 까닭뿐이다.
    /// 받은 본문은 읽은 뒤 바로 0으로 덮는다([`KeyRequest::parse_wiping`]).
    pub(crate) async fn register_key(&self, body: &mut [u8]) -> Result<(), SetupRefusal> {
        let request = KeyRequest::parse_wiping(body)?;
        if !allow(&self.limit) {
            tracing::warn!("키 등록 시도가 너무 많아 확인하지 않고 거부합니다");
            return Err(SetupRefusal::TooManyAttempts);
        }
        let kind = request.kind;
        let registering = self.secrets.register(kind, request.values, &self.verifier);
        let outcome = match tokio::time::timeout(VERIFY_TIMEOUT, registering).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => Err(refusal(&error)),
            Err(_) => Err(SetupRefusal::Unreachable),
        };
        match outcome {
            Ok(()) => {
                tracing::info!(?kind, "키를 등록했습니다");
                self.wake.notify_one();
            }
            // 까닭만 남긴다(값도 요청 본문도 남기지 않는다).
            Err(why) => tracing::warn!(?kind, %why, "키를 등록하지 못했습니다"),
        }
        outcome
    }

    /// 지켜볼 종목을 등록한다: 입력 검사 → 시도 횟수 → 저장하고, 쓰는 종목을 바꾸고,
    /// 목록이 실제로 바뀌었을 때만 시세 흐름을 깨운다(4차).
    ///
    /// 파일을 쓰므로 막히는 작업 자리(`spawn_blocking`)에서 부른다.
    pub(crate) fn register_symbols(&self, body: &[u8]) -> Result<(), SetupRefusal> {
        let symbols = parse_symbols(body)?;
        // 저장과 바꾸기를 한 잠금 안에서 한다(함께 온 등록의 파일과 쓰는 종목이 어긋나지 않게).
        let mut watching = self.watching();
        if watching.source == SymbolsSource::Argument {
            return Err(SetupRefusal::FixedByArgument);
        }
        if !allow(&self.symbols_limit) {
            tracing::warn!("종목 등록 시도가 너무 많아 저장하지 않고 거부합니다");
            return Err(SetupRefusal::TooManyAttempts);
        }
        let changed = watching.symbols != symbols;
        save_watchlist(&self.home, &symbols).map_err(|error| {
            tracing::warn!(%error, "지켜볼 종목을 저장하지 못했습니다");
            SetupRefusal::SaveFailed
        })?;
        watching.symbols = symbols;
        watching.source = SymbolsSource::Saved;
        drop(watching);
        if changed {
            self.wake.notify_one();
        }
        Ok(())
    }
}

/// 시도 하나를 센다. 정한 횟수를 넘었으면 `false`.
fn allow(limit: &Mutex<AttemptLimit>) -> bool {
    limit
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .allow(Instant::now())
}

/// 등록 실패를 화면에 알릴 까닭으로 옮긴다.
fn refusal(error: &SecretsError) -> SetupRefusal {
    match error {
        SecretsError::VerifyFailed(VerifyFailure::Rejected) => SetupRefusal::KeyRejected,
        SecretsError::VerifyFailed(VerifyFailure::Unreachable) => SetupRefusal::Unreachable,
        SecretsError::MissingField { .. }
        | SecretsError::UnknownField
        | SecretsError::TooShort { .. }
        | SecretsError::InvalidValue { .. } => SetupRefusal::InvalidInput,
        _ => SetupRefusal::SaveFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 등록_실패는_까닭으로만_옮긴다() {
        let cases = [
            (
                SecretsError::VerifyFailed(VerifyFailure::Rejected),
                SetupRefusal::KeyRejected,
            ),
            (
                SecretsError::VerifyFailed(VerifyFailure::Unreachable),
                SetupRefusal::Unreachable,
            ),
            (SecretsError::UnknownField, SetupRefusal::InvalidInput),
            (
                SecretsError::Io {
                    op: "쓰기",
                    kind: std::io::ErrorKind::PermissionDenied,
                },
                SetupRefusal::SaveFailed,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(refusal(&error), expected, "{error}");
        }
    }
}

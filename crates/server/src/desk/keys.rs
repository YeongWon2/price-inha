//! 설정 책상의 키 등록: 앞 차수의 키 종류와 (8차) 바깥 키 칸(DART · ECOS · FRED · 백업 GitHub 토큰).

use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretsError, SlotStatus, Verifier, VerifyFailure,
};

use super::{Desk, VERIFY_TIMEOUT, allow, refusal};
use crate::SetupRefusal;
use crate::setup::KeyRequest;
use crate::slots::{KeyBody, ProviderRequest, SlotRequest, provider_declarations, registered_of};

/// 확인 시간 상한을 둔 등록 결과를 까닭으로 옮긴다.
fn settle<T>(
    outcome: Result<Result<T, SecretsError>, tokio::time::error::Elapsed>,
) -> Result<(), SetupRefusal> {
    match outcome {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(refusal(&error)),
        Err(_) => Err(SetupRefusal::Unreachable),
    }
}

/// (결정 36 · E56) 키 확인기: 체험 폴더면 바깥에 닿지 않고 받아 둔다(칸 검사는 `pi-secrets`가 이미 했다),
/// 아니면 [`crate::KeyVerifier`]로 실제로 확인한다.
pub(crate) struct KeyCheck<'a> {
    real: &'a crate::KeyVerifier,
    trial: bool,
}

impl Verifier for KeyCheck<'_> {
    async fn verify(&self, kind: KeyKind, values: &Credentials) -> Result<(), VerifyFailure> {
        if self.trial {
            return Ok(());
        }
        self.real.verify(kind, values).await
    }

    async fn verify_slot(&self, slot: KeySlot, values: &Credentials) -> Result<(), VerifyFailure> {
        if self.trial {
            return Ok(());
        }
        self.real.verify_slot(slot, values).await
    }

    async fn verify_provider(
        &self,
        keys: ProviderKeys,
        values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        if self.trial {
            return Ok(());
        }
        self.real.verify_provider(keys, values).await
    }
}

impl Desk {
    /// (결정 36 · E56) 키를 실제로 확인하지 않는 시연인가: `price-inha demo`가 만든 체험 폴더이거나, (47차) 실행
    /// 파일의 시연 띄우기(`run --demo` · `demo` — [`Desk::use_trial_keys`])다. 시연은 바깥에 닿지 않는다.
    pub(crate) fn trial(&self) -> bool {
        self.demo
            && (crate::is_demo_home(&self.home)
                || self.trial_keys.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// (47차) 실행 파일의 시연 띄우기가 부른다: 이 시연에서는 키를 실제로 확인하지 않는다.
    pub(crate) fn use_trial_keys(&self) {
        self.trial_keys
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// (48차) 시연이면 저장하기 전에 그 칸을 "확인 대기"로 표시한다. 표시하지 못하면 저장하지 않는다.
    fn mark_trial(&self, item: &str) -> Result<(), SetupRefusal> {
        crate::unverified::before_register(&self.home, item, self.trial()).map_err(|error| {
            tracing::warn!(item, %error, "시연 표시를 남기지 못해 키를 받아 두지 않습니다");
            SetupRefusal::SaveFailed
        })
    }

    /// (48차) 키 등록과 같은 확인기(시연에서 받아 둔 키를 시연이 아닌 실행이 확인할 때 쓴다).
    pub(crate) fn key_verifier(&self) -> &crate::KeyVerifier {
        &self.verifier
    }

    fn key_check(&self) -> KeyCheck<'_> {
        KeyCheck {
            real: &self.verifier,
            trial: self.trial(),
        }
    }

    /// 키를 등록한다: 입력 검사 → 시도 횟수 → 확인 → 저장. 키 종류가 등록되면 시세 흐름을 깨운다.
    ///
    /// 받은 값은 `pi-secrets`로만 넘긴다. 돌려주는 것은 까닭뿐이다.
    /// 받은 본문은 읽은 뒤 바로 0으로 덮는다([`KeyBody::parse_wiping`]).
    pub(crate) async fn register_key(&self, body: &mut [u8]) -> Result<(), SetupRefusal> {
        let request = KeyBody::parse_wiping(body)?;
        // (다듬기 E 2번) 옛 한 칸짜리 자료 키는 확인할 곳이 없다: 확인기를 부르지 않고 까닭을 분명히 해 거절한다.
        if matches!(&request, KeyBody::Kind(kind) if kind.kind == pi_secrets::KeyKind::Data) {
            return Err(SetupRefusal::RetiredItem);
        }
        if !allow(&self.limit) {
            tracing::warn!("키 등록 시도가 너무 많아 확인하지 않고 거부합니다");
            return Err(SetupRefusal::TooManyAttempts);
        }
        match request {
            KeyBody::Kind(request) => self.register_kind(request).await,
            KeyBody::Slot(request) => self.register_slot(request).await,
            KeyBody::Provider(request) => self.register_provider(request).await,
        }
    }

    /// (10b 10번) 공급자 선언의 키(네이버 뉴스 등). 선택 항목이라 시세 흐름을 깨우지 않는다.
    async fn register_provider(&self, request: ProviderRequest) -> Result<(), SetupRefusal> {
        let provider = request.keys.provider();
        self.mark_trial(provider)?;
        let check = self.key_check();
        let registering = self
            .secrets
            .register_provider(request.keys, request.values, &check);
        let outcome = settle(tokio::time::timeout(VERIFY_TIMEOUT, registering).await);
        match outcome {
            Ok(()) => {
                crate::unverified::after_register(&self.home, provider, check.trial);
                tracing::info!(provider, "공급자 키를 등록했습니다");
            }
            Err(why) => tracing::warn!(provider, %why, "공급자 키를 등록하지 못했습니다"),
        }
        outcome
    }

    /// (10b 10번) 공급자 선언마다 등록 여부(선언 순서). 키 파일을 읽지 못하면 등록되지 않은 것으로 본다.
    pub(crate) fn provider_status(&self) -> Vec<(pi_secrets::ProviderKeys, bool)> {
        let declared = provider_declarations();
        let status = self.secrets.provider_status(&declared).unwrap_or_else(|error| {
            tracing::warn!(%error, "키 파일을 읽지 못해 공급자 칸을 등록되지 않은 것으로 봅니다");
            Vec::new()
        });
        declared
            .into_iter()
            .map(|keys| (keys, registered_of(&status, keys)))
            .collect()
    }

    async fn register_kind(&self, request: KeyRequest) -> Result<(), SetupRefusal> {
        let kind = request.kind;
        self.mark_trial(crate::unverified::kind_item(kind))?;
        let check = self.key_check();
        let registering = self.secrets.register(kind, request.values, &check);
        let outcome = settle(tokio::time::timeout(VERIFY_TIMEOUT, registering).await);
        match outcome {
            Ok(()) => {
                let item = crate::unverified::kind_item(kind);
                crate::unverified::after_register(&self.home, item, check.trial);
                tracing::info!(?kind, "키를 등록했습니다");
                self.wake.notify_one();
            }
            // 까닭만 남긴다(값도 요청 본문도 남기지 않는다).
            Err(why) => tracing::warn!(?kind, %why, "키를 등록하지 못했습니다"),
        }
        outcome
    }

    /// (8차) 바깥 키 칸. 선택 항목이라 시세 흐름을 깨우지 않는다.
    async fn register_slot(&self, request: SlotRequest) -> Result<(), SetupRefusal> {
        let slot = request.slot;
        self.mark_trial(crate::unverified::slot_item(slot))?;
        let check = self.key_check();
        let registering = self.secrets.register_slot(slot, request.values, &check);
        let outcome = settle(tokio::time::timeout(VERIFY_TIMEOUT, registering).await);
        match outcome {
            Ok(()) => {
                let item = crate::unverified::slot_item(slot);
                crate::unverified::after_register(&self.home, item, check.trial);
                tracing::info!(?slot, "키 칸을 등록했습니다");
            }
            Err(why) => tracing::warn!(?slot, %why, "키 칸을 등록하지 못했습니다"),
        }
        outcome
    }

    /// (8차) 칸별 등록 여부. 키 파일을 읽지 못하면 모두 등록되지 않은 것으로 본다(값은 싣지 않는다).
    pub(crate) fn slot_status(&self) -> Vec<SlotStatus> {
        self.secrets.slot_status().unwrap_or_else(|error| {
            tracing::warn!(%error, "키 파일을 읽지 못해 칸을 등록되지 않은 것으로 봅니다");
            KeySlot::ALL
                .into_iter()
                .map(|slot| SlotStatus {
                    slot,
                    registered: false,
                })
                .collect()
        })
    }
}

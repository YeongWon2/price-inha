//! 설정 책상의 키 등록: 앞 차수의 키 종류와 (8차) 바깥 키 칸(DART · ECOS · FRED · 백업 GitHub 토큰).

use pi_secrets::{KeySlot, SecretsError, SlotStatus};

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

impl Desk {
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
        let registering =
            self.secrets
                .register_provider(request.keys, request.values, &self.verifier);
        let outcome = settle(tokio::time::timeout(VERIFY_TIMEOUT, registering).await);
        match outcome {
            Ok(()) => tracing::info!(provider, "공급자 키를 등록했습니다"),
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
        let registering = self.secrets.register(kind, request.values, &self.verifier);
        let outcome = settle(tokio::time::timeout(VERIFY_TIMEOUT, registering).await);
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

    /// (8차) 바깥 키 칸. 선택 항목이라 시세 흐름을 깨우지 않는다.
    async fn register_slot(&self, request: SlotRequest) -> Result<(), SetupRefusal> {
        let slot = request.slot;
        let registering = self
            .secrets
            .register_slot(slot, request.values, &self.verifier);
        let outcome = settle(tokio::time::timeout(VERIFY_TIMEOUT, registering).await);
        match outcome {
            Ok(()) => tracing::info!(?slot, "키 칸을 등록했습니다"),
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

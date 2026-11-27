//! (round64, 10b-2 5번) 쓰는 템플릿의 구독이 바뀔 때 다시 걸기.
//!
//! 자료 고리([`super::poll::Poller`])는 고리마다 구독을 읽고, 지난 고리의 구독과 견준다.
//!
//! - **끌 때**(켜짐 → 꺼짐: 템플릿 값 바뀜 · 다른 템플릿 · 멈춤 · 쓰는 템플릿 없음): 그 전에 받아 깨우기 엔진의
//!   묶음 · 장 밖 보관에 든 그 종류의 사건을 버린다([`pi_trigger::Trigger::discard_data`]) — 해제 직전 대기열의
//!   사건이 뒤늦게 팀을 깨우지 않게. 뉴스는 뉴스, 공시는 공시 · 재무 · 거시(결정 19: 재무 · 거시는 공시 구독에 묶는다).
//! - **다시 켤 때**(꺼짐 → 켜짐): 꺼진 사이 쌓인 뉴스 · 공시 · 재무 · 거시 바뀜으로 한꺼번에 깨우지 않는다. 다시 켠 뒤
//!   첫 가져오기는 그때 있던 것을 "본 것"으로만 적고(처음 보는 종목 · 출처와 같은 규칙), 재무 · 거시는 다음 하루
//!   보기에서 지난번 값만 새로 둔다. 그 뒤에 새로 나온 것으로는 깨운다.
//! - 읽기 오류는 같은 오류가 이어지는 동안 한 번만 남긴다(고리마다 경고가 쌓이지 않게). 읽지 못하면 아무것도 걸지
//!   않는다(앞 차수 그대로). 서버를 켠 뒤 첫 고리는 견줄 것이 없어 그대로 건다.
//!
//! 원본에 없음(원본 Hermes webhook은 경로를 끄면 받지 않을 뿐이다 — 묶음이 경로마다라 함께 사라진다). 결정 19의
//! 구독 규칙을 바뀌는 때에도 지키게 한 조정자 해석.

use pi_template::TriggerKind;
use pi_trigger::{DataKind, Trigger};

use super::Crew;
use super::poll::Subscribed;

/// 공시 구독에 묶인 자료 사건 종류(결정 19).
const DISCLOSURE_KINDS: [DataKind; 3] =
    [DataKind::Disclosure, DataKind::Financials, DataKind::Macro];

/// 지난 고리의 구독과 다시 켠 뒤 할 일.
#[derive(Default)]
pub(super) struct Resubscribe {
    last: Option<Subscribed>,
    failing: Option<String>,
    /// 다시 켠 뒤 첫 뉴스 가져오기는 본 것으로만 적는다.
    pub(super) rebase_news: bool,
    /// 다시 켠 뒤 첫 공시 가져오기는 본 것으로만 적는다.
    pub(super) rebase_dart: bool,
    /// 다시 켠 뒤 첫 재무 · 거시 보기는 지난번 값만 새로 둔다.
    rebase_changes: bool,
}

impl Resubscribe {
    /// 이번 고리의 구독을 읽고, 지난 고리와 견주어 끌 때 · 다시 켤 때의 일을 한다.
    pub(super) fn read(&mut self, crew: &Crew, trigger: &Trigger) -> Subscribed {
        let now = self.subscriptions(crew);
        if let Some(before) = self.last {
            if before.news && !now.news {
                trigger.discard_data(&[DataKind::News]);
            }
            if before.disclosure && !now.disclosure {
                trigger.discard_data(&DISCLOSURE_KINDS);
            }
            if !before.news && now.news {
                self.rebase_news = true;
            }
            if !before.disclosure && now.disclosure {
                self.rebase_dart = true;
                self.rebase_changes = true;
            }
        }
        self.last = Some(now);
        now
    }

    /// 재무 · 거시의 지난번 값을 새로 둘 차례인가(한 번 가져가면 거짓이 된다).
    pub(super) fn take_rebase_changes(&mut self) -> bool {
        std::mem::take(&mut self.rebase_changes)
    }

    /// 쓰는 템플릿의 구독. 없거나(멈춤 포함) 읽지 못하면 아무것도 걸지 않는다. 같은 읽기 오류는 한 번만 남긴다.
    fn subscriptions(&mut self, crew: &Crew) -> Subscribed {
        match crew.trading.wake_subscriptions() {
            Ok(kinds) => {
                if self.failing.take().is_some() {
                    tracing::info!("쓰는 템플릿의 구독을 다시 읽었다");
                }
                kinds.map_or_else(Subscribed::default, |kinds| Subscribed {
                    quote: kinds.contains(&TriggerKind::Quote),
                    disclosure: kinds.contains(&TriggerKind::Disclosure),
                    news: kinds.contains(&TriggerKind::News),
                })
            }
            Err(error) => {
                let text = error.to_string();
                if self.failing.as_deref() != Some(text.as_str()) {
                    tracing::warn!(error = %text, "쓰는 템플릿의 구독을 읽지 못했다(같은 오류는 다시 남기지 않는다)");
                    self.failing = Some(text);
                }
                Subscribed::default()
            }
        }
    }
}

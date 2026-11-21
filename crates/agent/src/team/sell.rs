//! 팔기 주문 안(9차): 팀장이 팔기로 정한 뒤 매매 담당이 같은 종목 · 계좌 종류의 보유 매매마다
//! 파는 주문 안을 만든다(V4 · E20 · P7 · E38, 결정 2026-11-21 13번).

use pi_store::{Trade, TradeId, TradeSide, TradeState};

use super::{Session, Team};
use crate::error::{Error, TurnFailure};
use crate::ports::SellDraft;
use crate::prompt;
use crate::role::Role;
use crate::verdict::parse_sell_draft;

impl Team {
    /// 팔기로 판단된 매매에 대해 매매 담당이 같은 종목 보유를 파는 주문 안을 만들어 넘긴다(V4, 9차).
    ///
    /// - 매매가 판단됨이 아니면 [`Error::TradeNotJudged`](E20), 저장소의 방향이 팔기가 아니면(사기 · 없음)
    ///   [`Error::TradeNotSell`]. 둘 다 AI를 부르지 않는다.
    /// - 팔 것: 저장소의 보유중 매매 가운데 종목 · 계좌 종류가 판단 매매와 같은 것(만든 순서).
    ///   없으면 AI를 부르지 않고 빈 목록이다.
    /// - 있으면 매매 담당 턴 한 번. 답은 `{"side": "sell", "note": 글, "quantity"?: 수, "price"?: 수}`.
    ///   `side`가 `sell`이 아니거나 모양이 틀리면 매매 담당 응답 실패([`Error::RoleUnavailable`]).
    /// - 보유 매매마다 [`SellDraft`] 하나(같은 설명 · 참고값)를 [`Tools::submit_sell_draft`]로 하나씩 넘기고,
    ///   넘긴 것을 돌려준다. 넘기기가 실패하면 그 오류로 끝난다.
    /// - 설명은 넘기기 전에 가린다(E38). 판단 매매의 상태는 옮기지 않는다.
    ///
    /// [`Tools::submit_sell_draft`]: crate::Tools::submit_sell_draft
    pub async fn draft_sell(&self, trade: TradeId) -> Result<Vec<SellDraft>, Error> {
        let (found, holdings) = self.sell_targets(trade)?;
        if holdings.is_empty() {
            return Ok(Vec::new());
        }
        self.sell_holdings(&found, &holdings).await
    }

    /// 팔기 주문 안의 대상: 판단 매매와 팔 보유 매매들(만든 순서). 판단됨 · 팔기 방향을 확인한다.
    fn sell_targets(&self, trade: TradeId) -> Result<(Trade, Vec<TradeId>), Error> {
        let found = self.ports.store.get_trade(trade)?;
        if found.state != TradeState::Judged {
            return Err(Error::TradeNotJudged {
                trade,
                state: found.state,
            });
        }
        if self.ports.store.trade_side(trade)? != Some(TradeSide::Sell) {
            return Err(Error::TradeNotSell { trade });
        }
        let holdings = self
            .ports
            .store
            .trades_in_state(TradeState::Holding)?
            .into_iter()
            .filter(|held| held.symbol == found.symbol && held.account == found.account)
            .map(|held| held.id)
            .collect();
        Ok((found, holdings))
    }

    /// 매매 담당 턴 한 번으로 보유 매매마다 팔기 주문 안을 만들어 하나씩 넘긴다.
    async fn sell_holdings(
        &self,
        found: &Trade,
        holdings: &[TradeId],
    ) -> Result<Vec<SellDraft>, Error> {
        let out = self
            .turn(Role::Trader, found.id, &prompt::sell_turn(found, holdings))
            .await?;
        let reply = parse_sell_draft(&out.text).ok_or(Error::RoleUnavailable {
            role: Role::Trader,
            cause: TurnFailure::InvalidStructuredReply,
        })?;
        let note = self.ports.redactor.redact(&reply.note);
        let mut sent = Vec::with_capacity(holdings.len());
        for holding in holdings {
            let draft = SellDraft {
                judged: found.id,
                holding: *holding,
                symbol: found.symbol.clone(),
                note: note.clone(),
                ai_quantity: reply.ai_quantity,
                ai_price: reply.ai_price,
            };
            self.ports.tools.submit_sell_draft(&draft).await?;
            sent.push(draft);
        }
        Ok(sent)
    }

    /// 팔기로 판단된 뒤 매매 담당의 팔기 주문 안들([`Team::draft_sell`]과 같은 길).
    /// 팔 보유가 없으면 방 로그에 `nothing_held`를 남기고, 매매 담당이 답하지 못하면 실패를 남긴다.
    /// 어느 쪽이든 빈 목록이다.
    pub(super) async fn sells_after_judgment(
        &self,
        session: &Session<'_>,
    ) -> Result<Vec<SellDraft>, Error> {
        let trade = session.request.trade;
        let (found, holdings) = self.sell_targets(trade)?;
        if holdings.is_empty() {
            session.room.nothing_held(trade)?;
            return Ok(Vec::new());
        }
        match self.sell_holdings(&found, &holdings).await {
            Ok(drafts) => Ok(drafts),
            Err(Error::RoleUnavailable { role, cause }) => {
                session.room.failed(role, &cause.to_string())?;
                Ok(Vec::new())
            }
            Err(other) => Err(other),
        }
    }
}

# rules 모듈 (T6, T13의 손실 · 멈춤 판단)

크레이트 `pi-rules`, 폴더 `crates/rules/`. 구성 요소 A6 규칙 엔진. 쓰는 것: `pi-template`(RiskParams · 상한), `pi-paper`(ApprovedOrder · CheckAuthority · 수수료 설정), `pi-market`(종목 · 시세 · 달력). 순수 계산이다.
근거: `dev.md` C9 · E10 · E11 · E18 · E20 · E21 · E23 · E24 · E31 · E33 · E34 · N6 · V17 · V24~V26, `principles.md` P1~P13, `spec.md` FN1 · F3 · F4 · 조건표.

## 원본 동작 근거

- 원본에 없음(우리 고유). Hermes · OpenClaw · DeskRPG에는 매매 규칙 엔진이 없다(`docs/dev/conformance.md` 5장).
- 따르는 원본의 생각 하나: 어떤 자동 승인 설정으로도 풀리지 않는 "바닥(floor)" 금지를 먼저 본다 — `docs/research/triggers-approval-memory-deep.md` 3.4.
- 숫자와 규칙의 근거: `docs/research/stock-investing-principles.md` 2.2(포지션 크기) · 2.4 · 2.5 · 2.6 · 5장(시장 규칙) · 8.1.

## 만드는 것 (1차)

1. `check(draft, ctx) -> Outcome` (C9). 점검 순서는 FN1을 따른다(원칙은 22개 기준이고 이 모듈은 P1~P13을 맡는다 — 결정 2026-11-16 3번).
   - 판단 상태: 판단됨이 아니면 막힘(E20).
   - 팀 상태: 운용 중이 아니면 사는 주문을 막고, 손절 · 청산은 어떤 멈춤에서도 받는다(E31 · E34).
   - 근거가 다 있는가(P9 · E10, 빠진 칸 목록 · V17).
   - 원칙 P1~P13(E11, 어긴 원칙 번호).
   - 주문 직전 점검 P12: 장 시간(E18), 가격 제한, 중복 주문, 1회 최대 금액, 일일 매매 횟수, API 요청 한도(호출하는 쪽이 센 값을 받는다).
   - 금액 한도를 넘으면 `NeedsApproval`(E21 · V25).
   - 다 지나면 `Pass(ApprovedOrder)`.
2. 수량 계산(P1 · P7 · P8): `floor(평가액 × R% / (진입 − 손절))`, 종목 · 업종 비중 상한, 호가 단위 맞춤, AI가 낸 값과 다르면 작은 쪽. 통화는 섞지 않는다.
3. 원칙 판정 하나하나
   - P2: 손절 필수, 보유 중 손절은 좁히기만.
   - P5: 금지 상품. 설정의 금지 목록과 상품 종류 표시를 본다.
   - P6: 손실 중인 종목은 더 사지 않는다.
   - P10: 자료의 알게 된 시각이 판단 시각보다 늦으면 막는다.
   - P11 · N6: 시세가 30초 넘게 늦거나 계좌가 어긋나면 새 주문을 막는다.
   - P13: 비용 · 세금 · 미끄러짐을 뺀 기대값이 양수일 때만 받는다.
   - P5 · P8 · P12의 자료(상품 종류 · 업종 · VI · 거래정지)는 1차에서 설정 파일의 금지 종목과 업종표를 쓴다. VI · 거래정지를 알 수 없으면 신규 주문을 막는다(결정 19번).
4. `issue(&CheckAuthority, Pass) -> CheckPermit` (E33). 권한은 조립하는 쪽이 rules에만 건넨다.
5. 손실과 멈춤(T13 · M1)
   - `day_loss`(실현 + 평가, 그날 시작 평가액 기준, 통화별로 따로)와 `drawdown`(최고 평가액 기준)을 센다(결정 31번).
   - `evaluate_stops(ctx) -> Option<요구 전이>`: 하루 한도에 닿으면 하루 한도 멈춤(E23 · P3), 최대 낙폭에 닿으면 전체 정지(E24 · P4).
   - 전이를 실제로 하는 것은 store다.
6. 막힌 이유는 store의 `BlockKind`와 1:1로 맞는 enum으로 낸다(V26).

## 받는 값을 믿지 않는다

`RiskParams`의 칸은 공개라 `adjust`를 거치지 않고 바뀔 수 있다. 점검은 언제나 `AbsoluteCaps::FIXED`로 다시 막는다(템플릿 값이 상한을 넘어도 상한이 이긴다).

## 승격 판정

승격 기준 판정은 `pi_template::RiskParams::evaluate_promotion`으로만 부른다(`promotion.evaluate`를 직접 부르면 낡은 낙폭 기준을 쓸 수 있다).

## 2차 작업 (1차 검토의 참고 — exec 전에 한다)

1. 시세의 종목이 주문 안의 종목과 다르면 막는다.
2. 늦은 시세 기준(P11)은 받은 설정을 30초로 다시 막는다(`min(설정, 30초)`).
3. P6 손실 중 판단은 `min(장부 마지막 값, 시세)`로 본다.
4. 중복 점검에서 손절 · 청산은 열린 익절 매도에 막히지 않는다(열린 주문 취소는 exec가 한다).
5. 승인된 주문을 점검 통과로 바꾸는 길: `approved(PendingApproval, 승인 소비 결과) -> Passed`(다시 점검을 거친다). 승인이 필요한 사는 주문은 지정가만(결정 7번) — 시장가 사기가 한도를 넘으면 막는다.
6. `Passed`는 `Clone`이 아니다.

## 4차 작업 (2026-11-21 발표 자료 대조)

근거: `docs/dev/decisions.md` "2026-11-21 발표 자료 대조 뒤 결정" 5 · 8번, `spec.md` 상태표 · F1 2b · 2d · F2 2a · F4 7b2 · FN2 3b3, `dev.md` E22 · E24 · E30 · E31 · E34 · N6, `principles.md` P11 · P12.
먼저 할 것: market 7차(거래 상태 값 · 추정)가 green이어야 3번을 시작할 수 있다. 3번은 `pi_market::TradingStatus`를 받는다.
공개 칸 · 공개 구조체 모양은 바꾸지 않는다(`CheckContext` · `SymbolStatus` · `OrderDraft`의 칸을 더하면 잠긴 테스트의 구조체 문장이 깨진다). 새 것은 새 함수 · 새 타입 · enum 변형 더하기로만 한다.

### 1. 전체 정지 중의 파는 주문 (F4 7b2 · E24)

(a) 공개 API 바뀜
- `Purpose`에 `TeamExit`(팀장이 팔기로 판단한 매도)를 더한다. `side()`는 팔기, `is_buy()`는 거짓. agent 9차의 팔기 주문 안을 server가 이 목적으로 바꿔 넣는다.
- `Reason::TeamNotOperating(TeamStatus)`의 문서를 "운용 중이 아닐 때의 사는 주문, 또는 전체 정지 중의 걸어 둔 손절 · 청산이 아닌 파는 주문"으로 넓힌다(변형은 그대로).

(b) 동작 규칙 · 경계
- 점검 2단계(팀 상태)에 하나를 더한다: 팀이 `FullStop`이고 목적이 `StopLoss` · `Liquidation`이 아니면(`TakeProfit` · `TeamExit`) `Reason::TeamNotOperating(FullStop)`로 막는다. 근거 · 원칙보다 먼저 본다(사는 주문의 막힘과 같은 자리).
- `StopLoss` · `Liquidation`은 지금처럼 어떤 멈춤에서도 지나간다(E34). `Liquidation`은 템플릿의 손절 방식에 들어 있는 장 마감 청산 · 최대 보유 기간(시간 손절)으로 걸어 둔 것이라 "걸어 둔 손절"로 읽는다(template 4차 2번이 이 둘을 손절 방식으로 셈). 이 해석은 사용자 확인 대상이다(아래 "물을 것").
- `DailyLimitHalt` · `AnomalyHalt` · `Resting`의 파는 주문은 지금 그대로 지나간다(하루 한도 멈춤 "파는 것만 한다", 이상 감지 멈춤 "새 주문만 멈춘다" — 파는 주문은 새 매매가 아니다).
- `TeamExit`의 나머지 점검은 `Liquidation`과 같다: 근거(P9) 확인, 가진 수량을 넘지 않음(P5), 중복 점검에서 열린 익절 매도에 막히지 않음(2차 4번, 열린 주문 취소는 exec), 금액 한도 승인 없음(P18).
- `approved()`는 `evaluate`를 다시 부르므로 같은 규칙을 따른다.

(c) 원본 동작 근거: 원본에 없음 — 스펙 F4 7b2("새 매매를 모두 멈추고 걸어 둔 손절값에 닿을 때만 판다"), `dev.md` E24("걸어 둔 손절만 실행"). 어떤 설정으로도 풀리지 않는 바닥 금지를 먼저 보는 생각은 `docs/research/triggers-approval-memory-deep.md` 3.4. 손절 · 청산은 승인 대상이 아니게 둔다: `docs/research/remote-access-security.md` 5.3 끝 문단.

(d) 테스트로 확인할 것 (검증 주석: E24 E34 P4)
- `FullStop`에서 `TakeProfit` · `TeamExit`는 `TeamNotOperating(FullStop)`, `StopLoss` · `Liquidation`은 통과(표로).
- `FullStop`이 아닌 네 상태에서 `TeamExit` · `TakeProfit`은 지금처럼 통과(가진 것이 있을 때).
- `FullStop`에서 근거가 빈 `TakeProfit`도 근거보다 팀 상태가 먼저 막는다(순서).
- `TeamExit`는 열린 익절 매도에 중복으로 막히지 않고, 가진 수량을 넘지 않는다.

(e) 잠긴 옛 테스트와 부딪히는 곳: 없음. `tests/suite/check_flow.rs:121-150`(`stop_loss_and_liquidation_pass_in_every_team_state`)은 손절 · 청산이 `FullStop`에서도 지나가는 것을 못 박아 두었고 위 설계는 그것을 지킨다. 사용자가 "전체 정지에서는 손절(`StopLoss`)만"으로 정하면 이 테스트의 `Purpose::Liquidation` × `FullStop` 칸이 반대가 되어 **reopen 필요**(`check_flow.rs:123-148`).

### 2. 이상 감지 멈춤에 들어가고 스스로 풀기 (결정 8번 · 상태표 · P11 · N6)

(a) 공개 API 바뀜 (새 파일 `src/anomaly.rs`)
- `pub struct HealthWatch<'a> { pub team: TeamStatus, pub now: DateTime<Utc>, pub calendar: &'a MarketCalendar, pub quotes: &'a [(Symbol, Option<Quote>)], pub stale: StalePolicy, pub health: Health, pub healthy_since: Option<DateTime<Utc>>, pub recovery: RecoveryPolicy }`. `quotes`는 지켜보는 종목 · 보유 종목마다 마지막 시세(없으면 `None`). `healthy_since`는 호출하는 쪽이 센 "모든 것이 정상이 된 첫 시각"(지금 정상이 아니면 `None`).
- `pub struct RecoveryPolicy { pub hold: Duration }`, `impl Default`(아래 기본값). `pub const DEFAULT_RECOVERY_HOLD`.
- `pub enum HealthTransition { AnomalyHalt, Recovered }`.
- `pub fn is_abnormal(w: &HealthWatch<'_>) -> bool`, `pub fn evaluate_health(w: &HealthWatch<'_>) -> Option<HealthTransition>`.

(b) 동작 규칙 · 경계
- 비정상: 시세 하나라도 늦음(`is_stale`, 기준은 `min(w.stale.max_age, 30초)` — 2차 2번과 같다), 장이 열려 있는데 시세가 `None`, 계좌 불일치, API 오류, 자료 이상. 장이 닫힌 동안의 늦음은 `is_stale`이 보지 않는다(달력이 낡으면 늦음 — market 2차 규칙 그대로).
- `AnomalyHalt`로: 팀이 `Operating`(F2 2a2) 또는 `Resting`(F1 2d1)이고 비정상이면. `DailyLimitHalt` · `FullStop` · 이미 `AnomalyHalt`면 `None`(store의 허용 표에 그 전이가 없고, 그 상태들은 이미 새로 사지 않는다).
- `Recovered`: 팀이 `AnomalyHalt`이고, 지금 비정상이 아니고, `healthy_since`가 있고 `now - healthy_since ≥ recovery.hold`일 때. 정확히 `hold`면 푼다. `healthy_since`가 `now`보다 뒤면(시계 어긋남) 풀지 않는다.
- `Recovered`를 받은 쪽(server)은 store의 `AnomalyHalt → Resting`(허용 표에 이미 있다)으로 옮긴 뒤 그날 계획대로 한다(오늘 매매이고 장이 열려 있으면 `Resting → Operating`). 그래서 store 표를 바꾸지 않는다. 사람이 다시 시작하는 길(server `restart`)은 그대로 둔다.
- `hold` 기본값 60초(추정 · 미확정, 설정값). 까닭: 시세가 30초 늦음 경계를 오가며 멈춤과 풀림이 번갈아 생기지 않게 한다. 풀린 순간 바로 사지 않는 쪽이 안전하다.
- 전이를 실제로 하는 것, 신호(늦은 시세 · API 오류 · 계좌 불일치)를 모아 넘기는 것, `healthy_since`를 세는 것은 server 차수다.

(c) 원본 동작 근거: 원본에 없음 — 스펙 상태표("시세 지연이나 오류로 새 주문만 멈추고 풀리면 다시 시작한다"), F1 2b("자료와 연결이 정상인지 다시 확인한다"), 결정 2026-11-21 8번. `principles.md` "이상 감지 (P11) — 이상이 풀리면 에이전트가 알아서". 머무는 시간(`hold`)은 우리가 정한 값이다.

(d) 테스트로 확인할 것 (검증 주석: N6 P11)
- 상태 5개 × (정상 · 늦은 시세 · 시세 없음 · 계좌 불일치 · API 오류 · 자료 이상) 표: `Operating` · `Resting`만 `AnomalyHalt`, 나머지 `None`.
- 늦음 경계: 받은 지 정확히 30초는 정상, 30초 넘으면 비정상. 설정이 60초여도 30초로 막힌다. 장 닫힘의 오래된 시세는 정상, 달력 밖은 비정상.
- `AnomalyHalt`에서 정상 59초는 `None`, 60초는 `Recovered`, 지금 비정상이면 `healthy_since`가 있어도 `None`, `healthy_since`가 `None`이거나 미래면 `None`.
- `FullStop`은 정상이어도 `Recovered`를 내지 않는다(전체 정지는 사람만 푼다, E9 · V42).

(e) 잠긴 옛 테스트와 부딪히는 곳: 없음(새 함수). 참고로 server의 `crates/server/tests/round9/loss.rs:160-168`은 증권사 오류가 풀린 **바로 그 시각**에 사면 `AnomalyHalt`로 막히는 것을 확인한다. `hold`가 0보다 크면 이 테스트와 맞는다(server 차수에서 볼 것).

### 3. VI · 거래정지가 '모름'일 때 시세 추정 결과로 (결정 5번)

(a) 공개 API 바뀜
- `impl SymbolStatus { pub fn from_trading_status(status: pi_market::TradingStatus) -> SymbolStatus }`. `pi_market::TradingStatus`는 market 7차 3번, 공급자 값과 추정을 합친 값은 market 7차 5번 `pi_market::resolve_status(..) -> (TradingStatus, StatusSource)`가 준다. server는 그 결과를 이 함수로 바꿔 `CheckContext::status`에 넣는다.

(b) 동작 규칙 · 경계
- `Normal` → `{ vi: Some(false), halted: Some(false) }`(통과). `Vi` → `{ vi: Some(true), halted: None }`. `Halted` → `{ vi: None, halted: Some(true) }`. `Unknown`과 앞으로 더해질 모르는 변형(`non_exhaustive`) → `{ None, None }`(지금처럼 `StatusUnknown`으로 막힘).
- `check`는 바꾸지 않는다. VI · 거래정지는 지금처럼 사는 주문에만 본다(파는 주문 · 손절은 모른다고 막지 않는다).
- 추정 함수(`estimate_status`)와 합치기(`resolve_status`)는 market이 만든다. rules는 그 값을 받기만 한다(하지 않는 것: VI · 거래정지 자료를 직접 가져오지 않는다).

(c) 원본 동작 근거: 원본에 없음 — 사용자 결정 2026-11-21 5번(2026-11-16 19번을 바꿈). 숫자 근거는 market 7차가 가리키는 `docs/research/stock-investing-principles.md` 5장.

(d) 테스트로 확인할 것 (검증 주석: P12)
- 네 값의 바꿈 표. 바꾼 값을 넣은 사는 주문이 `Normal`이면 통과, `Vi`면 `ViActive`, `Halted`면 `TradingHalted`, `Unknown`이면 `StatusUnknown`.
- 같은 값으로 파는 주문은 막히지 않는다.

(e) 잠긴 옛 테스트와 부딪히는 곳: 없음. `tests/suite/principles_table.rs:219` · `:231`(`StatusUnknown`)은 `SymbolStatus`의 `None`을 직접 넣으므로 그대로 맞다.

### 4. 답 없는 사기를 금액 한도 안으로 줄여 다시 점검 (FN2 3b3 · E22)

(a) 공개 API 바뀜
- `pub fn reduce_within_limit(pending: PendingApproval, draft: &OrderDraft, ctx: &CheckContext<'_>) -> Result<Passed, Blocked>`.
- `Violation`에 `NothingWithinLimit`(P12)을 더한다.

(b) 동작 규칙 · 경계
- approval의 `DueAction::ReduceWithinLimit`를 받은 server가 부른다. 이 함수는 승인 증표를 받지 않는다(답 없음이라 승인이 없다).
- `draft`를 지금의 `ctx`로 `evaluate`와 같이 다시 점검한다(판단 · 팀 상태 · 근거 · 원칙 · 주문 직전 점검). 막히면 그 막힘 그대로.
- 다시 셈한 주문의 종목 · 방향 · 호가 유형이 `pending.order()`와 다르면 `ApprovalMismatch`(승인을 기다리던 주문이 아니다). 사는 주문이 아니면 같은 막힘.
- 수량 = `min(다시 셈한 수량, pending의 수량, floor(지금 금액 한도 ÷ 지정가))`. 금액 한도는 `check`와 같은 `평가액 × 자동 주문 금액 한도%`(지금 `ctx`로 다시 센다). 지정가는 다시 셈한 값(시세가 내려갔으면 그 값)을 쓴다.
- 줄인 수량이 0이면 `Violations([NothingWithinLimit])`. 줄인 금액은 한도 **이하**라 승인 없이 `Passed`이고 `Passed::approval`은 `None`이다.
- 시장가 사기는 애초에 승인 대기에 오지 않는다(결정 7번). 와도 `MarketBuyOverAutoLimit` 막힘 그대로.

(c) 원본 동작 근거: 원본에 없음(원본의 답 없음은 거절 하나) — 스펙 FN2 3b3, `dev.md` E22, 결정 2026-11-16 5번. `docs/research/remote-access-security.md` 5.4(답 없음이면 "한도 안에서" 판단, 손실 상한은 금액 한도 + P1~P13).

(d) 테스트로 확인할 것 (검증 주석: E22 E21 P12)
- 한도를 넘어 승인 대기가 된 사기 → 같은 `ctx`로 줄이면 금액 ≤ 한도, 수량은 손으로 센 `floor` 값과 끝자리까지 같음, 호가 단위 그대로.
- 그 사이 팀이 멈췄으면 `TeamNotOperating`, 종목이 다르면 `ApprovalMismatch`, 한 주도 못 사면 `NothingWithinLimit`.
- 시세가 내려가 다시 셈한 수량이 pending보다 작으면 작은 쪽.

(e) 잠긴 옛 테스트와 부딪히는 곳: 없음(새 함수 · 새 변형). `Violation`에 변형을 더하므로 `Violation::principle` · `Blocked::detail`의 글을 함께 채운다.

### 물을 것
- 전체 정지에서 `Liquidation`(장 마감 청산 · 최대 보유 기간)을 걸어 둔 손절로 보고 통과시킬지(추천), 손절(`StopLoss`)만 통과시킬지(그러면 위 1번 (e)의 reopen).
- 이상 감지 풀림 머무는 시간 `hold` 60초(추정)로 둘지.

## 하지 않는 것

AI 호출, 승인 요청 만들기(approval), 주문 내기(exec), 저장, 시계 읽기. VI · 거래정지 자료를 직접 가져오지 않는다(받은 표시만 본다).

## 테스트 주의

- 원칙마다 어기는 주문 안 하나씩 표로 넣어 모두 막히는지 본다(V24).
- 속성 테스트: 어떤 입력에도 `수량 × (진입 − 손절) ≤ 평가액 × R%`, 비중 ≤ 상한.
- 돈은 손으로 계산한 값과 끝자리까지 비교한다.
- 시세 · 달력은 `pi-market` FakeAdapter의 값을 쓴다.

## 필수 번호

C9 E10 E11 E18 E20 E21 E23 E24 E31 E33 E34 N6 V17 V24 V25 V26 P1 P2 P3 P4 P5 P6 P7 P8 P9 P10 P11 P12 P13

## 3차 작업 (후보 일괄 처리)

- 점검 통과 표시를 매매 번호에 묶는 길: `issue_for_trade(&CheckAuthority, Passed, 매매 번호) -> CheckPermit`. 내용은 `issue`와 같고 표시가 그 매매 번호를 지닌다(paper `CheckPermit::trade`). `OrderDraft`는 매매 번호를 받지 않는다(공개 칸을 더하면 기존 쓰는 곳이 깨진다). `issue`는 그대로 둔다.

## 4차 작업 (2026-11-24 조정자, 결정 14 — 수수료 넣은 금액 한도 · 승인 표시)

1. **금액 한도에 사는 수수료**: 사는 주문의 금액 한도 판정(E21)과 [`Passed`]의 금액은 `수량 × 진입가 + 올림(수량 × 진입가 × 시장 수수료율)`이다(올림 단위는 그 시장 돈의 최소 단위 — paper `money.rs`의 사는 주문 묶는 돈과 같은 셈). 파는 주문은 지금 그대로.
2. **답 없음 줄임도 같은 셈**: `reduce_within_limit`의 수량은 1번 금액이 한도 이하인 가장 큰 수량이다(지금의 `floor(한도 ÷ 지정가)`에서 수수료를 넣는다).
3. **승인 표시 표**: `issue_for_trade`가 받은 `Passed`에 승인 증표(`Passed::approval`)가 있으면 paper `approve_granted_for_trade`로 낸다(`granted() == true`). 증표가 없으면 지금처럼. 매매에 묶이지 않은 `issue`에 증표가 있는 `Passed`가 오면 표시를 내지 않는 대신 막힘(`ApprovalMismatch`)으로 한다 — 시그니처를 바꿔야 하면 `issue`는 증표 있는 `Passed`를 받아도 `granted == false`로 내지 않게 `Result`를 돌려준다(어느 쪽인지 N1이 정해 보고).
4. **`reduce_within_limit` 속성 시험**: 아무 한도 · 값 · 수수료율에서 결과 수량의 1번 금액은 한도 이하이고, 한 주 더하면 한도를 넘는다(또는 요청 · 다시 셈한 수량에 막힌다).

- 원본 동작 근거: 원본에 없음 — 스펙 E21 · E22 · E33, 결정 14.
- 시험으로 확인할 것: 한도 100만 · 수수료 0.015%에서 수량 × 값이 한도와 같으면 승인대기(전엔 통과), 수수료까지 한도 안이면 통과 / 답 없음 줄임 수량이 수수료를 넣어 한도 이하 / 승인 증표로 지난 주문의 표시는 `granted`, 아니면 아님 / 4번 속성 시험.

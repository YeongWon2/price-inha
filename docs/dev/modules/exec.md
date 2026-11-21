# exec 모듈 (T7의 남은 것: 주문 실행기)

크레이트 `pi-exec`, 폴더 `crates/exec/`. 구성 요소 A8 가운데 모의계좌 바깥 부분. `pi-paper`(Broker · CheckPermit · 스냅샷), `pi-market`, `pi-store`를 쓴다.
근거: `dev.md` C10 · E12 · E13 · E16 · E25 · E29 · E30 · E33 · V6, `spec.md` F3 4 · F4, `docs/dev/modules/paper.md`.

## 원본 동작 근거

- 원본에 없음(우리 고유): 모의계좌와 주문 실행은 원본에 없다(`docs/dev/conformance.md` 5장 F1).
- 따르는 원본의 규칙 하나: 실행하기 전에 먼저 저장하고, 저장에 실패하면 실행하지 않는다(persist-before-execute) — `docs/research/agent-runtime-deep.md` 1장 "가격인하에 옮길 때".

## 만드는 것 (1차)

1. `Executor::submit(trade_id, permit, 템플릿 계좌 종류, 승인 소비 결과)` (C10)
   - 템플릿 계좌 종류와 증권사 종류가 다르면 막힘(E25). 실전 증권사 구현은 없으므로 실전 주문은 언제나 막힌다(결정 22번).
   - 매매를 주문됨으로 먼저 저장한 뒤 `place_order`를 부른다.
   - 점검 표시가 없으면 받지 않는 것은 `pi-paper`가 이미 한다(E33).
2. 체결 이벤트를 저장소 전이로 옮긴다: 체결되면 보유중. 일부 체결 뒤 남은 것이 취소되면 체결된 수량으로 손절값을 건다(E29). 미체결 취소 이유를 남긴다(E12).
3. 증권사가 받지 않거나 응답이 없으면 `AnomalyStop`을 돌려준다(이상 감지 멈춤 요구, E13).
4. 파는 주문이 실패하면 이상 감지로 바꾸고, 물러서기 간격을 늘리며 계속 다시 낸다(E30).
5. 바뀔 때마다 계좌 스냅샷을 store 9차의 칸에 저장하고, 켤 때 되살린다(V6).
6. 다시 켠 뒤 저장소의 주문됨 매매와 증권사 주문 목록을 맞춰 보기 전에는 `ready()`가 거짓이고 새 주문을 받지 않는다(E16).

## 손절 감시는 여기서 한다

트리거 감시(pi-trigger)는 전체 정지 중에 팀을 깨우지 않는다(원본 Hermes tick 3단계). 그러나 손절 · 청산은 어떤 멈춤에서도 받아야 한다(E31 · E34). 그래서 보유 종목의 손절값 감시와 손절 주문은 팀을 거치지 않고 이 모듈이 시세를 보고 직접 낸다(점검은 rules의 손절 · 청산 길).

## rules와 맞출 것 (rules 2차 검토의 참고)

- 손절 · 청산을 내기 전에 같은 종목의 열린 매도(익절 · 이전 손절)를 먼저 취소한다(rules는 손절 · 청산의 중복 점검을 건너뛴다 — 같은 수량의 매도가 겹치지 않게 exec가 지킨다).
- 승인받은 주문은 approval의 소비 결과로만 `rules::approved`를 부른다(rules의 증표는 대기 주문과 묶여 있지 않다).

## 하지 않는 것

점검 · 수량 계산(rules), 승인(approval), 실제 증권사(규칙 ARCH-REAL-BROKER로 막는다).

## 테스트 주의

증권사는 `PaperBroker`의 장애 흉내 자리로 E13 · E30을 일으킨다. 저장은 임시 폴더의 `pi-store`로 한다. 껐다 켜기는 스냅샷 값과 새 Executor로 흉내 낸다.

## 필수 번호

C10 E12 E13 E16 E25 E29 E30 E33 V6 E22

## 1차 검토 참고 (다음 차수 후보)

- `submit`은 `ApprovalGrant`가 있는지만 본다. 저장소의 승인 번호 · 지문과 대조하거나, 증표를 approval 모듈만 만들 수 있게 막는다.
- 점검 통과 표시가 매매 번호에 묶이지 않고, 파는 수량이 그 매매의 남은 수량 이하인지 보지 않는다.
- 다시 낼 파는 주문 목록이 저장되지 않아 재시작 때 사라진다(E30). 파는 키가 시도마다 새로 생겨 실제 증권사라면 이중 매도 위험(모의계좌는 안전).
- 모의계좌의 끝난 주문 보관이 붙으면 reconcile이 사는 주문을 못 찾아 손절 감시가 사라질 수 있다.
- 주문은 나갔는데 스냅샷 저장이 실패하면 오류를 돌려준다.
- 손절값 감시와 손절 주문(지시서 메모)은 아직 자리(`guards`)만 있다.
- 받은 걱정(N1): 이상 감지 뒤 팀 상태를 바꾸는 일은 부르는 쪽 몫.

## 2차 작업 (1차 검토 참고에서)

- 승인 증표: `submit`은 받은 `ApprovalGrant`가 그 매매의 승인인지(매매 번호 · 내용 지문) 확인한다. 증표를 approval 모듈만 만들 수 있게 할 수 없으면 store의 승인 기록과 대조한다. 시험: 다른 매매의 증표 · 지문이 다른 증표로는 사는 주문이 나가지 않는다.
- 점검 통과 표시가 그 매매 번호에 묶이고, 파는 수량은 그 매매의 남은 수량 이하다.
- 다시 낼 파는 주문 목록을 스냅샷에 남겨 재시작 뒤에도 이어 낸다(E30). 파는 키는 같은 시도 묶음에서 같게 둔다.
- 주문은 나갔는데 스냅샷 저장만 실패하면 주문됨으로 기록하고 다음 저장에서 맞춘다(오류로 끝내지 않는다).

### 2차 검토 참고 (다음 차수 후보)

- 장부에 없는 보유 매매는 파는 수량 검사를 건너뛰어, 같은 종목의 다른 매매 몫까지 팔 수 있다(끝난 주문 보관이 붙을 때 다시 본다).
- 점검 통과 표시가 매매 번호가 아니라 종목 · 방향으로 묶인다. 승인 없는 판단됨 매매의 사는 수량은 Trade에 수량 칸이 없어 대조하지 못한다.
- 시장가 주문에 증표를 붙이면 가격 대조에서 늘 막힌다(안전 쪽). 시장가 승인을 쓰면 맞춘다.
- 주문은 나갔는데 스냅샷 저장만 실패하는 경우의 시험이 없다(공개 API로 일으킬 수 없음).

## 3차 작업 (후보 일괄 처리)

- 보유 장부를 스냅샷의 실행기 몫에 남기고 켤 때 되살린다. 맞춰 보기는 장부를 지우지 않고 그 위에 증권사 주문을 더한다. 그래서 모의계좌가 끝난 주문을 덜어 낸(보관한) 뒤 다시 켜도 손절 감시와 파는 수량 검사가 남는다(1차 후보 · 2차 후보 "장부에 없는 보유").
- 장부가 없는 보유 매매(2차 이전 문서)는 맞춰 볼 때 증권사 보유 수량에서 같은 종목의 다른 매매 장부 몫을 뺀 것으로 장부를 만든다. 장부가 끝내 없으면 파는 수량 검사에서 남은 수량 0으로 본다(건너뛰지 않는다).
- 손절 감시(F4 대안 흐름 1a): `stop_hits(시세)`는 시세 값이 손절값 이하인 감시 자리를 돌려준다(같은 종목 · 같은 통화, 손절 다시 내기를 기다리는 매매는 뺀다). `stop_draft(감시, 시세)`는 논의 없이 손절 주문 안(`pi_rules::OrderDraft`, 목적 손절 · 시장가 · 수량은 남은 수량 · 근거는 매매의 것 · 자료에 그 시세)을 만든다(1a1). 점검(rules)과 표시 · `sell`로 잇는 조립은 부르는 쪽이다(1a2 → 3).
  - 손절값(`Amount`)은 통화의 최소 단위다: 국내는 원, 미국은 센트(÷100).
- 주문은 나갔는데 스냅샷 저장만 실패하는 경우의 시험: 다른 연결이 저장소의 쓰기를 잠근 동안 파는 주문을 낸다.
- 하지 않는 것: 점검 통과 표시를 매매 번호에 묶기(paper · rules), 사는 수량 대조(store의 Trade에 수량 칸), 시장가 증표(결정 7번으로 승인 사기는 지정가만), 이상 감지 뒤 팀 상태(부르는 쪽).

### 3차 검토 참고 (다음 차수 후보)
- 파는 수량 검사가 같은 매매의 아직 체결되지 않은 열린 매도를 남은 수량에서 빼지 않는다(지금은 rules 중복 점검이 막는다).
- 장부 없는 매매가 한 종목에 둘 이상이면 각자 남은 보유 전부를 상한으로 받는다(2차 이전 문서에서만). 섞인 경우의 시험이 없다.
- 증권사 보유를 찾을 때 종목 글자만 보고 시장은 보지 않는다(`track.rs:126`).

## 새 store API 쓰기 (store 12차에서 넘어옴)
- 점검 통과 표시: `submit` · `sell` · `retry_sell`에서 `pi_rules::issue_for_trade`로 낸 표시를 받고 `permit.trade() == Some(trade_id.0)`이 아니면(묶이지 않은 `None` 포함) PermitMismatch.
- 사는 수량을 `trade_quantity`와 대조한다(승인 내용 · 주문 수량).

## 4차 작업 (후보 일괄 처리 · 새 store API)

- 점검 통과 표시: `submit` · `sell` · `retry_sell`은 `permit.trade()`가 다른 매매 번호면 PermitMismatch(주문 · 저장 없음). 묶이지 않은 표시(`None`, `pi_rules::issue`)는 아직 받는다: 잠긴 시험(`common.rs`의 `permit`)과 server(`trading/order.rs`의 `pi_rules::issue`)가 모두 묶이지 않은 표시를 쓰기 때문이다. server가 `issue_for_trade`로 옮기고 잠긴 시험의 도우미를 묶인 표시로 바로잡은(amend) 뒤 `None`도 막는다.
- 사는 수량: 저장소에 매매 수량(`trade_quantity`)이 있으면 표시의 수량이 그것과 같아야 한다(아니면 PermitMismatch). 승인 내용은 이미 표시와 대조하므로 승인 수량도 함께 맞는다. 매매 수량이 없으면(2차 이전 문서 · 아직 두지 않는 server) 대조하지 않는다.
- 시험: `tests/suite/tied.rs`.

## 5차 작업 (server가 issue_for_trade로 옮긴 뒤)
- 묶이지 않은 점검 표시(`permit.trade() == None`)도 PermitMismatch로 막는다(`fits_trade`의 `is_none_or`를 `== Some`으로). 시험 도우미 `tests/suite/common.rs`의 permit · buy · sell을 `approve_for_trade(trade_id.0, ..)`로 바꾸고 부르는 곳에 매매 번호를 넘긴다.

### 5차 구현 메모
- `fits_trade`는 `permit.trade() == Some(매매 번호)`만 받는다. 묶이지 않은 표시는 submit · sell · retry_sell 모두 PermitMismatch. 시험 도우미 `permit(trade_id, ..)`는 묶인 표시, `unbound_permit`은 옛 길.

## 6차 작업 (2026-11-21 답 없는 사기를 줄여 내기 — server 10a 에서 넘어옴)

근거: `spec.md` FN2 3b3, `dev.md` E22, `docs/dev/decisions.md` 2026-11-16 5번(알림이 갔는데 답이 없으면 줄여서 다시 점검해 낸다, 알림이 가지 않았으면 사지 않는다 P20).
왜: server 는 `expire_due` → `unhandled_due` → `pi_rules::reduce_within_limit`(승인 표시 없는 `Passed`) → 실행 → `record_due_handling(Reduced)` 로 잇는데, `submit`은 승인대기 매매를 승인 소비 결과(승인됨 · 소비됨 · 수량 같음)가 있어야만 받는다. 답 없음으로 닫힌 요청은 승인됨도 소비됨도 아니고 수량도 줄어서, 줄인 사는 주문을 낼 길이 없다(server 잠긴 시험 `crates/server/tests/round17/due.rs`의 `알림이_간_사기에_답이_없으면_한도_안으로_줄여_내고_한_번만_기록한다`).

### 원본 동작 근거

- 원본에 없음(우리 고유) — 스펙 FN2 3b3 · E22. 모의계좌와 주문 실행이 원본에 없다(위 "원본 동작 근거").
- 따르는 조사 문서의 규칙: `docs/research/remote-access-security.md` 5.4(답 없음 규칙과의 관계) · RS10 · P19 · P20 — 승인 정본은 서버 레코드뿐이다(부르는 쪽의 말을 믿지 않고 저장소 기록과 대조한다). 확인 실패 · 거절을 "답 없음"으로 바꾸지 않는다. 알림이 가지 않은 답 없음은 사지 않는다.
- persist-before-execute(`docs/research/agent-runtime-deep.md` 1장)는 그대로: 주문됨으로 저장한 뒤에만 `place_order`.

### 공개 API (더하기만)

- `Submission` · `ApprovalGrant` · `Executor::submit`은 그대로 둔다. 잠긴 시험(`tests/suite/common.rs`의 `buy`, `submit.rs` · `tied.rs` · `bind.rs` · `restart.rs`)과 server(`trading/order.rs`)가 `Submission`을 다섯 칸 구조체 글자로 만들기 때문에, 칸을 더하거나 `approval`의 타입을 바꾸면 깨진다. `ApprovalGrant`는 `pi-rules`의 것이라 이 모듈이 갈래를 더할 수도 없다.
- 새 길 하나를 더한다: `Executor::submit_reduced(ReducedSubmission) -> Result<OrderId, Error>`.
  - `ReducedSubmission { trade_id: TradeId, permit: CheckPermit, template_account: AccountKind, unanswered: pi_approval::ApprovalNumber, timeout: UnfilledTimeout }`. `unanswered`는 답 없음으로 닫힌 승인 요청의 번호다. 증표(`ApprovalGrant`)는 받지 않는다(승인이 없으므로).
  - `lib.rs`에서 `ReducedSubmission`을 다시 내보낸다.
- `Error`(이미 `#[non_exhaustive]`)에 두 갈래를 더한다.
  - `NotAwaitingApproval(TradeState)`: 답 없음 줄임 길은 승인대기인 매매만 받는다. 지금 상태를 담는다(`NotHolding`과 같은 모양).
  - `UnansweredMismatch`: 그 번호가 이 매매의 "답 없음 · 줄여서 낼 것"이 아니다(아래 대조 2 · 3 가운데 하나라도 어긋남). `ApprovalMismatch`와 같은 자리의 오류다.
- 옛 길은 바뀌지 않는다: 승인대기 매매에 `submit`을 `approval: None`으로 부르면 여전히 `ApprovalRequired`, 답 없음 번호로 만든 `ApprovalGrant`를 `submit`에 주면 여전히 `ApprovalMismatch`(승인됨 · 소비됨이 아니므로).

### 실행기가 스스로 대조하는 것 (server 를 믿지 않는다)

순서대로 본다. 어긋나면 매매는 그대로이고, 저장도 주문도 하지 않는다.

1. 준비됨(E16) → `NotReady`. 계좌 종류(E25)는 `submit`과 똑같이 템플릿 · 매매 · 표시의 계좌 종류를 증권사와 대조 → `AccountKindMismatch`(실전 계좌는 언제나 막힌다).
2. 점검 통과 표시(E33): 종목이 매매의 종목과 같고, 방향이 **사기**이고, `permit.trade() == Some(매매 번호)` → 아니면 `PermitMismatch`(파는 표시 · 묶이지 않은 표시 · 다른 매매의 표시 포함).
3. 매매 상태가 승인대기 → 아니면 `NotAwaitingApproval(상태)`. 판단됨 매매는 이 길로 받지 않는다(그것은 `submit`의 일).
4. 승인 기록 대조(실행기의 `Approvals` 읽기만 쓴다 — `get` · `unhandled_due`. 읽을 때마다 저장소에서 다시 읽으므로 server 의 관리자가 남긴 알림 · 처리 기록이 보인다) → 하나라도 아니면 `UnansweredMismatch`. 저장소 오류는 `Error::Store`.
   - `get(번호)`: 기록이 있고, 결과가 **답 없음**(`ApprovalStatus::NoAnswer`)이고, 소비되지 않았다(`consumed == false`). 기다림 · 승인됨 · 거절됨(지문 달라 무효가 된 것 포함) · 만료는 받지 않는다.
   - `unhandled_due()`에 그 번호가 있고 할 일이 `DueAction::ReduceWithinLimit`다. 이 한 줄이 다음을 함께 본다: 사는 요청이다 / 알림이 갔다(`Notice::Sent`. 알림 실패 · 기록 없음은 `DoNotBuy`라 받지 않는다, P20) / 신호 유효 시간이 지나지 않았다(지났으면 `Dropped`) / **처리 기록이 아직 없다**(있으면 목록에 없다).
   - 대상이 주문(`Subject::Order`)이고, 신호 id 가 `trade-<매매 번호>`이고, 종목 · 계좌가 매매와 같고, 방향이 사기이고, 호가 유형이 표시와 같다. 값은 대조하지 않는다(rules 가 지금 시세로 다시 셈한 지정가라 요청 때와 다를 수 있다).
   - 수량: `0 < 표시 수량 ≤ 요청 수량(subject.quantity)`. `approval`의 `Reduced { quantity }` 규칙과 같은 범위다.
5. 저장소의 매매 수량(`trade_quantity`)이 있으면 표시 수량이 그것 **이하**여야 한다(`submit`은 같음, 이 길은 이하) → 아니면 `PermitMismatch`. server 는 내기 전에 매매 수량을 줄인 수량으로 바꿔 두므로(`keep_quantity`) 보통은 같다. 실행기는 매매 수량을 고쳐 쓰지 않는다.
6. 매매를 주문됨으로 **저장** → `place_order`. 큰 금액 확인 표시는 **붙이지 않는다**(`confirm_high_value = false`. `submit`은 증표가 있을 때만 붙인다). 그래서 승인이 필요한 금액의 주문은 두 겹으로 막힌다: 표시는 rules 의 권한으로만 나오고(`reduce_within_limit`의 `Passed`는 한도 이하, E33), 그래도 큰 금액 확인 기준 이상이면 증권사가 `ConfirmHighValueRequired`로 거부한다. 증권사 거부 뒤의 처리는 `submit`과 같다: 매매를 막힘(미체결 취소, 이유 남김)으로 옮기고 받지 않음 · 응답 없음은 `AnomalyStop`, 그 밖은 `Rejected`.

한 번만: 같은 승인 번호로 두 번째 부르면 내지 않는다. 지키는 것은 둘이다.
- 매매 상태 전이(실행기 몫): 첫 호출이 승인대기 → 주문됨으로 저장하므로(증권사가 거부했으면 막힘) 두 번째는 3번에서 `NotAwaitingApproval`. 신호 id 가 매매 하나에 묶여 있어 같은 번호를 다른 매매에 쓸 수도 없다(4번).
- approval 의 처리 기록(부르는 쪽 몫): server 가 `record_due_handling`을 남긴 뒤에는 4번의 `unhandled_due`에 없어 `UnansweredMismatch`. 실행기는 처리 기록을 쓰지 않는다(approval 을 읽기만 한다).

### 매매 상태 (store)

- 승인대기 → 주문됨은 저장소 허용 표에 이미 있다(`crates/store/src/trade.rs`의 `allowed`: `(AwaitingApproval, Ordered)`). 승인 소비 길(`submit` + 증표)이 지금 쓰는 그 전이다. **store 차수는 필요 없다.**
- 승인대기 매매의 수량 바꾸기(`set_trade_quantity`)도 승인이 붙지 않은 승인대기면 이미 된다(`plan.rs`). server 는 매매에 승인 번호를 붙이지 않는다.
- 주문됨으로 가는 전이의 근거 칸 검사(`RationaleIncomplete`)는 그대로 `Error::Store`.

### 테스트로 확인할 것 (`tests/suite/reduced.rs` 새 파일)

옛 시험과 도우미는 고치지 않고 더하기만 한다. 승인 요청 · 알림 기록 · 시간 넘기기 · `expire_due`는 `pi-approval`의 공개 API 로 한다(`common.rs`에 도우미를 **더한다**: 예 `unanswered(trade_id, 수량, 값, 알림)`).

- 정상: 요청 10주, 알림 감, 답 없음으로 닫힘 → 4주 표시로 `submit_reduced` → 주문 하나(4주, 확인 표시 없음), 매매는 주문됨, 승인 기록은 답 없음 · 소비 안 됨 그대로.
- 수량 경계: 요청 수량과 같음(10) → 낸다 / 요청 수량 + 1(11) → `UnansweredMismatch` / 0 → `UnansweredMismatch`. 거부된 것은 승인대기 그대로 · 주문 없음.
- 두 번 내기: 같은 번호 · 같은 매매로 다시 → `NotAwaitingApproval(Ordered)`, 주문은 하나뿐. 처리 기록(`record_due_handling(Reduced)`)을 남긴 번호 → 승인대기인 매매라도 `UnansweredMismatch`.
- 파는 주문: 파는 표시 → `PermitMismatch`. 파는 요청의 답 없음 번호(`TeamDecides`) → `UnansweredMismatch`.
- 다른 매매의 승인 번호(신호 id 가 다른 매매) → `UnansweredMismatch`. 다른 매매에 묶인 표시 · 묶이지 않은 표시 → `PermitMismatch`.
- 요청의 결과: 기다림(아직 기다릴 시간 안) · 승인됨(소비 전) · 승인됨 + 소비됨 · 거절됨 · 만료(신호 유효 시간 지남) · 없는 번호 → 모두 `UnansweredMismatch`.
- 알림: 알림 실패(`Notice::Failed`) · 알림 기록 없음 → `UnansweredMismatch`(P20).
- 종목이 다른 요청 · 호가 유형이 다른 표시 → `UnansweredMismatch`.
- 매매 상태: 판단됨 매매에 `submit_reduced` → `NotAwaitingApproval(Judged)`.
- 매매 수량: 저장소 매매 수량보다 큰 표시 → `PermitMismatch`. 매매 수량 10 · 표시 4 → 낸다.
- 계좌(E25): 템플릿 계좌 종류가 실전 → `AccountKindMismatch`, 매매 그대로 · 주문 없음.
- 준비 전(E16) → `NotReady`.
- 큰 금액: 큰 금액 확인 기준 이상의 줄인 주문 → 증권사가 거부해 `Rejected`, 매매는 막힘(미체결 취소). 지금 시험대의 모의계좌는 기준이 없으므로(`common.rs`의 `high_value_confirm_from: None`) 기준을 둔 시험대를 여는 도우미를 더한다.
- 증권사 장애(E13) → `AnomalyStop`, 매매는 막힘(미체결 취소).
- 옛 길 그대로: 답 없음 번호로 만든 증표를 `submit`에 주면 `ApprovalMismatch`.

### 잠긴 옛 테스트와 부딪히는 곳

- 없다(더하기만 하면). `submit.rs`의 `awaiting_approval_without_grant_is_refused`(`ApprovalRequired`)와 `bind.rs`의 `ApprovalMismatch` 시험들은 `submit`을 보므로 그대로 지난다.
- 부딪히게 되는 모양(고르지 않는다): `Submission`에 칸 더하기, `approval`의 타입 바꾸기, `submit`이 승인대기 + `None`을 받게 하기.
- `common.rs`는 도우미를 더하기만 한다. 있는 도우미(`buy` · `grant` · `permit`)의 모양을 바꾸지 않는다.

### 필수 번호

- `E22`를 더한다. 위 "## 필수 번호" 줄은 N1 의 red 커밋과 함께 고친다(번호만 먼저 넣으면 G1 이 실패한다). 새 시험은 `// 검증: E22`와 함께 닿는 번호(C10 · E25 · E33 · E16 · E13)를 적는다.

### 하지 않는 것

- 줄이는 셈 · 금액 한도 판정(rules 의 `reduce_within_limit`), 답 없음으로 닫기 · 처리 기록 남기기(approval · server), 알림(server).
- 승인 기록을 쓰는 일. 실행기는 approval 을 읽기만 한다.
- server 가 넘겨받을 것(server 10a): 사는 주문이고 답 없음 줄임이면 `submit` 대신 `submit_reduced`를 부른다. 주문은 나갔는데 처리 기록을 남기기 전에 꺼지면, 다시 켠 뒤 `unhandled_due`가 그 번호를 또 준다 — 이때 매매가 이미 주문됨 · 보유중이면 막힘으로 닫지 말고 `Reduced`(매매 수량)로 기록만 남겨야 한다(실행기는 `NotAwaitingApproval`로 다시 내기를 막을 뿐이다).

### 6차 조정자 결정 (초안의 "물을 것" — 가장 보수적인 쪽)
1. 모양은 새 메서드 `submit_reduced`로 한다(잠긴 시험이 `Submission`을 구조체 글자로 만든다).
2. **금액 상한을 하나 더 둔다**: 줄인 주문의 `수량 × 지정가`가 요청 때의 `요청 수량 × 요청 값`을 넘으면 내지 않는다(`UnansweredMismatch`). 한도 판정은 rules가 하지만, 답 없는 주문이 사용자가 본 요청보다 커지는 길은 실행기에서도 막는다.
3. 저장소 매매 수량과의 대조는 "이하"로 둔다.
4. 알림 기록(`Notice::Sent`)이 없으면 내지 않는다(P20). server가 `record_notice`를 빠뜨리면 줄인 주문이 안 나가는 것이 의도다.
5. 재시작 틈(주문은 나갔는데 처리 기록 전에 꺼짐)은 server 10a에서 고친다: 다시 켠 뒤 `unhandled_due`가 준 요청의 매매가 이미 주문됨 · 보유중이면 막힘으로 닫지 않고 `Reduced`(그 매매의 수량)로 기록만 남긴다.

### 6차 검토 참고 (다음 차수 후보)
- 실행기는 "승인이 필요한 금액"을 스스로 막지 못한다(요청 크기 이하로만 묶는다). `CheckPermit`에 "승인 증표로 낸 표시인지"가 남지 않아, 조립하는 쪽이 `rules::approved`의 표시(요청 전량)를 `submit_reduced`에 넘기면 통과한다. paper · rules 차수에서 표시에 그 표를 남기고 `submit_reduced`가 거부하게 한다. 그때까지 server는 `submit_reduced`에 `reduce_within_limit`의 표시만 넘긴다(server 검토에서 확인).
- `within_request`: 지정가가 0 이하면 거부(지금은 증권사가 거부해 매매가 막힘으로 간다).
- 같은 매매에 `submit_reduced` 둘을 동시에 불러 주문이 하나뿐임을 보는 시험. 동시에 진 쪽은 `Error::Store(TransitionDenied)`다(지시서의 `NotAwaitingApproval`과 다름 — 주문은 나가지 않는다).

## 7차 작업 (2026-11-21 팀장 팔기 목적 — server 10a 검토에서 넘어옴)

server가 팀장 팔기(`pi_rules::Purpose::TeamExit`)를 실행기에 청산(`SellPurpose::Liquidation`)으로 넘겨, 같은 종목의 열린 파는 주문이 모두 취소된다(같은 종목 보유가 둘이면 뒤 보유의 매도가 앞 보유의 매도를 취소해 마지막 보유만 팔린다). 결정 2026-11-21 13번("팀장 팔기는 같은 종목 보유를 모두")과 어긋난다.

- **공개 API (더하기만)**: `SellPurpose::TeamExit` 갈래를 더한다. 뜻: 팀장 판단 매도. **내기 전에 다른 열린 파는 주문을 취소하지 않는다**(익절과 같다). 기록 · 이벤트의 목적 글자는 `team_exit`. 손절 · 청산의 "먼저 취소"는 그대로다(그때 이 매도도 함께 취소된다 — 손절이 먼저다).
- 실패했을 때(받지 않음 · 응답 없음)의 처리 · 다시 내기(`retry_sell`) · `pending` 다루기는 익절과 같은 길을 탄다. 손절 감시(`stop_hits`)는 `pending`에 팀장 팔기가 남아 있어도 그 매매의 손절을 건너뛰지 않는다(건너뛰는 것은 손절 · 청산이 남아 있을 때뿐 — 지금 코드가 무엇을 건너뛰는지 확인하고, 팀장 팔기 · 익절이 남아 손절이 막히는 길이 있으면 고친다).
- 원본 동작 근거: 원본에 없음 — 스펙 F4("팀장이 팔기로 정하거나"), 결정 2026-11-21 13번.
- 테스트로 확인할 것: 같은 종목 보유 둘에 `TeamExit` 매도를 잇달아 내면 열린 매도가 둘 다 남는다 / `TeamExit` 뒤 같은 종목 손절은 열린 파는 주문을 모두 취소하고 나간다 / 실패한 `TeamExit`가 `pending`에 남아도 그 매매의 손절값에 닿으면 손절이 나간다 / 기록의 목적 글자 / 옛 세 갈래의 동작 그대로.
- 잠긴 옛 테스트와 부딪히는 곳: `SellPurpose`를 빠짐없이 `match`하는 곳이 테스트 · server에 있으면 적고(server의 `sell_purpose`는 `_ =>` 갈래가 있다), 깨지면 멈춘다.

### 7차 검토 참고 (다음 차수 후보 — 돈을 지키는 쪽이라 server 10b 앞에 연다)
- **실행기 스스로 막기**: `pending`은 매매당 하나라, 손절 · 청산이 다시 내기를 기다리는 매매에 익절 · 팀장 팔기가 실패하면 손절 기록을 덮어쓰고(횟수도 1로), 성공하면 지운다. 지금은 server의 `open_sell` 한 곳이 막고 있고 그 확인과 `sell` 사이에 틈이 있다. 손절 · 청산이 기다리는 매매에는 익절 · 팀장 팔기를 거부한다.
- **매매별 수량 상한**: `sell`이 그 매매의 남은 수량에서 **열린 매도 수량까지** 빼고 대조한다(같은 종목 보유가 둘이면 한 매매에 익절 10 + 팀장 팔기 10이 둘 다 열려 다른 매매 몫까지 팔릴 수 있다 — 모의계좌는 종목 단위로만 막는다).
- `team_exit`이 기다리는 스냅샷은 옛 판 실행 파일이 열지 못한다(닫히는 쪽으로 실패). 운영 문서에 한 줄.

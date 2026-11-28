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

C10 E12 E13 E16 E25 E29 E30 E33 V6 E22 E34

## 1차 검토 참고 (다음 차수 후보)
- 끝남: submit은 ApprovalGrant가 있는지만 본다. 저장소의 승인 번호 · 지문과 대조하거나, 증표를 approval 모듈만 만들 수 있게 막는다. — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/exec.md`

## 2차 작업 (1차 검토 참고에서)
- 끝남: 승인 증표: submit은 받은 ApprovalGrant가 그 매매의 승인인지(매매 번호 · 내용 지문) 확인한다. 증표를 approval 모듈만 만들 수 있게… — 커밋 8eea431(red) · 1cd0a45(green) · 전문: `git show 1a73576:docs/dev/modules/exec.md`

## 3차 작업 (후보 일괄 처리)
- 끝남: 보유 장부를 스냅샷의 실행기 몫에 남기고 켤 때 되살린다. 맞춰 보기는 장부를 지우지 않고 그 위에 증권사 주문을 더한다. 그래서 모의계좌가 끝난 주문을 덜어 … — 커밋 c30a403(red) · 094efc3(green) · 전문: `git show 1a73576:docs/dev/modules/exec.md`

## 새 store API 쓰기 (store 12차에서 넘어옴)
- 점검 통과 표시: `submit` · `sell` · `retry_sell`에서 `pi_rules::issue_for_trade`로 낸 표시를 받고 `permit.trade() == Some(trade_id.0)`이 아니면(묶이지 않은 `None` 포함) PermitMismatch.
- 사는 수량을 `trade_quantity`와 대조한다(승인 내용 · 주문 수량).

## 4차 작업 (후보 일괄 처리 · 새 store API)
- 끝남: 점검 통과 표시: submit · sell · retry_sell은 permit.trade()가 다른 매매 번호면 PermitMismatch(주문 · 저장 없음… — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/exec.md`

## 5차 작업 (server가 issue_for_trade로 옮긴 뒤)
- 끝남: 묶이지 않은 점검 표시(permit.trade() == None)도 PermitMismatch로 막는다(fits_trade의 is_none_or를 == Some… — 커밋 e6179cd(red) · ff8f7ff(green) · 전문: `git show 1a73576:docs/dev/modules/exec.md`

## 6차 작업 (2026-11-21 답 없는 사기를 줄여 내기 — server 10a 에서 넘어옴)
- 끝남: 근거: spec.md FN2 3b3, dev.md E22, docs/dev/decisions.md 2026-11-16 5번(알림이 갔는데 답이 없으면 줄여서 다… — 커밋 e31ac46(red) · 6a038a9(green) · 전문: `git show 1a73576:docs/dev/modules/exec.md`

## 7차 작업 (2026-11-21 팀장 팔기 목적 — server 10a 검토에서 넘어옴)
- 끝남: server가 팀장 팔기(pi_rules::Purpose::TeamExit)를 실행기에 청산(SellPurpose::Liquidation)으로 넘겨, 같은 종목… — 커밋 d387f9f(red) · 507d56a(green) · 전문: `git show 1a73576:docs/dev/modules/exec.md`

## 8차 작업 (2026-11-21 파는 주문 지키기 — 7차 검토에서 넘어옴, server 10b 앞)
- 끝남: 7차 검토 참고의 두 틈을 실행기 스스로 막는다(지금은 server의 open_sell 한 곳에 기대고, 그 확인과 sell 사이에 틈이 있다). 공개 API는… — 커밋 3af31bc(red) · b3b31c8(green) · 전문: `git show 1a73576:docs/dev/modules/exec.md`

## 9차 작업 (2026-11-24 조정자, 사용자 "다음 이어서 작업해" · "알아서 추천대로" — 남은 후보, 안전)
- 끝남: server 10b 참고에 남은 실행기 쪽 두 틈을 막는다. 공개 API는 더하기만 한다. — 커밋 ec91cbf(red) · 73e6e83(green) · 전문: `git show 1a73576:docs/dev/modules/exec.md`

## 10차 작업 (2026-11-24 조정자, 결정 14 — 남은 후보, 안전)

1. **답 없음 줄임 길은 승인 표시를 받지 않는다**: `submit_reduced`가 받은 표시가 `granted()`(승인 증표로 낸 것)이면 주문을 내지 않고 거부한다(새 오류 변형 또는 `UnansweredMismatch` — 이름은 N1이 정해 보고). 매매 상태 · 저장은 그대로.
2. **증권사 보유 찾기에 시장도 본다**: `track.rs`가 증권사 보유를 매매와 맞출 때 종목 글자만 보지 말고 시장도 같아야 한다(같은 글자의 다른 시장 종목을 섞지 않는다).
3. **시험 보강**: 같은 매매에 `submit_reduced` 둘을 동시에 부르면 주문은 하나뿐이고 진 쪽은 오류(지금 동작 — `Error::Store(TransitionDenied)`)다 / 한 종목에 장부 없는 매매가 둘 이상일 때의 지금 동작을 잠근다.
4. **운영 문서 한 줄**: `team_exit`이 기다리는 다시 내기 스냅샷은 옛 판 실행 파일이 열지 못한다(판을 되돌릴 때 주의) — `docs/dev/modules/exec.md` 이 절에 적어 둔다(코드 주석에도).

- 원본 동작 근거: 원본에 없음 — 스펙 E22 · E33, 결정 14, exec 2 · 6차 검토 참고.
- 시험으로 확인할 것: 승인 표시로 `submit_reduced` → 거부 · 주문 0건 / 같은 종목 글자 다른 시장 보유를 매매에 맞추지 않음 / 위 3번.
- 운영 주의(4번): 실행기가 저장하는 다시 내기 대기 스냅샷(팀장 팔기 포함)은 이 판 이후 모양이라, 실행 파일을 옛 판으로 되돌리면 읽지 못한다. 되돌리기 전에 열린 다시 내기를 비운다.

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| `crates/exec/tests/suite/reduced.rs` · `reduced/` | 유지 | 줄임 길은 `reduce_within_limit` 표시(granted 거짓)만 넘긴다 — 승인 표시 거부는 새 시험 |
| `crates/exec/tests/suite/ledger.rs` · `restart.rs` · `resume.rs` | 유지 | 보유 맞추기는 같은 시장 사례뿐 — 다른 시장 섞임은 새 시험 |
| 그 밖의 exec 시험 | 유지 | 10차와 무관 |
- 10차 걸음 메모(조정자): 저장소 매매에 시장 칸이 없어 장부 없는 매매의 시장은 종목 글자 모양(숫자 여섯 자리 = 국내, 그 밖 = 미국)으로 정했다. 다음 후보: store 매매에 시장 칸을 두면 이 추정을 버린다.

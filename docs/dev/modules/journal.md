# journal 모듈 (T12, T15, 하루 정리)

크레이트 `pi-journal`, 폴더 `crates/journal/`. 구성 요소 A4 복기 · M4 · A3 하루 정리 내용. `pi-store`, `pi-template`를 쓴다. 순수 계산과 저장소 호출만 한다.
근거: `dev.md` C6 · M2 · M4 · E17 · E28 · E35 · V12 · V16 · V18~V21 · V50, `spec.md` F5 · F7, `principles.md` P15.

## 원본 동작 근거

- 복기를 기억으로 남기기와 압축 전 저장: `docs/research/agent-runtime-deep.md` 6장, `docs/research/hermes-agent.md` 3.6 · 3.4, `docs/research/openclaw.md` 2.3.
- 하루 보고: `docs/research/office-deep.md` 7-3.
- 쉬운 말 근거 요약, 실수 세기, 규칙 개선안의 냉각은 원본에 없음(우리 고유). 근거: `principles.md` P15, `spec.md` F5.

## 만드는 것 (1차)

1. `plain_summary(trade) -> String` (C6 · V18): 근거 칸 · 담당 점수 · 자료 시각으로 만든 고정 문장 틀. AI를 쓰지 않는다(결정 2026-11-16 28번).
2. 근거가 다 있는지 판정(V16).
3. 복기 기록과 실수 종류(정한 목록 + 자유 글). 팔린 매매를 장 마감 뒤 복기됨으로 바꾼다(V19). 못 끝낸 매매는 다음 마감으로 넘긴다(E17).
4. 같은 실수가 2건이면 개선안(RiskParams 차이)을 낸다(V20).
   - 한도나 손절을 넓히는 변경이면 냉각중으로 두고 다음 거래일 시작에 반영한다(E28).
   - 승인 번호가 없으면 반영하지 않는다(E35).
   - 거절하면 그대로다(V21).
   - 반영해도 상한을 넘지 않는다(pi-template로 확인).
5. 하루 정리 내용(V12): 한 일, 손익, 복기, 마지막 백업 완료 시각(V50).

## 승격 판정

승격 기준 판정은 `pi_template::RiskParams::evaluate_promotion`으로만 부른다(`promotion.evaluate`를 직접 부르면 낡은 낙폭 기준을 쓸 수 있다).

## 하지 않는 것

AI로 복기 글 쓰기(agent의 복기 담당), 알림 보내기(notify), 화면.

## 필수 번호

C6 M4 E17 E28 E35 V12 V16 V18 V19 V20 V21 V50

## 2차 작업: 위험 증가 판정 빈틈 (1차 검토 되돌림)
- 끝남: increases_risk는 칸이 비는 쪽 전이도 센다: 확신 비중 None→Some(상한이 생겨도 종목 비중보다 넓어지면 증가), 고정 손절 Some→None… — 커밋 8ca60ac(red) · ed84586(green) · 전문: `git show 1a73576:docs/dev/modules/journal.md`

## 3차 작업 (후보 일괄 처리)
- 끝남: increases_risk: 연속 손실 정지의 손실 횟수 증가 · 쉬는 기간 단축(그날만 < 거래일 n일), ATR 손절 Some→None도 위험 증가로 센다. — 커밋 cac420e(red) · a3f3338(green) · 전문: `git show 1a73576:docs/dev/modules/journal.md`

## 새 store API 쓰기 (store 12차에서 넘어옴)
- `approve_as_risky` 우회를 걷어내고 `mark_proposal_risky`로 위험 표시를 올린다. 하루 정리는 `trade_state_changes_between`으로 그날의 중간 상태 바뀜을 본다.

## 4차 작업 (후보 일괄 처리: 새 store API)
- 끝남: approve: 제안 때는 안전했지만 지금 규칙보다 위험해진 개선안은 mark_proposal_risky로 같은 개선안에 위험 표시를 올린 뒤 승인한다(냉각중)… — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/journal.md`

## 5차 작업 (2026-11-24 조정자, 남은 후보 — 안전, 사용자 "남은 일 한 개도 없이")
- 끝남: 위험 증가 판정을 하나로: increases_risk는 자기 판정 대신 pi_template::increases_risk(template 11차)를 그대로 부른… — 커밋 3123c56(red) · 870e293(green) · 전문: `git show 1a73576:docs/dev/modules/journal.md`

## 6차 작업 (2026-11-25 조정자, 결정 17 — 결정 정산과 교훈("어제 메모"의 재료))
- 끝남: 근거: docs/product/stock-team/additions.md의 R7-4 · M11 · C25 · E54 · N20 · T33(차수 표 22번). 저… — 커밋 cf021ab(red) · f9c6cc1(green) · 전문: `git show 1a73576:docs/dev/modules/journal.md`

## 7차 작업 (2026-11-27 조정자, 결정 30 — 사용자 "실수 기록들이나 복기 회고들은 승격해서 항상 규칙을 개선… 노이즈는 최대한 제거하고 가져갈 규칙 기억들만 쌓는다" · "매매가 잘 되는 선에서 익절 확률이 높은 쪽으로")
- 끝남: 동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지… — 커밋 477f4a9(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/journal.md`

## 8차 작업 (2026-11-27 조정자, 결정 30 · 백로그 T — 사용자 "백로그 다 실행해" · "남은 것들만 다 실행해 key 제외하고")

동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지 보고 잠근다.

1. **1층 후보 → 개선안**: 걸러진 지침 후보의 글에서 RiskParams 칸 하나와 바꿀 쪽(목표 값 하나, 또는 늘림 · 줄임 한쪽 — 값이 없으면 지금 값의 10%)이 확실히 읽힐 때만 개선안을 만든다(`guideline_rules.rs`). 읽히지 않으면 지금처럼 이벤트만.
2. **기록된 거래일로 날 수 세기**(`guideline_days.rs`, 장부 새 칸 `open_buys`).

- 원본 동작 근거: 결정 30(읽기 기준 · 10% 걸음은 조정자 해석).
- 시험으로 확인할 것: `tests/suite/guideline_layer.rs`가 합친 구현에서 통과하고, 기존 시험도 모두 통과한다.

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| 새 시험 `tests/suite/guideline_layer.rs` | 유지 | 작업 폴더에서 red 커밋으로 먼저 쓰고 green 커밋으로 통과 |
| `tests/suite/main.rs` | 고침 | 새 시험 모듈을 잇는 줄 · 문서 한 줄만 더함(단언은 그대로) |
| 그 밖의 journal 시험 | 유지 | 동작이 바뀌지 않는다 |

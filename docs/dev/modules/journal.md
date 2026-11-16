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

- `increases_risk`는 칸이 비는 쪽 전이도 센다: 확신 비중 None→Some(상한이 생겨도 종목 비중보다 넓어지면 증가), 고정 손절 Some→None(손절이 사라짐), 연속 손실 정지 Some→None(해제), ATR 배수 증가, 최소 현금 감소, 고정 손절 아래 · 위 끝이 넓어짐. 스펙 "한도를 넓히거나 손절을 넓히는 변경"(spec.md 50행 근처)을 칸마다 표로 시험한다.
- `approve`는 승인 때 받은 `current`와 개선안의 `after`로 **다시** 위험 증가를 판정한다(제안 때의 `before`만 믿지 않는다). 다른 개선안이 먼저 반영돼 `after`가 지금보다 위험하면 냉각으로 간다.
- 1차 참고(되돌림 아님): `apply_cooled`의 `_current` 미사용, `daily_summary`가 `updated_at` 기준이라 그날 지나 갱신된 매매가 빠질 수 있음 — 다음 차수 후보.

### 2차 검토 참고 (다음 차수 후보)

- `approve_as_risky`가 승인 성공 뒤 원래 개선안 거절에 실패하면 원래 것이 제안됨으로 남는다(다시 승인돼도 냉각으로 감). store에 "위험 표시 갱신" API가 생기면 이 우회를 걷어낸다.
- `checked_improvement` 문서 주석이 `approve_as_risky` 위로 어긋나 있다.
- 연속 손실 정지 횟수 증가 · 쉬는 기간 단축, ATR Some→None도 위험 증가로 셀지 정한다.

## 3차 작업 (후보 일괄 처리)

- `increases_risk`: 연속 손실 정지의 손실 횟수 증가 · 쉬는 기간 단축(그날만 < 거래일 n일), ATR 손절 Some→None도 위험 증가로 센다.
- `daily_summary`의 한 일: 만든 · 판단한 · 마지막으로 바뀐 시각 가운데 하나가 그날이면 남긴다(다음 날 복기돼도 그날 판 매매가 빠지지 않음). 중간 상태 바뀜 시각은 store에 없어 못 본다.
- `checked_improvement` 문서 주석 자리 바로잡음. `apply_cooled`의 `current`는 쓰지 않는 까닭을 문서로 남김(잠긴 테스트가 `after` 통째 반영 뜻).
- 남김(store 몫): 위험 표시 갱신 API가 생기면 `approve_as_risky` 우회 걷어내기, 매매 상태 바뀜 이력.

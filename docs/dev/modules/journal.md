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

## 하지 않는 것

AI로 복기 글 쓰기(agent의 복기 담당), 알림 보내기(notify), 화면.

## 필수 번호

C6 M4 E17 E28 E35 V12 V16 V18 V19 V20 V21 V50

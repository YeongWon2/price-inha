# agent 모듈 (T9)

크레이트 `pi-agent`, 폴더 `crates/agent/`. 구성 요소 A4. `pi-ai`(AiProvider)와 `pi-store`(10차의 세션 · 메시지 · 방 로그, 매매)를 쓴다. 도구(자료 · 지식 · 주문 안 · 승인)는 `Tools` 포트로 받는다. `pi-secrets`에는 기대지 않는다(P21).
근거: `dev.md` M2 · E6 · E20 · E27 · E32 · E38 · V2~V4, `spec.md` F2 · F3, `principles.md` P7 · P9 · P21.

## 원본 동작 근거

- 한 턴의 고리(interrupt → 예산 → 준비 → 호출 → 도구 · 끝), persist-before-execute, 취소 결과 짝 맞추기, 오류 상한(이름 3 · JSON 3 · 빈 응답 3 · 잘림 4 · 바깥 8), 무활동 감시: `docs/research/agent-runtime-deep.md` 1장.
- 시스템 프롬프트 세 층(공통 · 담당 SOUL · 팀 규칙): `docs/research/agent-runtime-deep.md` 2장, `docs/research/deskrpg.md` 4-5.
- 팀 논의 방(0라운드는 지명된 담당, 그 뒤는 멘션된 담당. 상한 3라운드 · 10글 · 6명): `docs/research/agent-runtime-deep.md` 7.1~7.6, `docs/research/hermes-agent.md` 6.5.
- 겹친 입력(steer · followup, 담당마다 대기 8): `docs/research/agent-runtime-deep.md` 9.1. 공급자 오류 분류: 같은 문서 4장.
- 의도해서 다름: 세션 lease 대신 한 프로세스 안의 담당별 잠금(D1). 도구 호출은 글 안의 JSON 행동 형식으로 API 키 · CLI 백엔드가 같게 한다(결정 2026-11-16 12번, 사용자 결정). 점수 턴과 합의 점수는 원본에 없다(결정 15번).

## 만드는 것 (1차)

1. `Role` 6개(뉴스 공시 시세 · 전략 · 리스크 · 복기 · 매매 · 팀장)와 담당별 설정(모델, 반복 상한 20, 시간 제한: 장중 담당 턴 무응답 60초 · 전체 180초, 채팅 180 · 600초 — 결정 16번).
2. 턴 실행기
   - 답 글 안의 JSON 행동을 해석하고 `Tools` 포트로 보낸다.
   - 도구를 부르기 전에 메시지를 먼저 저장한다.
   - 상한에 닿거나 시간이 지나면 E6(담당 응답 실패)으로 끝낸다.
3. 팀 논의 방과 끝 조건(결정 14번). 논의가 끝나면 다음 순서로 간다.
   1. 담당마다 점수 턴: 정해진 JSON(점수 · 근거), 방 로그와 따로 저장.
   2. 합의 점수를 템플릿 기준과 비교해, 못 미치면 지켜보기(E27). 담당 하나가 실패하면 합의 불가로 보고 지켜보기.
   3. 팀장 판단 턴: `judge_trade`로 담당 점수와 판단을 남긴다(V3).
   4. 사기로 정했으면 매매 담당이 주문 안을 만든다(V4). 수량 · 가격은 rules가 정한다(P7).
4. 넘기기 기록: 담당이 관련 담당에게 넘긴 것을 방 로그에 남긴다(V2).
5. 필요한 자료가 비면 판단을 지켜보기로 남긴다(E32). 판단됨이 아닌 매매로 주문 안을 내지 않는다(E20).
6. AI로 보내는 글은 `Redactor` 포트를 거친다(E38).

## 9차 작업 (2026-11-21 발표 자료 대조)
- 끝남: 근거: spec.md F2 8b("보유 종목 팔기 흐름으로 넘어간다") · F4 시작("팀장이 팔기로 정하거나") · F4 기본 흐름 1 · 2("매매 담당이 … — 커밋 94b5455(red) · 4182c54(green) · 전문: `git show 1a73576:docs/dev/modules/agent.md`

## 하지 않는 것

주문 내기 · 점검 · 승인(포트 너머), 비밀 읽기, 스트리밍(결정 13번), 컨텍스트 압축(조사가 얇아 2차).

## 테스트 주의

AI는 대본대로 답하는 가짜 AiProvider만 쓴다. 담당 하나가 실패하는 경우, 상한에 닿는 경우, JSON이 깨진 경우를 표로 시험한다.

## 필수 번호

M2 E6 E20 E27 E32 E38 V2 V3 V4 P7 P9 P21

## 1차 검토 참고 (다음 차수 후보)
- 끝남: 방 로그 · 세션에는 가리지 않은 원문이 저장되고 공급자 오류 문구도 turn.failed에 그대로 남는다. T18(기록 가리기)과 맞춰 저장 전에도 redac… — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/agent.md`

## 2차 작업 (1차 검토 참고에서)
- 끝남: draft_order는 판단 방향이 사기일 때만 사기 주문 안을 만든다(팔기면 거부). — 커밋 5359e00(red) · 91d373a(green) · 전문: `git show 1a73576:docs/dev/modules/agent.md`

## 3차 작업 (후보 일괄 처리)
- 끝남: 저장 전 가리기(E38 "기록 … 어디에도"): 세션 메시지(받은 글 · 답 · 도구 결과), 방 로그 내용(글 · 실패 까닭), 판단(근거 · 담당 점수 까닭… — 커밋 9de573a(red) · f8e5995(green) · 전문: `git show 1a73576:docs/dev/modules/agent.md`

## 새 store API 쓰기 (store 12차에서 넘어옴)
- 팔기 매매의 `draft_order`는 `judge_trade_with_side`로 방향을 저장하고 `trade_side`로 막는다(다시 띄운 뒤에도). Team 메모리에만 기대지 않는다.

## 4차 작업 (후보 일괄 처리: 새 store API)
- 끝남: 팀장 판단은 judge_trade_with_side로 근거와 방향을 한 묶음에 남긴다. — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/agent.md`

## 5차 작업 (server 마지막 차수에서 넘어옴)
- 끝남: AI가 내는 판단 금액(손절값 · 목표가 등)은 통화의 최소 단위다: 국내는 원, 미국은 센트. 담당 프롬프트 · 도구 설명에 시장별 단위를 알리고, 받은 값을… — 커밋 3df2ce3(red) · 0bd700d(green) · 전문: `git show 1a73576:docs/dev/modules/agent.md`

## 8차 작업 (후보 일괄 처리)
- 끝남: 방 줄 id에 넣는 방의 마지막 번호를 store room_last_seq(room)로 바로 읽는다(500줄씩 읽어 세던 것을 없앴다). 줄 id 모양(trad… — 커밋 —(red) · 30779f5(green) · 전문: `git show 1a73576:docs/dev/modules/agent.md`

## 10차 작업 (2026-11-25 조정자, 결정 17 — 논의 진행 신호 · 정리 모드 · 팀장 예비분)
- 끝남: 근거: docs/product/stock-team/additions.md의 R1-5 · R5-11 · C21 · M13 · E51 · E52 · N14 · V5… — 커밋 d521bd2(red) · fb59965(green) · 전문: `git show 1a73576:docs/dev/modules/agent.md`

## 11차 작업 (2026-11-25 조정자, 결정 17 — 담당 의견 칸 · 팀장 요약 · 뒤집힐 조건 · 공지를 논의 자료에)
- 끝남: 근거: docs/product/stock-team/additions.md의 R6-4 · R6-5 · R17-2 · M9 · D13 · T29(차수 표 16번).… — 커밋 15c5d12(red) · 1ec6c57(green) · 전문: `git show 1a73576:docs/dev/modules/agent.md`

## 12차 작업 (2026-11-27 조정자, 결정 30 — 사용자 "사람이 정한 규칙들은 강제성이고 AI가 자가 루프 학습")

동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지 보고 잠근다.

1. **팀 지침 블록**: `DiscussionOptions::with_guidelines` — 브리핑의 자료 · 공지 뒤 정해진 자리에 `[팀 지침]` 블록과 "지침은 참고다. 한도 · 금지는 시스템이 따로 막는다" 한 줄. 줄은 가린 뒤 상한 안에서만, 지침이 없으면 블록 없음(새 파일 `guideline.rs`).

- 원본 동작 근거: `docs/research/hermes-agent.md` 3.6 · `docs/research/openclaw.md` 3.6(기억을 프롬프트의 정해진 자리에, 예산을 넘으면 주입본만 자름).
- 시험으로 확인할 것: `tests/suite/guideline.rs`(5개)

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| 새 시험 `tests/suite/guideline.rs`(5개) | 유지 | 기능 작업 폴더에서 red 커밋으로 먼저 쓰고 green 커밋으로 통과 |
| `tests/suite/main.rs` | 고침 | 새 시험 모듈을 잇는 줄만 더함(단언은 그대로) |
| 그 밖의 agent 시험 | 유지 | 동작이 바뀌지 않는다 |

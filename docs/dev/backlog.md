# 남은 일 목록 (2026-11-24 조정자, 사용자 "남은 일 한 개도 없이")

모듈 지시서마다 흩어진 "다음 차수 후보 · 검토 참고"를 한곳에 모은 표다. 처리하면 줄 끝에 `→ 처리: <모듈> <차수>`를 단다.
근거 줄 번호는 모을 때의 것이다.

## 묶음 A — 진행 중

- [x] store 16: 승격대기 템플릿에 사용자 변경 거부 · 승격은 `pass_revision = revision`일 때만 · 판 10 뒤 무결성 검사 (store.md:251)
- [x] template 11: ATR 손절 봉 단위 넓힘(분봉 → 일봉)도 위험 증가 (template.md 11차)
- [x] journal 5: `increases_risk`를 template 것 하나로 (journal.md 5차)

## 묶음 B — 돈 · 안전

- [x] store: 새 표 `proposal_sources` · `user_revisions` · `office_looks` 지우기 막는 트리거(잠긴 `round10_replace.rs` PROTECTED 목록 함께) (store.md:249)
- [x] (결정 15로 닫음) store + exec: 매매(Trade)에 시장 칸 — exec의 종목 글자 모양 추정을 걷어냄 (exec.md:256)
- [x] gate: 세션 확인 때 저장소 세대도 본다(다른 프로세스가 키를 바꾼 경우) (server.md:221)
- [x] (결정 15로 닫음) approval + store: 승인에 대상 종류 · 번호 칸(주문 승인을 다른 대상에 못 쓰게) · 전체 정지 중 승격 막기 (approval.md:31)
- [ ] approval: `unhandled_due`가 닫힌 요청을 매번 다 읽음 · `Blocked.reason` 길이 상한 (server.md:357)
- [ ] server: 자료 이상 멈춤 · 풀림 알림 시간당 묶기, scout 세션 지우기, 본 것 기억 기록 시점, 개선안 답과 하루 마감 승격 판정 겹침 잠금 (server.md:277 · 310)
- [ ] notify: `release_held` 동시 두 번 시험 (notify.md:73)

## 묶음 C — 기능

- [x] (server 22 · 원본 맞춤은 23) server: 팀 논의 글을 사무실에 보인다(R5-2 · 사용 흐름 4번 — 방 로그에만 있고 화면에 안 옴), 머리 위 말풍선(원본 DeskRPG), 지나가며 잡담(원본) (사용자 "서로 대화하는 게 보여야")
- [x] server: 설정 화면에 하루 AI 호출 상한 (server.md:591)
- [ ] server: 담당 모델을 사용자가 고르는 설정 (server.md:66)
- [ ] server + knowledge + store: `hybrid_search` 잇기(store 모델별 조각 벡터 조회 API 먼저) · 시장 메모 색인 · 한가할 때만 재색인 시험 (server.md:345, knowledge.md:85)
- [ ] server 운영: systemd `%` · `$` 이스케이프, Windows 경로 끝 `\` 따옴표, `--bind` · `--port` 환경 변수 안내, macOS 죽는 서비스 알림, 기록 파일 돌려 쓰기, doctor 읽기 전용 손잡이 (server.md:221)
- [ ] server: 설치 스크립트 · 자동 업데이트 "알리기만" (server.md:211)
- [ ] server: 공시 — 고유번호 없는 종목 "받을 수 없음", DART 키 없을 때 하루 수 안 쓰기, scout 답의 부정 문맥 종목 코드 (server.md:316)
- [ ] server: 구독 — 템플릿 구독 매 tick 읽기, 다시 켤 때 쌓인 사건 한꺼번에 깨우기 막기, 해제 직전 대기열 사건 버리기, 값 바뀜 · 멈춤 → 재시작 시험 (server.md:331)
- [ ] conformance: C12 이벤트 로그 용량 상한, A12 자격 풀 (conformance.md:196)
- [ ] ai: 같은 집 다른 기기 Ollama 허용 (ai.md:191, 결정 필요)

## 묶음 D — 정리

- [ ] server: `proposal_notices` 이벤트를 매번 처음부터 읽음 (review.rs:207)
- [ ] server: 사무실 따라가기 카메라 인원 문턱값(회의 3 · 쉼 4) 근거 (server.md:633)
- [ ] server 10c 하 항목: 날짜 조건 근거 `updated_at`, 끄는 순간 재색인 끊기, doctor Ollama 확인 제한, 뜻 검색 도구 시간 제한, snippet "바깥 글" 표시, 원격 설정 POST 403 시험, 루프백 아닌 설정 주소 진단 글, 접수번호 없는 공시 기록 키 중복 (server.md:347)
- [ ] server: 진단 곁 파일 경쟁, 실전 템플릿 주문이 막힐 때 매매에 이유 (server.md:270)
- [ ] server 17차 N3: 장부 없을 때 exec 남은 수량 공개 API, 폰 POST 중 새 승인 카드 시험 (server.md:536)
- [ ] server: V32 거래량 덮기 (server.md:67)
- [ ] server: 하루 AI 상한 저장(ai-limit.json)이 백업에 드는지 확인하고, 안 들면 넣는다 (server 22차 검토 참고)
- [ ] server: 실시간 연결을 다시 이을 때 다시 받은 옛 논의 글이 말풍선으로 잠깐 뜨지 않게 (server 22차 N2 참고)

## 묶음 E — 사무실을 게임 · 메타버스처럼 (2026-11-25 사용자 "진짜 게임처럼" · "실제 메타버스 환경 같은 느낌" · "추천순으로 다 넣어")

레퍼런스는 rea CLI로 뜯는다(조사 문서: `docs/research/` — 에이전트 사무실 · 투자 에이전트 팀 · 메타버스 세 갈래).

- [ ] server 23: 이름표 · 말풍선을 원본 DeskRPG대로, 화면 아래 자막, 회의 발언자 카메라, 채팅 패널 모양
- [ ] server: 시연 모드에서 팀이 실제로 돈다 — 지금 시연용 가짜 증권사는 달력을 정하지 않아(늘 휴장) 깨우기 엔진 첫 tick이 503으로 실패하고 논의가 열리지 않는다(e2e 준비 중 발견)
- [ ] server: 내 캐릭터 + 키보드 이동(WASD · 방향키 · 충돌 · 카메라 따라가기), 담당에게 다가가 말 걸기
- [ ] server: 채팅 멘션(`@` 자동완성, 멘션한 담당만 답), 팀 방 전체 공지(모든 담당에게 전달), 담당 불러서 보고받기
- [ ] server: 승인 기다림 ✋ 말풍선 + 알림음, 종목별 근거 카드(담당별 점수 · 한 줄 이유 · 팀장 결정)
- [ ] server: "어제 메모" 카드(복기 · 하루 정리), 담당별 하루 AI 잔량 바, 익절 · 손절 연출
- [ ] 사무실 e2e 전체 테스트(브라우저로 시연 서버: 첫 화면 · 이동 · 논의 · 채팅 · 멘션 · 공지 · 승인 · 외형 · 가벼운 화면 · 다시 잇기 · 폰 · 다크 모드) + 영상


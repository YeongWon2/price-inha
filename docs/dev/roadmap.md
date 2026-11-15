# 로드맵 T6~T22 모듈 계획 (2026-11-16)

`docs/product/stock-team/dev.md`의 작업 계획 T6~T22를 모듈로 나눈 표다. 사용자 승인(2026-11-16): 새 모듈 등록과 검사 스크립트의 모듈 목록 확장, 조립(server)을 gate에서 떼어 내기, 그리고 `docs/dev/decisions.md` 2026-11-16 절의 결정.

## 전제 (W0, 끝남)

1. `scripts/lib.sh`: 모든 모듈 목록(`ALL_MODULES`)과, 크레이트가 있는 모듈만 검사하는 `MODULES`. gate는 `crates/gate`, server는 `crates/server`(패키지 `-p price-inha`).
2. `scripts/selftest.py`: 모듈 목록을 `architecture.toml`의 크레이트에서 낸다. server는 라이브러리가 없어 테스트가 크레이트를 부르지 않는다.
3. `scripts/pipeline.py`: 모듈 폴더는 모두 `crates/<모듈>`.
4. `red/server` 태그를 조립을 떼어 낸 시점에 붙였다. `.pipeline/state.json`의 server는 검토 통과(gate 1~8차를 이어받음).
5. 크레이트 이름은 `pi-<모듈>`, 폴더는 `crates/<모듈>`이다.
6. architecture.toml의 새 항목은 지금 더하지 않는다. 모듈마다 N0에서 크레이트 뼈대와 함께 더한다(`docs/dev/pipeline.md` 6장 "새 모듈을 더하는 절차"). 그때 함께 더할 금지 모양(초안): `ARCH-REAL-BROKER`(pi-paper 밖에서 `Broker` 구현 금지), `ARCH-MONEY-FLOAT-2`(template · rules · exec · journal · approval에서 f32 · f64 금지), `ARCH-RULES-PURE`(template · rules · journal에서 네트워크 · 파일 · 시계 금지), `ARCH-CLOCK-2`, `ARCH-NET-IN-CORE`, `ARCH-ENV-READ-2`, `CONV-BOX-ERROR-2`. 새 모양마다 selftest 표본을 더한다.

## 모듈 표

| 물결 | 모듈 | 크레이트 · 폴더 | 구성 요소 | 맡는 T | needs | 크기 |
|---|---|---|---|---|---|---|
| W1 | store 9차 | pi-store | A12 | T8 · T14 · T15 · T7 · T17 저장 | — | M |
| W1 | secrets 6차 | pi-secrets | A13 | T10 · T19 · T22 키 | — | S |
| W1 | ai 7차 | pi-ai | A11 | T18 · T9(나가는 글 가리기 자리, 요청별 모델) | — | S |
| W1 | template | pi-template · crates/template | A6 템플릿 · 상한, M3 | T8 | [] | S |
| W1 | data | pi-data · crates/data | A10 | T10 | secrets | M |
| W1 | trigger | pi-trigger · crates/trigger | A5 | T11, T13 일정 | market | M |
| W2 | rules | pi-rules · crates/rules | A6 | T6, T13 손실 · 멈춤 판단 | template, paper, market | L |
| W2 | approval | pi-approval · crates/approval | A7 | T14 | store(9차 뒤) | M |
| W2 | gate 9차 | pi-gate | A2 | T17 | store(9차 뒤) | M |
| W2 | store 10차 | pi-store | A12 | T9 · T11 · T10 저장 | — | M |
| W2 | office | pi-office · crates/office (+ web/office/) | A3 · A1 | T20, T21 | store | L |
| W3 | exec | pi-exec · crates/exec | A8(모의계좌 바깥) | T7 남은 것 | paper, store | M |
| W3 | journal | pi-journal · crates/journal | A4 복기 · M4 · 하루 정리 | T12, T15 | store, template | M |
| W3 | agent | pi-agent · crates/agent | A4 | T9 | ai, store(10차 뒤) | XL |
| W3 | store 11차 | pi-store | A12 | T16 · T19 · T22 저장 | — | M |
| W3 | notify | pi-notify · crates/notify (+ web/phone/) | A3 알림 · A1 폰 | T22, E2 | store | M |
| W4 | knowledge | pi-knowledge · crates/knowledge | A12 검색 | T16 | store(11차 뒤) | M |
| W4 | backup | pi-backup · crates/backup | A14 | T19 | store(11차 뒤), secrets(6차 뒤) | M |
| W5 | server 1차 | price-inha · crates/server | 조립 | T6 · T7 · T8 · T13 · T14, T18 tracing | 위 전부 | L |
| W5 | server 2차 | price-inha | 조립 | T9 · T10 · T11 · T12 · T15 | — | L |
| W5 | server 3차 | price-inha | 조립 | T16 · T19 · T20 · T21 · T22, V47 | — | L |

체크포인트: server 1차 뒤 V5~V9 · V24~V31 · V42~V45 / 2차 뒤 V1~V4 · V16~V21 · V32~V36 / 3차 뒤 V10~V15 · V22 · V23 · V37~V41 · V46~V52.

가장 오래 걸리는 줄: ai 7 → store 10 → agent → server 2차.

## 이미 있는 것과 겹치는 곳

- **T7**: pi-paper에 증권사 trait, 받을 때의 확인, 체결, 미체결 취소(E12), 계좌 종류(E25), 일부 체결(E29), 점검 통과 표시(E33), 상태 내보내기 · 되살리기가 있다. 남은 것은 exec와 서버 연결이다: 점검 통과 표시를 낼 권한을 rules에 건네기, 주문됨으로 먼저 저장하고 주문 내기, 체결을 저장소 매매 상태로 옮기기, E13 · E30, 스냅샷 저장(V6), 다시 켠 뒤 맞춰 보기(E16).
- **T13**: 팀 상태 5개와 전이는 store에 있다. 남은 것은 손실 계산(rules), 장 시작 · 마감 일정(trigger), 팀장의 쉬기 판단(agent), 서버 연결.
- **T17**: 루프백만 키 없이 받기, 메모리 세션(7일), Host · Origin 검사, 접속 기록 쌓기는 있다. 남은 것은 접속 키 · 세대, 루프백 밖에서 들어오기, 잠금, 잠금 알림, 원격 세션 Max-Age, 대조표 D17 · D21 · D22(gate 9차).
- **T18**: secrets의 `redact`와 관문 이벤트 가리기는 있다. 남은 것은 tracing 출력(server 1차), AI로 나가는 글(ai 7차), 알림(notify), 백업 바이트 검사(backup · secrets 6차), V47 시험(server 3차).
- **T20~T22 화면**: 지금은 `crates/server/web/index.html` 한 장이다.

## 화면의 자리

- 3D 사무실과 채팅: `crates/office/web/office/`. three.js는 `web/office/vendor/`에 넣고(라이선스 파일 함께) 바깥 CDN을 쓰지 않는다.
- 폰 PWA: `crates/notify/web/phone/`(매니페스트 · sw.js · 가벼운 화면). three.js를 불러오지 않는다.
- 크레이트는 `web_assets() -> &'static [Asset]`(`include_bytes!`)를 공개하고 price-inha가 경로에 단다. Node 빌드는 없다.
- 웹 규칙(바깥 주소 없음, 폰 묶음에 three.js 없음, 매니페스트 칸)은 check_arch.py가 `.rs`만 보므로 그 크레이트의 Rust 테스트로 확인한다.

## 진행

물결 순서로 열되, 같은 물결 안에서는 needs가 풀린 것부터 병렬로 한다. 모듈 하나가 검토를 통과할 때마다 push한다.

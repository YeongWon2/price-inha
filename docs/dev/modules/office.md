# office 모듈 (T20, T21)

크레이트 `pi-office`, 폴더 `crates/office/`(서버)(서버만. 3D 사무실 · 채팅 화면은 server 11차부터 `crates/server/web/office/` — 아래 '옛 화면 정리'). 구성 요소 A3 · A1. `pi-store`를 쓴다. 이벤트를 보내는 길은 포트로 받는다(조립에서 gate의 Publisher를 끼운다).
근거: `dev.md` D1 · D11 · C2 · C5 · E3 · E6 · E37 · V1 · V13~V15, `spec.md` F7.

## 원본 동작 근거

- 장면 · 격자 32px · 자리 앵커 · 카메라 38도 · 그리기 대책 · 탭이 숨으면 멈춤: `docs/research/office-deep.md` 1 · 2 · 9장, `docs/research/deskrpg.md` 7 · 9-2.
- 회의 모습: `docs/research/office-deep.md` 3장, `docs/research/deskrpg.md` 6.
- 채팅 수명주기(queued → thinking → complete, updatedAt 단조 증가, 끝난 뒤 바뀌지 않음, 담당별 FIFO 8): `docs/research/office-deep.md` 4장, `docs/research/deskrpg.md` 3-2 · 4-6, `docs/research/agent-runtime-deep.md` 8장, `docs/research/hermes-agent.md` 5.3. 대조표 `docs/dev/conformance.md` D19 · D20.
- 침대와 출근: `docs/research/office-deep.md` 11장(원본에 없음, 제안 구조).
- 의도해서 다름: (사용자 캐릭터는 결정 17로 생긴다 — 5차) 서버는 지도를 모르고 "자리 이름"만 보냄, 자산은 새로 만듦(라이선스, `docs/research/deskrpg.md` 9-4), 출근 · 퇴근은 팀 상태(M1)에 묶음(결정 2026-11-16 25번).

## 만드는 것 (1차)

1. 서버: `place_of(팀 상태, 담당 활동) -> 자리`(침대 · 자기 책상 · 논의 탁자)와 번호 있는 이벤트 `office.agent` (V13 · V15).
2. 채팅(C5): 팀 방 1 + 담당별 1:1, 수명주기 객체, 담당 실패 표시(E6). 스트리밍 없이 queued → thinking → complete(결정 13번).
3. 화면이 없어도 이벤트를 쌓는다(E37).
4. 웹
   - ES 모듈, three.js를 저장소에 넣음(라이선스 파일 함께, Node 빌드 없음).
   - 지도 정의 파일(6명의 책상 · 방 · 침대 · 논의 탁자).
   - 절차적 캐릭터, DOM 이름표, 채팅 칸.
   - 끊기면 `last_seq`로 잇기(E3), WebGL이 안 되면 가벼운 화면으로.
5. `web_assets()`.

## 2차 후보 (1차 검토의 참고)
- 끝남: ask · finish에서 첫 이벤트는 나갔는데 둘째 이벤트가 실패하면 오류로 돌려주지 않고(이미 들어간 응답) 다음 바꿈 때 다시 낸다. — 커밋 e0c9a1a(red) · 10805ee(green) · 전문: `git show 1a73576:docs/dev/modules/office.md`

## 하지 않는 것

길 찾기의 서버 동기화, 지도 편집, 사용자 걸어 다니기, 토큰 스트리밍.

## 테스트 주의

투영은 순수 함수로 표 시험을 한다. 자산은 Rust 테스트로 확인한다(결정 27번).
- 바깥 `http(s)://`를 불러오지 않는다.
- three.js 라이선스 파일이 있다.
- 지도 JSON에서 6명 모두 책상과 침대가 있다(V1 · V15).
- 화면이 실제로 그려지는지는 G3에서 브라우저로 본다.

## 필수 번호

C2 C5 E3 E6 E37 V1 V13 V14 V15 D20

## 4차 작업 (후보 일괄 처리)
- 끝남: 2차 후보 1: ask · start_next · complete · fail에서 응답 이벤트가 나갔으면 성공이다. 담당 모습 이벤트가 실패해도 오류로 돌려주지… — 커밋 2c4bca1(red) · cff8e1c(green) · 전문: `git show 1a73576:docs/dev/modules/office.md`

## 옛 화면 정리 (2026-11-24, 결정 1 · server 11차 뒤)

server 11차에서 화면을 `crates/server/web`(Vite)으로 옮겼고, server는 더는 이 크레이트의 화면 자산을 쓰지 않는다. 이 크레이트에서 옛 화면을 지운다.

1. 지울 것: `crates/office/web/` 폴더 전체, `src/web.rs`(`WebAsset` · `web_assets` · `INDEX_PATH` · `MAP_PATH`)와 `lib.rs`의 내보내기, 그것만 보는 시험(`tests/web_assets.rs`, `tests/web_prune.rs`).
2. 남길 것: 이 크레이트의 다른 기능(이벤트 · 알림 · 그 밖의 공개 API)과 그 시험은 그대로. 다른 크레이트(특히 server)는 고치지 않는다 — server가 아직 이것을 쓰면 멈추고 보고한다.
3. 지우는 시험이 보던 동작 가운데 화면 쪽 동작(예: 응답 목록 정리, 서비스 워커 · manifest 모양)이 server의 시험(`crates/server/tests/web/`, `round20`)에 이미 있는지 보고, 없는 것은 목록으로 보고한다(server 다음 차수에 옮긴다).
4. 시험: 지운 뒤 이 크레이트의 공개 API에 화면 자산이 없다(이름이 없어 컴파일되지 않는 것은 시험할 수 없으니, 남은 시험이 통과하고 `web/` 폴더가 없음을 Rust 시험 하나로 본다 — `CARGO_MANIFEST_DIR` 아래 `web`이 없다).

## 5차 작업 (2026-11-25 조정자, 결정 17 — 부르기 단계 · 오래된 "일하는 중" 풀기)
- 끝남: 근거: docs/product/stock-team/additions.md의 D12 · C20 · M12 · E47 · E48 · E55 · N10 · N15 ·… — 커밋 e82f4b4(red) · 5b0fcd8(green) · 전문: `git show 1a73576:docs/dev/modules/office.md`

## 6차 작업 (2026-11-25 조정자, 결정 17 — 멘션 · 응답자 정하기 · 공지 나눠 주기)
- 끝남: 근거: docs/product/stock-team/additions.md의 C18 · C19 · E43~E46 · N11 · N18 · T24(차수 표 11번)… — 커밋 fc4a9b1(red) · e3e83a7(green) · 전문: `git show 1a73576:docs/dev/modules/office.md`

## 7차 작업 (2026-11-27 조정자, 결정 29 — 사용자 "멘션을 하거나 해당 사람한테 대화를 시도하면 내 앞으로 다 모여야지")

동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지 보고 잠근다.

1. **멘션하면 모두 곧바로**: `Office::mention_posted(&[Agent])` — 멘션된 담당 모두를 곧바로 `Called`(이유 Mention)로. 같은 담당은 한 번, 이미 오는 중 · 와 있으면 그대로, 논의 중이면 `MeetingReserved`. 답 턴 시작 때 부르던 길은 server가 지운다.

- 원본 동작 근거: `docs/research/office-deep.md` · `agent-office-refs.md`(부르기 · 길 찾기). 모이는 때를 바꾼 것은 결정 29(사용자).
- 시험으로 확인할 것: `tests/gather.rs`(3개)

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| 새 시험 `tests/gather.rs`(3개) | 유지 | 기능 작업 폴더에서 red 커밋으로 먼저 쓰고 green 커밋으로 통과 |
| 그 밖의 office 시험 | 유지 | 동작이 바뀌지 않는다 |

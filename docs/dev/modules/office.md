# office 모듈 (T20, T21)

크레이트 `pi-office`, 폴더 `crates/office/`(서버) + `crates/office/web/office/`(3D 사무실 · 채팅 화면). 구성 요소 A3 · A1. `pi-store`를 쓴다. 이벤트를 보내는 길은 포트로 받는다(조립에서 gate의 Publisher를 끼운다).
근거: `dev.md` D1 · D11 · C2 · C5 · E3 · E6 · E37 · V1 · V13~V15, `spec.md` F7.

## 원본 동작 근거

- 장면 · 격자 32px · 자리 앵커 · 카메라 38도 · 그리기 대책 · 탭이 숨으면 멈춤: `docs/research/office-deep.md` 1 · 2 · 9장, `docs/research/deskrpg.md` 7 · 9-2.
- 회의 모습: `docs/research/office-deep.md` 3장, `docs/research/deskrpg.md` 6.
- 채팅 수명주기(queued → thinking → complete, updatedAt 단조 증가, 끝난 뒤 바뀌지 않음, 담당별 FIFO 8): `docs/research/office-deep.md` 4장, `docs/research/deskrpg.md` 3-2 · 4-6, `docs/research/agent-runtime-deep.md` 8장, `docs/research/hermes-agent.md` 5.3. 대조표 `docs/dev/conformance.md` D19 · D20.
- 침대와 출근: `docs/research/office-deep.md` 11장(원본에 없음, 제안 구조).
- 의도해서 다름: 사용자 캐릭터 없음, 서버는 지도를 모르고 "자리 이름"만 보냄, 자산은 새로 만듦(라이선스, `docs/research/deskrpg.md` 9-4), 출근 · 퇴근은 팀 상태(M1)에 묶음(결정 2026-11-16 25번).

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

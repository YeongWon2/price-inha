# notify 모듈 (T22, 잠금 알림 E2)

크레이트 `pi-notify`, 폴더 `crates/notify/`(서버)(서버만. 폰 PWA 화면은 server 11차부터 `crates/server/web/phone/` — 아래 '옛 화면 정리'). 구성 요소 A3 알림 · A1 폰 화면. `pi-store`(11차의 구독 표)를 쓴다. 가리기와 시계는 포트로 받는다.
근거: `dev.md` D11 · N5 · C7 · E2 · E38 · V10 · V11 · V12, `spec.md` F7 1a · FN2 1.

## 원본 동작 근거

- 승인 웹 푸시와 조용한 시간 · 종류별 설정: `docs/research/triggers-approval-memory-deep.md` 3.4(OpenClaw `approval-web-push`).
- 알림 · 주의 수신함: `docs/research/office-deep.md` 7-4, `docs/research/deskrpg.md` 7-1 · 3-1.
- 폰 화면을 따로 묶고 3D를 불러오지 않음, 매니페스트: `docs/research/office-deep.md` 8-3 · 9-4.
- 위협과 보호: `docs/research/remote-access-security.md` 5장.
- 웹 푸시 암호화(RFC 8291)와 VAPID(RFC 8292)는 표준을 따른다(우리가 옮길 원본 코드는 없다).

## 만드는 것 (1차)

1. 알림 종류(승인 요청, 멈춤, 이상, 접속 잠금 E2, 백업 실패, 하루 정리)와 보내기
   - 번호 있는 이벤트 + 웹 푸시.
   - 일이 생긴 뒤 10초 안에 첫 시도를 한다(N5 · V11).
   - 승인과 이상은 조용한 시간을 무시한다. 조용한 시간 기본값은 없음.
2. 보내기 전에 가린다(E38).
3. VAPID ES256 서명과 aes128gcm 암호화를 순수 Rust(p256 · aes-gcm · hkdf)로 한다. openssl-sys · native-tls가 끌려 들어오지 않게 한다. VAPID 키는 처음 켤 때 만들어 비밀 폴더에 0600으로 두고 백업에서 뺀다(결정 2026-11-16 24번). 404 · 410이면 구독을 지운다.
4. 폰 웹(따로 묶음, 결정 26번)
   - 승인 카드(승인 · 거절, 6자리 표시, V10), 알림 목록, 하루 정리(V12), "3D 사무실" 단추.
   - manifest, sw.js.
   - three.js를 불러오지 않는다.

## 하지 않는 것

승인 판정(approval), 접속 확인(gate).

## 테스트 주의

푸시 서비스는 로컬 가짜 서버로 하고, 받은 본문을 테스트 쪽 키로 풀어 본다. 폰 묶음에 three 글자가 없는지 Rust 테스트로 본다.

## 필수 번호

N5 E2 E38 V10 V11 V12

## 1차 검토 참고 (다음 차수 후보)
- 끝남: SSRF: 구독 주소는 store에서 https://인지만 본다. https://localhost · 사설 IP 같은 내부망 주소를 막는다(보내기 전 해석된 주… — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/notify.md`

## 2차 작업 (후보 일괄 처리 때 꼭 함)
- 끝남: SSRF: 보낼 때 구독 주소의 이름을 해석해 해석된 주소가 하나라도 내부망(루프백 · 사설 · 링크 로컬 · ULA 등)이면 보내지 않고, 검사한 IP로만 접… — 커밋 7885605(red) · 3d80e16(green) · 전문: `git show 1a73576:docs/dev/modules/notify.md`

## 3차 작업 (2차 검토 되돌림)
- 끝남: 내부망 판정: IPv4를 담는 IPv6 형식을 모두 풀어 IPv4 판정에 넘긴다. NAT64 64:ff9b::/96 · 64:ff9b:1::/48, IPv4-c… — 커밋 9c13004(red) · 213ef33(green) · 전문: `git show 1a73576:docs/dev/modules/notify.md`

## 4차 작업 (후보 일괄 처리)
- 끝남: 내부망 판정에 Teredo 2001::/32 · site-local fec0::/10을 더했다. — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/notify.md`

## 5차 작업 (server 6차와 함께)
- 끝남: HttpTransport::redirect_to를 test-redirect feature 뒤로 옮긴다(기본은 꺼짐). 기본 빌드에서 그 함수가 없는지 시험한다. — 커밋 39afc66(red) · b24fba3(green) · 전문: `git show 1a73576:docs/dev/modules/notify.md`

## 옛 화면 정리 (2026-11-24, 결정 1 · server 11차 뒤)

server 11차에서 화면을 `crates/server/web`(Vite)으로 옮겼고, server는 더는 이 크레이트의 화면 자산을 쓰지 않는다. 이 크레이트에서 옛 화면을 지운다.

1. 지울 것: `crates/notify/web/` 폴더 전체, `src/phone.rs`(`Asset` · `phone_asset` · `phone_assets`)와 `lib.rs`의 내보내기 · 문서 주석의 `web/phone/` 언급, 그것만 보는 시험(`tests/suite/phone.rs`).
2. 남길 것: 이 크레이트의 다른 기능(이벤트 · 알림 · 그 밖의 공개 API)과 그 시험은 그대로. 다른 크레이트(특히 server)는 고치지 않는다 — server가 아직 이것을 쓰면 멈추고 보고한다.
3. 지우는 시험이 보던 동작 가운데 화면 쪽 동작(예: 응답 목록 정리, 서비스 워커 · manifest 모양)이 server의 시험(`crates/server/tests/web/`, `round20`)에 이미 있는지 보고, 없는 것은 목록으로 보고한다(server 다음 차수에 옮긴다).
4. 시험: 지운 뒤 이 크레이트의 공개 API에 화면 자산이 없다(이름이 없어 컴파일되지 않는 것은 시험할 수 없으니, 남은 시험이 통과하고 `web/` 폴더가 없음을 Rust 시험 하나로 본다 — `CARGO_MANIFEST_DIR` 아래 `web`이 없다).

## 6차 작업 (2026-11-27 조정자, 백로그 Q — 사용자 "백로그 다 실행해" · "남은 것들만 다 실행해 key 제외하고")

동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지 보고 잠근다.

1. `release_held`를 동시에 두 번 불러도 한 번만 보낸다는 시험을 더한다(`release_gate`가 이미 있어 처음부터 통과 — 시험만).

- 원본 동작 근거: 지시서 그대로.
- 시험으로 확인할 것: `tests/round5/`(release_twice)가 합친 구현에서 통과하고, 기존 시험도 모두 통과한다.

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| 새 시험 `tests/round5/`(release_twice) | 유지 | 작업 폴더에서 red 커밋으로 먼저 쓰고 green 커밋으로 통과 |
| 그 밖의 notify 시험 | 유지 | 동작이 바뀌지 않는다 |

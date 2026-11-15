# notify 모듈 (T22, 잠금 알림 E2)

크레이트 `pi-notify`, 폴더 `crates/notify/`(서버) + `crates/notify/web/phone/`(폰 PWA). 구성 요소 A3 알림 · A1 폰 화면. `pi-store`(11차의 구독 표)를 쓴다. 가리기와 시계는 포트로 받는다.
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

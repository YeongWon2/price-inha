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

- SSRF: 구독 주소는 store에서 `https://`인지만 본다. `https://localhost` · 사설 IP 같은 내부망 주소를 막는다(보내기 전 해석된 주소 검사, 또는 알려진 푸시 서비스 도메인만). 리다이렉트는 이미 막힘.
- 푸시 본문의 `link`를 자르지 않아 아주 길면 3993바이트를 넘어 모든 구독 보내기가 실패한다.
- 조용한 시간에 미룬 알림(백업 실패 · 하루 정리)을 나중에 다시 보내는 길이 없다.
- VAPID 키 파일을 만들고 쓰는 사이에 꺼지면 빈 파일이 남아 켤 때마다 InvalidKey로 멈춘다(임시 파일에 쓰고 이름 바꾸기).
- RFC 8291 부록 A 시험값으로 확인하는 테스트를 더한다.
- 폰 화면이 가정한 서버 경로(`/api/push/key`, `/api/push/subscribe`, `/api/notices`, `/api/approvals/{id}/approve|reject`, `/api/daily-summary`)를 server 조립 때 맞춘다.

## 2차 작업 (후보 일괄 처리 때 꼭 함)

- SSRF: 보낼 때 구독 주소의 이름을 해석해 해석된 주소가 하나라도 내부망(루프백 · 사설 · 링크 로컬 · ULA 등)이면 보내지 않고, 검사한 IP로만 접속한다(reqwest `resolve`로 고정, DNS 재바인딩 막기). 리다이렉트는 계속 막는다. 서버의 구독 때 글자 검사(`crates/server/src/reach/phone_http.rs` `inside`)는 앞단 거르기로 남긴다.
- 폰 승인: 결정 4번대로 사용자가 숫자를 치지 않는다. 카드에 지문 앞 6자리를 보이고, 화면이 승인 카드의 지문을 그대로 보낸다(지금 `app.js`의 6자리 숫자 입력 강제를 없앤다).

## 3차 작업 (2차 검토 되돌림)

- 내부망 판정: IPv4를 담는 IPv6 형식을 모두 풀어 IPv4 판정에 넘긴다. NAT64 `64:ff9b::/96` · `64:ff9b:1::/48`, IPv4-compatible `::a.b.c.d`, 6to4 `2002::/16`. IPv4 예약 대역 240/4 · 198.18/15 · 192.0.0/24도 내부망으로 본다. `ssrf.rs` 표에 예를 더한다.
- VAPID 임시 파일: `create_new`로 만들고(심볼릭 링크를 따라가지 않음), 이름에 프로세스 안에서도 겹치지 않는 값을 넣고, 시작할 때 남은 `vapid.tmp-*`를 지운다.
- 시험용 `redirect_to`는 그대로 둔다(서버 시험이 쓴다). 다음 차수 후보: 시험 전용 feature로 묶기.

### 3차 검토 참고 (다음 차수 후보)
- 내부망 판정에 Teredo `2001::/32`, 폐기된 site-local `fec0::/10`이 없다.
- 이미 있는 `vapid.key`가 링크이면 권한 좁히기가 링크를 따라간다.
- 여러 스레드 · 프로세스가 같은 비밀 폴더로 동시에 처음 켜지면 키가 다르거나 이름 바꾸기가 실패할 수 있다(파일 잠금으로 묶기).
- 시험용 `redirect_to`를 시험 전용 feature로 묶는다.

## 4차 작업 (후보 일괄 처리)

- 내부망 판정에 Teredo `2001::/32` · site-local `fec0::/10`을 더했다.
- `vapid.key`가 링크이면 따라가지 않고 `Error::KeyFile`로 멈춘다(가리킨 파일의 권한 · 내용을 건드리지 않는다). 폴더 잠금(폴더 자체에 배타 잠금, Unix)으로 동시에 처음 켜도 모두 같은 키를 받는다.
- 미룬 푸시 다시 보내기: 미룰 때 `push.held` 표시 이벤트를 남기고, `Notifier::release_held()`가 조용한 시간이 아닐 때 기한(`HELD_RELEASE_WINDOW_MS` 24시간) 안에 미룬 것을 번호 순서로 한 번씩 보낸 뒤 `push.released`를 남긴다. 표시는 알림 목록에 나오지 않고 다시 켜도 이어진다. 구독이 없으면 표시를 남기지 않는다. 부르는 쪽(server)이 조용한 시간 끝 · 구독 때 불러야 한다.
- 하지 않음(넘김): 폰 화면이 가정한 서버 경로 맞추기(server 조립), 시험용 `redirect_to`를 시험 전용 feature로 묶기(server 시험도 쓰므로 server `Cargo.toml` 수정이 필요).

### 4차 검토 참고 (다음 차수 후보)
- `release_held`를 동시에 두 번 부르는 시험(`release_gate`)이 없다.

## 5차 작업 (server 6차와 함께)
- `HttpTransport::redirect_to`를 `test-redirect` feature 뒤로 옮긴다(기본은 꺼짐). 기본 빌드에서 그 함수가 없는지 시험한다.

- `HttpTransport::redirect_to`를 `test-redirect` feature(기본 꺼짐) 뒤로 옮겼다. notify 자기 시험은 dev-dependency로 자신을 이 feature를 켜서 쓴다. server는 자기 `Cargo.toml`의 dev-dependencies에서 `pi-notify`에 `features = ["test-redirect"]`를 켜야 한다(server 6차).

## 옛 화면 정리 (2026-11-24, 결정 1 · server 11차 뒤)

server 11차에서 화면을 `crates/server/web`(Vite)으로 옮겼고, server는 더는 이 크레이트의 화면 자산을 쓰지 않는다. 이 크레이트에서 옛 화면을 지운다.

1. 지울 것: `crates/notify/web/` 폴더 전체, `src/phone.rs`(`Asset` · `phone_asset` · `phone_assets`)와 `lib.rs`의 내보내기 · 문서 주석의 `web/phone/` 언급, 그것만 보는 시험(`tests/suite/phone.rs`).
2. 남길 것: 이 크레이트의 다른 기능(이벤트 · 알림 · 그 밖의 공개 API)과 그 시험은 그대로. 다른 크레이트(특히 server)는 고치지 않는다 — server가 아직 이것을 쓰면 멈추고 보고한다.
3. 지우는 시험이 보던 동작 가운데 화면 쪽 동작(예: 응답 목록 정리, 서비스 워커 · manifest 모양)이 server의 시험(`crates/server/tests/web/`, `round20`)에 이미 있는지 보고, 없는 것은 목록으로 보고한다(server 다음 차수에 옮긴다).
4. 시험: 지운 뒤 이 크레이트의 공개 API에 화면 자산이 없다(이름이 없어 컴파일되지 않는 것은 시험할 수 없으니, 남은 시험이 통과하고 `web/` 폴더가 없음을 Rust 시험 하나로 본다 — `CARGO_MANIFEST_DIR` 아래 `web`이 없다).

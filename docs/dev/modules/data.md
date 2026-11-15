# data 모듈 (T10)

크레이트 `pi-data`, 폴더 `crates/data/`. 구성 요소 A10. `pi-secrets`를 쓴다(자료 키, secrets 6차의 출처별 칸). 모양은 `pi-market`과 같게 한다.
근거: `dev.md` D9 · C11 · E14 · E32 · V33 · V34 · V36, `principles.md` P10.

## 원본 동작 근거

- 원본에 없음(우리 고유): Hermes · OpenClaw · DeskRPG에는 재무 · 거시 · 공시 자료 어댑터가 없다.
- 기준: 각 공식 API의 공개 명세(DART 오픈API · 한국은행 ECOS · FRED), `docs/research/stock-investing-principles.md` 3장(판단 시점까지 알 수 있던 자료만).
- 어댑터의 모양(trait · 가짜 어댑터 · 주소 주입 · 키를 오류에 싣지 않기)은 `docs/dev/modules/market.md`를 따른다.

## 만드는 것 (1차)

1. `DataAdapter::fetch(종목|지표, 종류) -> Fact` (C11)
   - `Fact`의 칸: 값, 출처 이름 · 주소, 공표 시각, 알게 된 시각(받은 시각, Clock).
   - 종류: 공시(DART), 재무(DART), 금리 · 환율(ECOS), 미국 금리(FRED).
2. 기본 주소는 생성자 인자로 받는다. reqwest는 rustls를 쓴다.
3. 오류를 종류로 구분한다(E14): 닿지 않음, 시간 초과, 키 없음, 한도, 형식. 키가 쿼리에 실리는 API는 오류의 주소에서 키를 가린다.
4. 비어 있으면 오류가 아니라 `Missing(종류)`로 돌려준다(E32: 판단을 지켜보기로 두는 근거).
5. `FakeDataAdapter`(정해진 Fact와 오류를 돌려준다).

## 확인기

`pi-secrets`의 `Verifier::verify_slot` 기본은 언제나 거부다. 자료 키를 화면에서 등록하려면 이 모듈이 출처별로 닿는지 확인하는 함수를 공개하고, 조립(server)이 그것으로 `verify_slot`을 구현한다.

## 하지 않는 것

뉴스 검색(V35)과 SEC EDGAR는 2차로 미룬다(결정 2026-11-16 17번: EDGAR의 User-Agent 연락처는 사용자가 정한다). 저장(store 10차), 판단, 다시 시도 일정(trigger).

## 테스트 주의

로컬 가짜 서버(wiremock)만 쓴다. 실제 공식 API 주소를 테스트에 넣지 않는다. 받은 값의 알게 된 시각이 가짜 시계의 값과 같은지 본다.

## 필수 번호

C11 E14 E32 V33 V34 V36 P10

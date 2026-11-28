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

## 2차 작업 (1차 검토의 참고 — server 조립 전에 한다)
- 끝남: 받을 양의 상한(ECOS 10000행, DART 20쪽)을 넘으면 잘린 결과를 완전한 것처럼 돌려주지 않고 잘렸다는 표시(또는 오류)를 낸다. — 커밋 058aca1(red) · 0a0188d(green) · 전문: `git show 1a73576:docs/dev/modules/data.md`

## 3차 후보 (2차 검토의 참고)
- 끝남: ECOS list_total_count가 없거나 숫자가 아니면 받은 행 수가 상한과 같을 때 잘림으로 본다. — 커밋 359f1f6(red) · 933cce4(green) · 전문: `git show 1a73576:docs/dev/modules/data.md`

## 3차 작업 (후보 일괄 처리)
- 끝남: 2차 검토의 "3차 후보" 1~2번을 처리했다. 공개 API는 더하기만 했다. — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/data.md`

## 4차 작업 (2026-11-21 발표 자료 대조 — 공급자 확장)
- 끝남: 근거: docs/dev/decisions.md 2026-11-21 2번(공급자 확장 구조) · 3번(뉴스: 네이버 뉴스 검색 API), dev.md D9 · V… — 커밋 f13af53(red) · 20921fb(green) · 전문: `git show 1a73576:docs/dev/modules/data.md`

## 5차 작업 (2026-11-21 이상한 기사는 그것만 버린다 — 사용자 결정)

사용자 결정(2026-11-21 16번): 네이버 응답에 링크가 이상한 기사가 섞이면 **그 기사만 버리고 나머지는 받는다. 버린 수는 결과에 보인다.** 4차의 "기사 하나라도 링크가 이상하면 응답 전체가 `Format`"을 바꾼다.

- **공개 API (더하기만)**: `NewsFetched::Items`에 버린 수를 실을 자리 — `NewsFetched::Items(Vec<NewsItem>)`의 모양을 잠긴 테스트가 쓰면 새 갈래 `NewsFetched::Partial { items: Vec<NewsItem>, dropped: usize }`를 더하고, 버린 것이 없으면 지금처럼 `Items`를 준다(4차 잠긴 테스트가 `Items`를 기대하는 곳은 그대로 지난다).
- **기사 단위로 버리는 것**: 링크(`originallink` · `link`)가 4차 2번 규칙(제어 문자 · 공백 · `"<>\``` ` ``` · userinfo · 비정상 꼴 · http(s) 아님 · 호스트 없음)에 걸린 기사. 그 기사의 `originallink`가 걸리고 `link`가 멀쩡하면 `link`를 쓴다(버리지 않는다).
- **여전히 응답 전체가 `Format`인 것**: 응답의 모양 자체가 깨진 경우 — JSON이 아님, `items`가 없거나 배열이 아님, 기사의 필수 칸(제목 · 요약 · 링크 · 시각)이 없거나 시각을 읽을 수 없음. (링크 **값**이 이상한 것만 기사 단위로 버린다.)
- 버리고 남은 것이 없으면 `Missing`이 아니라 `Partial { items: [], dropped: n }`(버린 것이 있었다는 사실을 숨기지 않는다). 받는 쪽(server)은 이것을 "뉴스 없음"으로 다루되 버린 수를 판단 기록에 남긴다.
- 원본 동작 근거: 원본에 없음 — 사용자 결정 2026-11-21 16번, 스펙 V35 · E32.
- 테스트로 확인할 것: 좋은 기사 둘 + 링크에 줄바꿈 든 기사 하나 → `Partial { items: 2, dropped: 1 }` / `originallink`만 이상하고 `link`는 멀쩡하면 버리지 않고 `link` 사용 / 모두 이상하면 `Partial { [], 3 }` / 이상한 것이 없으면 `Items` / 칸이 빠진 기사가 섞이면 여전히 전체 `Format` / 4차 잠긴 테스트의 "기사 하나짜리 응답에 이상한 링크 → `Format`"은 이번에 뜻이 바뀐다 — 그 테스트(`tests/news.rs:591` 근처)는 이 차수에서 고친다(reopen).

## 하지 않는 것

이 차수 뒤에도 하지 않는 것: 뉴스 본문 긁어 오기, 다른 뉴스 공급자(구글 등 — 다음에 어댑터만 더한다), 뉴스의 `FactKind` 편입(위 부딪히는 곳).
뉴스 검색(V35)은 4차에서 한다(결정 2026-11-21 3번). SEC EDGAR는 미룬다(결정 2026-11-16 17번: EDGAR의 User-Agent 연락처는 사용자가 정한다). 저장(store 10차), 판단, 다시 시도 일정(trigger).

## 테스트 주의

로컬 가짜 서버(wiremock)만 쓴다. 실제 공식 API 주소를 테스트에 넣지 않는다. 받은 값의 알게 된 시각이 가짜 시계의 값과 같은지 본다.

## 필수 번호

C11 E14 E32 V33 V34 V36 P10 D9 V35

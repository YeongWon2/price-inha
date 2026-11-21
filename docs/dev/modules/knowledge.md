# knowledge 모듈 (T16)

크레이트 `pi-knowledge`, 폴더 `crates/knowledge/`. 구성 요소 A12의 검색 부분. `pi-store`(11차의 조각 · FTS5 trigram · 임베딩 칸)를 쓴다. 임베딩은 `Embedder` 포트로 받는다. 벡터에는 f32를 써도 된다(돈이 아니다).
근거: `dev.md` D10 · M7 · V22 · V23.

## 원본 동작 근거

- 표 모양, 조각 400/80과 줄 번호 인용, 합치는 식(0.7/0.3 · 반감기 30일 · MMR 0.7 · 후보 4배 · 결과 6 · 문턱 0.35), 한글 trigram과 3글자 미만은 LIKE, 0건이면 AND → OR, 임베딩이 없으면 키워드만이고 "쓸 수 없음"을 돌려줌, 다시 색인 조건, 앞 8,192자만 색인: `docs/research/triggers-approval-memory-deep.md` 4.1 · 4.2 · 4.3, `docs/research/openclaw.md` 2.3 · 4의 7번, `docs/research/hermes-agent.md` 3.5 · 7.1.
- 조건으로 찾기(종목 · 기간 · 손익)는 원본에 없음(우리 고유, 4.3).

## 만드는 것 (1차)

키워드 검색, 조건 검색, 합치기, 출처 등급(바깥 자료는 자동 회상에서 뺀다, M7), Embedder가 없을 때의 길(뜻 검색은 "쓸 수 없음"), 다시 색인, 검색 도구가 돌려주는 모양(에이전트가 다시 검색하며 돈다).

## 하지 않는 것

실제 임베딩 공급자(결정 2026-11-16 11번, 사용자 결정: 키워드 · 조건을 먼저, 공급자는 사용자가 고른 뒤 붙인다), sqlite-vec.

## 테스트 주의

"매수" · "손절" 같은 두 글자 검색을 시험한다. 가짜 Embedder는 정해진 벡터를 준다. Embedder가 없으면 바깥으로 나가는 요청이 0건인지 본다(V23).

## 필수 번호

D10 M7 V22 V23

## 1차 검토 참고 (다음 차수 후보)

- `index()`가 조각 저장과 줄 범위 메타(`chunk.<번호>`) 저장 사이에 꺼지면 메타 없는 조각이 남아 `embed_missing`이 영원히 건너뛴다. 공급자를 붙일 때 한 트랜잭션으로 묶거나 복구 길을 만든다(store에 조각 조회 API를 두면 메타 키를 없앨 수 있다).
- 지운 조각의 `chunk.<번호>` 메타가 남는다. 정리 길을 둔다.
- 비밀키 거르기(M7)는 이 모듈이 하지 않는다. 기록을 만드는 쪽(journal · agent)이 가린 글만 넘기는지 server 조립 때 확인한다.
- `search_by_condition`을 자동 회상에 쓰게 되면 출처 등급 거르기를 넣는다.

## 2차 작업 (후보 일괄 처리)

- 메타 빠진 조각 복구: 같은 글을 다시 `index()`하면 빠진 줄 범위 메타를 채운다. `embed_missing`은 메타 없는 조각을 건너뛰되 그 때문에 뒤 조각이 막히지 않게 창을 넓혀 다시 본다(포트는 여전히 한 번). 한 트랜잭션으로 묶는 길은 store에 조각 번호 조회 · 메타 지우기 API가 있어야 한다.
- 지운 조각의 메타 정리: 글이 바뀌어 사라진 조각의 `chunk.<번호>` 메타는 빈 값으로 둔다(빈 값은 "없음"). 키 자체를 지우려면 store API가 필요하다.
- 조건 검색의 자동 회상: `recall_by_condition`을 더해 바깥 자료 조각은 건너뛴다(M7). `search_by_condition`(명시 검색)은 그대로.
- 넘김: 비밀키 거르기(M7)는 기록을 만드는 쪽(journal · agent)이 가린 글만 넘기는지 server 조립에서 확인한다.

## 새 store API 쓰기 (store 12차에서 넘어옴)
- 조각과 메타를 `index_record_with_meta`로 한 트랜잭션에 묶고, 사라진 조각의 메타는 빈 값 대신 `remove_knowledge_meta`로 지운다.

## 4차 작업 (2026-11-21 발표 자료 대조)

(3차는 위 "새 store API 쓰기" 절이다.) 근거: `docs/dev/decisions.md` 2026-11-21 2번 · 4번(임베딩은 Voyage, 키가 없으면 "쓸 수 없음"), `dev.md` D10 · M7("기록 = 매매 기록 · 복기 · 실수 기록 · 시장 메모", "바깥 자료는 자동 회상에 넣지 않는다", "임베딩: 기록이 바뀌면 다시 만든다") · V22 · V23. 아래를 **테스트로 먼저 쓰고** 구현한다. **공개 API는 더하기만 한다**(`TradeRecord` · `RecordText` · `SearchConfig` · `SearchOutput` · `EmbedError`의 칸과 갈래를 바꾸지 않는다 — 잠긴 테스트와 server가 구조체를 글자 그대로 만든다).

### 1. 조건 검색에 실수 유형

- **공개 API**: `Condition`에 칸 `mistake: Option<String>`을 더한다(잠긴 테스트와 server는 모두 `..Condition::default()`로 만들어 깨지지 않는다). `TradeCatalog`에 **기본 구현이 있는** 메서드 `fn mistakes(&self, record_ref: &str) -> Vec<String> { Vec::new() }`를 더한다(`TradeRecord`에 칸을 더하지 않는다 — 잠긴 `tests/suite/common.rs`가 글자 그대로 만든다).
- **규칙**: `mistake`가 `Some(m)`이면 `catalog.mistakes(record_ref)` 가운데 앞뒤 공백을 뗀 글자가 `m.trim()`과 **똑같은** 것이 하나라도 있는 매매만 맞는다(대소문자 그대로, 부분 일치 아님). 글자는 journal 실수의 기계 글자(`chased_entry` · `late_stop` · `oversized_position` · `no_rationale` · `panic_exit`, 자유 글은 그 글)다. 실수 목록이 비면 맞지 않는다. `m.trim()`이 비면 조건이 없는 것으로 본다. 다른 조건(종목 · 기간 · 손익)과는 AND. `search_by_condition` · `recall_by_condition` 둘 다 같은 규칙. 돌려주는 적중은 지금처럼 그 매매의 `trade` 첫 조각이다(V22의 `trade_refs`가 그대로 돈다).
- **넘김**: 날짜(`from` · `to`)와 실수 유형을 검색 도구(`knowledge_search`의 `condition` 모드)에 내보내는 일, server의 `Records`가 `mistakes`를 복기 기록에서 채우는 일은 server 차수에서 잇는다.

### 2. 실수 기록 · 시장 메모 · 바깥 자료를 넣는 길

지금 `index()`는 어떤 종류든 받지만, 실제로 올라오는 것은 server의 `trade` · `review`뿐이다. 종류마다 글 모양과 출처 등급을 이 모듈이 정해 테스트로 묶는다.

- **공개 API** (`RecordText`의 연관 함수로 더한다, 칸은 그대로):
  - `RecordText::mistakes(trade_ref: &str, symbol: &str, mistakes: &[String], note: &str, at) -> Option<RecordText>` — 종류 `Mistake`, 등급 `Team`, 기록 키 = 매매 키. 글은 `실수 · 매매 {trade_ref} · 종목 {symbol}\n실수: {a, b}\n{note}`. 공백을 뗀 실수가 하나도 없으면 `None`(실수 없는 복기는 실수 기록을 만들지 않는다).
  - `RecordText::market_note(note_ref: &str, source: KnowledgeSource, text: &str, at) -> Result<RecordText, Error>` — 종류 `MarketNote`. 등급은 `Mine` · `Team`만 받고 `External`이면 `Error::InvalidRecord`(바깥 자료는 아래 함수로만 들어오게 해 등급이 섞이지 않는다).
  - `RecordText::external(item_ref: &str, title: &str, summary: &str, link: &str, published_at) -> RecordText` — 뉴스 · 공시 요약. 종류 `MarketNote`, 등급 **`External`**. 글은 `{title}\n{summary}\n{link}`. 시각은 공개 시각.
  - 기록 키 모양(예: `news:<공급자>:<ID>`, `dart:<접수번호>`)은 부르는 쪽(server)이 정한다. 이 모듈은 비었는지만 본다(지금 `index()` 규칙).
- **규칙**: 올리는 것은 여전히 `index()` 하나다(앞 8,192자 · 400/80 조각 · 같은 글이면 그대로 · 바뀌면 조각과 임베딩을 지우고 다시). `External` 조각은 `auto_recall`이 참인 검색과 `recall_by_condition`에서 지금처럼 빠진다(M7). 명시 검색(`SearchRequest::keywords`)에서는 나온다.
- **넘김**: 복기 때 `RecordText::mistakes`를 올리는 일, 담당이 쓴 시장 메모를 올리는 일, data(네이버 뉴스 · DART) 결과를 `RecordText::external`로 올리는 일, 자동 회상 종류 목록에 `Mistake` · `MarketNote`를 더하는 일은 server 차수에서 잇는다. 글을 가리는 것도 부르는 쪽이다(1차 검토 참고 그대로).

### 3. 임베더가 붙었을 때 기존 기록의 임베딩을 다시 만드는 길

- **공개 API**: `pub async fn reembed_step(&self, current: &IndexSettings, batch: usize) -> Result<ReembedProgress, Error>`와 `enum ReembedProgress { Unavailable, InProgress { embedded: usize }, Done { embedded: usize, plan: ReindexPlan } }`.
- **규칙** (한 번 부르면 한 걸음 — 끝까지 도는 일 · 쉬는 간격 · 취소는 server의 뒤 작업이 한다):
  1. 포트가 없으면 `Unavailable`. 저장소를 바꾸지 않고 바깥 요청 0건(V23). 남은 설정도 그대로.
  2. `current.model` · `current.dims`가 붙은 포트의 `model()` · `Some(dims())`와 다르면 `Error::Embedder`이고 아무것도 바꾸지 않는다.
  3. `embed_missing(batch)`를 한 번 부른다(포트 호출 많아야 1번 — 지금 약속 그대로). 임베딩은 모델별로 남으므로(`chunks_missing_embedding(model, …)`), 공급자 · 모델이 바뀌면 새 모델 기준으로 모든 조각이 "빠짐"이 되어 차례로 다시 만들어진다. 글이 바뀐 기록(`index()`가 옛 임베딩을 지움)도 같은 길로 다시 만들어진다(M7 "기록이 바뀌면 다시 만든다").
  4. 이번에 둔 것이 있으면 `InProgress { embedded }`. 빠진 조각이 하나도 없으면 그때 **처음으로** `record_settings(current)`를 하고 `Done`(그때의 `plan_reindex(current, true, false)` 결과를 함께 준다).
  5. 포트가 실패하면 `Err`이고 남은 설정은 그대로다. 그래서 도중에 꺼지거나 실패해도 다음 부름이 이어서 하고, 남은 설정은 "다 만들어진 색인"만 가리킨다(원본의 "옆에 새 색인을 만들어 한 번에 바꿔치기"를 줄인 것, 4.3 "줄여서").
  6. 조각 설정 · 색인 방식 · 범위가 바뀐 까닭(`ChunkSettingsChanged` · `TokenizerChanged` · `ScopeChanged`)은 이 함수가 글을 다시 나누지 않는다(이 모듈은 원래 글을 갖고 있지 않다). 그 경우 server가 기록을 `index()`로 다시 올린 뒤 이 함수를 부른다 — server 차수.
- 공급자가 죽었을 때 벡터 색인을 키워드 전용으로 덮어쓰지 않는 규칙(`plan_reindex`의 `SkipProviderUnavailable`)은 그대로다.

### 4. 뜻 검색 몸통 (3번이 쓸모 있으려면 필요하다)

지금 `semantic_search`는 포트가 붙어 있어도 `semantic_not_wired`를 돌려준다. 원본 4.1의 하이브리드를 붙인다.

- **공개 API**: `pub async fn hybrid_search(&self, request: &SearchRequest, now) -> Result<SearchOutput, Error>`를 더한다(종류 · 등급 · `auto_recall` 거르기는 `search`와 같다). `semantic_search(text, now)`는 모양 그대로 두고 `hybrid_search(&SearchRequest::keywords(text), now)`를 부르게 한다. `Embedder` 포트에 **기본 구현이 있는** `async fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError>`(기본은 `embed(&[text])`의 첫 것)를 더한다 — 질의와 색인 글을 공급자가 달리 다룰 수 있게(Voyage `input_type`).
- **규칙** (원본 4.1의 1~9 그대로, 숫자는 지금 `SearchConfig`): 벡터 후보 · 키워드 후보를 각각 `max × candidate_multiplier`개, 키워드 점수 다시 재기(4번 식), 합 = 0.7 × 벡터 + 0.3 × 다시 잰 키워드(가중치는 원본처럼 설정이 아니라 공개 상수 `VECTOR_WEIGHT_MICROS = 700_000` · `TEXT_WEIGHT_MICROS = 300_000`), 최신성 감쇠, 문턱은 감쇠 전 점수로, MMR, 모두 문턱 아래면 키워드 결과로 물러섬. 벡터 점수는 현재 모델의 조각 벡터와 질의 벡터의 비슷함이다(식은 아래 "물을 것" 참고).
- **쓸 수 없을 때는 오류가 아니라 결과다** (원본 4.1 "못 쓸 때"): 포트 없음 → 지금처럼 `no_embedder`. 남은 설정의 모델 · 차원이 포트와 다름(3번이 아직 안 끝남) → 키워드 결과 + `Unavailable { code: "reindexing" }`. 질의 임베딩이 `EmbedError::Unavailable` → 키워드 결과 + `code: "embedder_unavailable"`, `Failed` → `code: "embedder_failed"`. 어느 경우도 `Err`가 아니다.
- **선행 조건**: 조각 벡터를 모델별로 훑을 저장소 조회 API가 지금 없다(`chunk_embedding(id)` 하나씩뿐, 조각 번호 전체를 얻는 길도 없음). store 차수에서 "그 모델의 임베딩이 있는 조각(번호 · 벡터 · 종류 · 등급 · 기록 키 · 시각)" 조회를 먼저 더해야 이 4번의 N1을 시작할 수 있다. 1~3번은 지금 API로 된다.

### 원본 동작 근거

- 3번(다시 만들기 조건 · 죽은 공급자가 덮어쓰지 않음 · 바꿔치기를 줄임) · 4번(하이브리드 식 · "쓸 수 없음" 결과): `docs/research/triggers-approval-memory-deep.md` 4.1 · 4.3.
- 1번(실수 유형 조건): 원본에 없음 — 조건 찾기 자체가 우리 고유(4.3)이고, 실수 유형은 사용자 결정 2026-11-21(발표 자료 대조 뒤 "안되어있는것들 다 구현") 범위.
- 2번(종류별 글 모양 · 바깥 자료 등급): 원본에 없음 — `dev.md` M7의 기록 목록과 출처 등급을 따른다. 뉴스 · 공시는 결정 2026-11-21 3번.

### 테스트로 확인할 것 (가짜 Embedder만 — 바깥 요청 없음)

- 1번: 실수 `late_stop`이 있는 매매만 나옴, 공백 · 대소문자 경계, 실수 없는 매매는 빠짐, 빈 `mistake`는 조건 없음, 종목 · 기간과 AND, `recall_by_condition`도 같고 바깥 자료는 빠짐, 기본 `mistakes`만 있는 카탈로그(잠긴 `common.rs`의 것)에 `mistake` 조건 → 0건. (D10 · V22)
- 2번: 세 함수가 만드는 종류 · 등급 · 글 모양, `mistakes`가 빈 목록 → `None`, `market_note`에 `External` → `InvalidRecord`, `external` 조각은 `auto_recall` 검색에 안 나오고 명시 검색에는 나옴(M7), 같은 뉴스를 두 번 올리면 조각이 그대로. (M7)
- 3번: 포트 없음 → `Unavailable`이고 가짜 포트 호출 0 · 남은 설정 그대로(V23). 모델 A로 다 만든 뒤 모델 B 포트로 바꾸면 모든 조각이 다시 만들어지고 `Done`에서만 설정이 B가 됨. 도중 실패 → 설정이 A 그대로, 다시 부르면 이어서. 글이 바뀐 기록만 다시 만들어짐. 부를 때마다 포트 호출 ≤ 1. `current`와 포트 모델이 다르면 `Error::Embedder`. (D10 · V23 · M7)
- 4번: 뜻이 가까운 조각이 키워드가 안 맞아도 나옴, 0.7/0.3 합, `auto_recall`이면 바깥 자료 빠짐, `reindexing` · `embedder_unavailable` · `embedder_failed`가 오류가 아니라 키워드 결과와 함께 옴, 포트 없을 때 바깥 요청 0. (D10 · V23)

### 잠긴 옛 테스트와 부딪히는 곳

- `tests/suite/common.rs`가 `TradeRecord { … }` · `RecordText { … }`를 글자 그대로 만든다 → 두 구조체에 칸을 더하지 않는다(실수는 `TradeCatalog`의 기본 메서드로, 종류별 길은 연관 함수로).
- `tool.rs`는 포트가 붙은 **키워드** 검색의 `semantic`이 `Available`이기를 기대한다 → `semantic_status()`는 그대로 두고, `reindexing` 등은 `hybrid_search` · `semantic_search`의 결과에만 싣는다.
- `semantic.rs`의 "포트 호출은 한 번에 많아야 한 번"(`embed_missing`)을 `reembed_step`도 지킨다. 포트 없을 때 `semantic_search`가 `no_embedder`인 것도 그대로.
- `Embedder` trait에 메서드를 더할 때는 **기본 구현을 둔다**(잠긴 `FakeEmbedder`가 그대로 컴파일돼야 한다). `Condition`에 칸을 더해도 잠긴 테스트는 모두 `..any_condition()`을 써서 깨지지 않는다.

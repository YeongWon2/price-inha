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
- 끝남: index()가 조각 저장과 줄 범위 메타(chunk.<번호>) 저장 사이에 꺼지면 메타 없는 조각이 남아 embed_missing이 영원히 건너뛴다. 공급자를… — 커밋 e308bae(red) · b80bb03(green) · 전문: `git show 1a73576:docs/dev/modules/knowledge.md`

## 2차 작업 (후보 일괄 처리)
- 끝남: 메타 빠진 조각 복구: 같은 글을 다시 index()하면 빠진 줄 범위 메타를 채운다. embed_missing은 메타 없는 조각을 건너뛰되 그 때문에 뒤 조각… — 커밋 bbb976b(red) · a3851ee(green) · 전문: `git show 1a73576:docs/dev/modules/knowledge.md`

## 새 store API 쓰기 (store 12차에서 넘어옴)
- 조각과 메타를 `index_record_with_meta`로 한 트랜잭션에 묶고, 사라진 조각의 메타는 빈 값 대신 `remove_knowledge_meta`로 지운다.

## 4차 작업 (2026-11-21 발표 자료 대조)
- 끝남: (3차는 위 "새 store API 쓰기" 절이다.) 근거: docs/dev/decisions.md 2026-11-21 2번 · 4번(임베딩은 Voyage, 키… — 커밋 89670c0(red) · 389aa1f(green) · 전문: `git show 1a73576:docs/dev/modules/knowledge.md`

## 5차 작업 (2026-11-27 조정자, 결정 31 — 사용자 "경량화 작업도 해야 해, 임베딩해서 압축한다던가")
- 끝남: 동작 잠금 차수다. 별도 작업 폴더(기능 M, 결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두… — 커밋 91967c5(red) · 4013f9c(green) · 전문: `git show 1a73576:docs/dev/modules/knowledge.md`

## 6차 작업 (2026-11-27 조정자, 백로그 R — 사용자 "백로그 다 실행해" · "남은 것들만 다 실행해 key 제외하고")

동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지 보고 잠근다.

1. **뜻 검색을 하이브리드로 잇기**: `hybrid_search`(지시서 4.1의 1~9 — 키워드 · 벡터 후보 · 가중 합) · `Embedder::embed_query`(기본 구현 있음). `semantic_search`가 이것을 부른다.
2. **시장 메모 색인**: 정산된 결정 메모의 교훈을 `market_note`로.

- 원본 동작 근거: `docs/research/openclaw.md` 3.6(벡터 ∥ 키워드 → 가중 합) · 지시서 4.1.
- 시험으로 확인할 것: `tests/hybrid.rs`가 합친 구현에서 통과하고, 기존 시험도 모두 통과한다.

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| 새 시험 `tests/hybrid.rs` | 유지 | 작업 폴더에서 red 커밋으로 먼저 쓰고 green 커밋으로 통과 |
| 그 밖의 knowledge 시험 | 유지 | 동작이 바뀌지 않는다 |

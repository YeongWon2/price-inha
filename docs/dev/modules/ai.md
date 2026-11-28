# ai 모듈 (T3)

크레이트 `pi-ai`, 폴더 `crates/ai/`. 구성 요소 A11 AI 공급자 어댑터. `pi-secrets`만 쓸 수 있다.
근거: `dev.md`의 D4 · A11 · V31 · E38.

## 원본 동작 근거

- 대조표: `docs/dev/conformance.md` 1장 "AI 공급자 어댑터".
- 따르는 원본: Hermes의 공급자 구조와 Anthropic API 키 인증 — `docs/research/hermes-agent.md` 4.1 · 4.3. 구독은 OpenClaw의 `claude-cli` 백엔드 — `docs/research/cli-backend.md` 2장 · 4장 · 5장.
- 의도해서 다른 것: 구독 OAuth 토큰을 API에 직접 보내지 않는다(Anthropic 금지, `docs/dev/decisions.md` 2026-11-14). CLI는 오래 사는 프로세스 · 세션 이어 가기 · MCP 도구 없이 "도구 없는 한 번 호출"만 쓴다(`cli-backend.md` 5.1, 결정 2026-11-15).
- 모르고 달랐던 것: `Retry-After` 읽는 법과 상한, 시간 초과 · 연결 실패의 다시 시도, 다시 시도 간격과 jitter, 연결 시간 제한 — 3차에서 원본대로 고친다. 구독 OAuth는 만들지 않는다(2026-11-15 결정).

## 만드는 것

1. **공급자 trait**: 비동기 `complete(요청) → 응답`. 요청은 모델 이름, 시스템 글, 대화(역할 · 글), 최대 토큰. 응답은 글, 멈춘 이유, 쓴 토큰 수(입력 · 출력). 에이전트 실행기(나중 작업)는 이 trait만 안다.
2. **자격 한 자리 (D4)**: API 키가 끼워지는 자리(`Credential`). 구독은 자격이 아니라 공급자(2차의 CLI 공급자)로 고른다. 공급자는 `pi-secrets`에서 자격을 읽는다. 자격을 요청 · 응답 · 오류 · 기록에 담지 않는다.
3. **Anthropic 공급자**: Messages API를 `reqwest`로 직접 부른다(호출마다 프로세스를 띄우지 않는다). 기본 주소는 설정으로 바꿀 수 있다(테스트는 로컬 가짜 서버). 헤더와 본문 모양은 **공식 문서를 확인하고** 맞춘다. 기억으로 짓지 않는다.
   - API 키는 `x-api-key`로 보낸다.
   - 모델 이름은 설정에서 받는다. 라이브러리 안에 기본 모델을 박지 않는다.
4. **자격 없음 (V31)**: 키가 등록되지 않았으면 바깥으로 요청을 보내지 않고 "설정 안 됨" 오류를 낸다.
5. **오류 구분**: 인증 실패(401 · 403), 한도 초과(429, `retry-after`가 있으면 그 값), 공급자 장애(5xx · 529), 잘못된 요청(400), 시간 초과, 연결 실패. 시간 제한은 설정값이다.
6. **다시 시도**: 한도 초과와 장애는 정해진 횟수(기본 2번)까지 간격을 늘려 다시 시도한다. 인증 실패와 잘못된 요청은 다시 시도하지 않는다. 테스트에서 간격을 0에 가깝게 줄일 수 있다.
7. **키 확인기**: `pi-secrets`의 확인기 trait을 구현해 AI 키가 실제로 닿는지 확인한다(가장 싼 요청 한 번).

## 3차 작업 (원본 대조의 "모르고 다름" + 1 · 2차 AI 검토의 참고 — 2026-11-15 사용자 "다 개선")
- 끝남: 근거: docs/dev/conformance.md A15 · A17 · A18 · A19(모르고 다름 8 · 9 · 10 · 16번), docs/research… — 커밋 179ffb9(red) · 98ac220(green) · 전문: `git show 1a73576:docs/dev/modules/ai.md`

## 4차 작업 (3차 검토의 참고 + gate 6차 검토 참고 2 — 2026-11-15 결정)
- 끝남: 아래를 테스트로 먼저 쓰고 구현한다. 공개 API는 더하기만 한다. 2 · 3 · 4번은 docs/dev/decisions.md 2026-11-15의 조정자 추… — 커밋 a685804(red) · 5c89062(green) · 전문: `git show 1a73576:docs/dev/modules/ai.md`

## 5차 작업 (4차 검토의 참고 + gate 7차 검토 참고 1 — 2026-11-15 조정자 결정, 사용자 "알아서 개선")
- 끝남: 아래를 테스트로 먼저 쓰고 구현한다. 공개 API는 더하기만 한다. — 커밋 b6392e5(red) · 68ce6d5(green) · 전문: `git show 1a73576:docs/dev/modules/ai.md`

## 6차 작업 (5차 검토의 참고 — 2026-11-15 조정자 결정)
- 끝남: 정상으로 끝난 확인 · 호출의 남은 손주도 끝까지 끝낸다: 그룹에 SIGTERM을 보낸 뒤 끝낼 때의 유예가 지나도 그룹이 남아 있으면 SIGKILL을 보낸다(… — 커밋 11967fd(red) · 68b8332(green) · 전문: `git show 1a73576:docs/dev/modules/ai.md`

## 7차 작업 (6차 검토의 참고 + 로드맵 W1 — 2026-11-16 결정)
- 끝남: 근거: dev.md E38 · V47 · D24, 원본 근거 docs/research/cli-backend.md 5.6, docs/research/hermes-… — 커밋 77b4b54(red) · 6e03aac(green) · 전문: `git show 1a73576:docs/dev/modules/ai.md`

## 8차 작업 (후보 일괄 처리 — 2026-11-16 사용자 "다 처리해")
- 끝남: terminate가 남은 손주를 기다리는 도중 취소돼도(끝내기와 띄우기가 겹치고 호출이 버려질 때) Drop이 그룹이 남아 있으면 SIGKILL을 보낸다: 기다… — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/ai.md`

## 9차 작업 (Linux CI에서 드러난 띄우기 실패 — 2026-11-17 조정자 결정, 사용자 "알아서 추천대로")
- 끝남: D1(모든 운영체제)에 따라 Linux에서도 같아야 한다. CI 진단에서 40번 중 4번, 실행 파일을 띄울 때 ETXTBSY(io::ErrorKind::Exe… — 커밋 0a4b12f(red) · c94036e(green) · 전문: `git show 1a73576:docs/dev/modules/ai.md`

## 2차 작업: 구독은 공식 Claude Code CLI로 (CLI 백엔드)
- 끝남: 근거: 사용자 결정(docs/dev/decisions.md 2026-11-15), docs/research/cli-backend.md 5장. 원본은 OpenCl… — 커밋 5793154(red) · cff3042(green) · 전문: `git show 1a73576:docs/dev/modules/ai.md`

## pi-secrets와 잇는 법

`pi-ai`은 `pi-secrets`에 직접 기대지 않는다. 이 모듈 안에 자격을 주는 작은 trait(`CredentialSource`)을 두고 그것만 안다.
`pi-secrets`로 그 trait을 구현하는 일과 AI 키 확인기를 `pi-secrets`의 확인기 trait에 잇는 일은 조립 단계(`price-inha`)에서 한다.
이 모듈에서는 "이 자격으로 닿는지 확인하는 함수 `verify`"까지만 만든다. 위 본문에서 "`pi-secrets`에서 읽는다"는 말은 이 trait을 거친다는 뜻이다.

## 10차 작업 (2026-11-21 발표 자료 대조 — 로컬 임베딩)
- 끝남: 근거: docs/dev/decisions.md 2026-11-21 2번(공급자 확장 구조: trait 하나 + 등록부, "구조는 다, 구현은 하나씩") · 15… — 커밋 6928536(red) · 9221ac3(green) · 전문: `git show 1a73576:docs/dev/modules/ai.md`

## 하지 않는 것

OAuth 로그인 절차와 구독 토큰을 API에 보내는 길은 만들지 않는다(구독은 2차의 CLI 공급자로만 쓴다). 도구 호출, 스트리밍, 다른 AI 공급자는 나중 작업이다. 임베딩은 10차의 Ollama(로컬) 하나만 만들고 다른 임베딩 공급자(Voyage 등)는 어댑터만 나중에 더한다.

## 테스트 주의

로컬 가짜 서버(`wiremock` 같은)만 쓴다. 가짜 서버가 받은 요청과 이 모듈이 낸 오류 · `Debug` 출력에 시험용 키 글자가 본문이나 기록으로 새지 않는지 확인한다(인증 헤더는 제외).

## 필수 번호

D4 V31 E38 V47 D24 D10 V23

## 11차 작업 (2026-11-27 조정자, 결정 34 — 사용자 "백로그 다 실행해" · "남은 것들만 다 실행해 key 제외하고")

동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지 보고 잠근다.

1. **사설망 Ollama는 켜야만**(결정 34): `OllamaEmbeddingProvider::on_private_network` · `ollama_reach` — 루프백과 사설망(10/8 · 172.16/12 · 192.168/16 · 169.254/16 · fc00::/7 · fe80::/10, `*.local`은 사설망 주소로 풀릴 때만)만 받는다. 지금 길(`new` · `with_limits`)은 그대로 루프백만.

- 원본 동작 근거: 결정 34(조정자 추천안 — 원본에 없음, V23의 '바깥으로 나가지 않는다'를 사설망까지만 넓힌다).
- 시험으로 확인할 것: `tests/ollama11.rs`가 합친 구현에서 통과하고, 기존 시험도 모두 통과한다.

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| 새 시험 `tests/ollama11.rs` | 유지 | 작업 폴더에서 red 커밋으로 먼저 쓰고 green 커밋으로 통과 |
| 그 밖의 ai 시험 | 유지 | 동작이 바뀌지 않는다 |

# 가격인하 작업 지도

AI 에이전트 기반 자율 주식 투자 시스템. 서버는 Rust 한 프로그램, 화면은 웹이다.
이 파일은 지도다. 자세한 내용은 아래 문서로 간다.

| 알고 싶은 것 | 문서 |
|---|---|
| 무엇을 왜 만드나 | `docs/product/stock-team/prd.md` |
| 화면과 흐름 | `docs/product/stock-team/spec.md`, `flow.md` |
| 구조 · 계약 · 오류 · 작업 · 검증 번호 | `docs/product/stock-team/dev.md` |
| 원칙 P1~P22 | `docs/product/principles.md` |
| 구현 파이프라인 (반드시 따른다) | `docs/dev/pipeline.md` |
| 아키텍처 · 컨벤션 규칙 (기계가 검사) | `docs/dev/architecture.toml` |
| 모듈별 작업 지시서 | `docs/dev/modules/<모듈>.md` |
| 원본(Hermes · DeskRPG · OpenClaw)의 동작 조사 | `docs/research/` |
| 만든 것과 원본 동작의 대조표 | `docs/dev/conformance.md` |
| 토스증권 API 요약 | `docs/dev/toss-api-notes.md` |
| 준비해야 하는 키 · 토큰 · 계정 | `docs/dev/keys-needed.md` |

## 반드시 지키는 것

1. 구현은 `docs/dev/pipeline.md`의 순서로만 한다: 테스트 먼저(red) → 구현(green) → 기계 검사 → AI 검토 → 사람 확인.
2. 끝났다는 기준은 `scripts/verify.sh`가 0으로 끝나는 것이다. 말로 끝났다고 하지 않는다.
   G2 · push 전 검사는 영향 범위(바뀐 · 기대는 · 돈 · 안전 크레이트와 바뀐 화면의 시험, 정적 검사는 늘 전부)와 캐시(같은 입력 지문으로 통과한 검사는 다시 돌리지 않음)로 돌고, 건너뛴 것은 출력에 밝힌다. 릴리스 앞에는 직전 릴리스 태그 뒤로 바뀐 것의 영향 범위가 돈다(`scripts/release.sh` — 직전 태그가 없으면 전체, `docs/dev/pipeline.md` 5장). 캐시 없이 전부 보려면 `scripts/verify.sh --full --no-cache`.
   지난 차수의 자세한 글은 git 기록이 맡는다 — 검토를 통과하면 `pipeline.py tidy`가 지시서의 옛 차수 절을 한 줄로 줄인다(11장).
3. red 뒤에는 `crates/<모듈>/tests/`를 고치지 않는다. 테스트가 틀렸으면 멈추고 알린다.
4. 비밀번호 · 토큰 · API 키 · 계좌번호를 코드 · 테스트 · 문서 · 기록 어디에도 넣지 않는다.
5. 테스트는 실제 증권사 · AI 서버를 부르지 않는다. 로컬 가짜 서버만 쓴다.
6. 계좌는 직접 만든 모의계좌(`pi-paper`)뿐이다. 실제 증권사로 주문을 보내는 코드를 만들거나 실행하지 않는다.
7. 모듈은 자기 폴더(`crates/<모듈>/`)만 고친다. 구조와 컨벤션은 `docs/dev/architecture.toml`을 따른다.
   `verify.sh`가 어긴 곳을 `[규칙 번호] 파일:줄 · 왜 · 고치기`로 알려 주면 그대로 고친다. 규칙과 검사 스크립트는 고치지 않는다.
8. 원본에서 가져오는 기능은 **원본과 같은 동작**으로 만든다. 지시서의 "원본 동작 근거"가 가리키는 조사 문서를 읽고 따른다. 원본과 다르게 만들려면 까닭(사용자 결정 · 스펙 번호)이 있어야 한다. 원본의 코드는 옮기지 않는다(동작만 같게).
9. 스펙과 다르게 만들어야 하면 임의로 바꾸지 말고 멈추고 묻는다.
10. 커밋 메시지와 사용자에게 하는 말은 한국어. push와 force push는 사용자가 말할 때만.

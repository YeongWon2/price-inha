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
| 모듈별 작업 지시서 | `docs/dev/modules/<모듈>.md` |
| 토스증권 API 요약 | `docs/dev/toss-api-notes.md` |

## 반드시 지키는 것

1. 구현은 `docs/dev/pipeline.md`의 순서로만 한다: 테스트 먼저(red) → 구현(green) → 기계 검사 → AI 검토 → 사람 확인.
2. 끝났다는 기준은 `scripts/verify.sh`가 0으로 끝나는 것이다. 말로 끝났다고 하지 않는다.
3. red 뒤에는 `crates/<모듈>/tests/`를 고치지 않는다. 테스트가 틀렸으면 멈추고 알린다.
4. 비밀번호 · 토큰 · API 키 · 계좌번호를 코드 · 테스트 · 문서 · 기록 어디에도 넣지 않는다.
5. 테스트는 실제 증권사 · AI 서버를 부르지 않는다. 로컬 가짜 서버만 쓴다.
6. 실제 돈이 움직이는 주문 코드를 실행하지 않는다.
7. 모듈은 자기 폴더(`crates/<모듈>/`)만 고친다. 의존 방향은 `docs/dev/pipeline.md`의 표를 따른다.
8. 스펙과 다르게 만들어야 하면 임의로 바꾸지 말고 멈추고 묻는다.
9. 커밋 메시지와 사용자에게 하는 말은 한국어. push와 force push는 사용자가 말할 때만.

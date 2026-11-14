# 가격인하 (PriceInha)

> 가격은 인하, 수익은 인상.

사람이 한도를 정하고, AI 팀이 판단하고, 자동 규칙이 지키는 **AI 에이전트 기반 자율 투자 시스템**입니다.
인하대학교 소프트웨어융합공학과 2026-2학기 SW융합프로젝트로 만듭니다.

## 차별점

| 차별점 | 기존 방식 | 가격인하 |
|---|---|---|
| 1. 복기가 규칙이 됨 | 반성이 기억에만 남거나, 사람이 규칙을 직접 고침 | 복기를 쌓아 규칙 개선안을 만들고, 승인 후 반영해 같은 실수를 막음 |
| 2. 규칙 안에서만 움직임 | 백테스트 · 교육용 중심, 실전 규칙 강제 없음 | 모의투자로 검증된 템플릿만 실전에 쓰고, 규칙 엔진이 강제함 |
| 3. 일하는 모습이 보임 | 결과와 텍스트 로그만 제공 | 에이전트가 묻고 답하고 회의하는 모습을 사무실 화면으로 보여 줌 |

## 진행 상황

지금은 **계획 단계**입니다. 개발은 명세를 먼저 쓰고 구현합니다.

| 단계 | 시기 | 상태 |
|---|---|---|
| 조사 · 구상 | 10월 | 완료 |
| 기능 명세 | 11월 초 | 완료 (기획 PRD · 상세 스펙) |
| 개발 스펙 명세 | 11월 중순 | 완료 |
| 구현 ① AI 에이전트 연결 | 11월 중순 ~ | 예정 |
| 구현 ② 모의투자 · 템플릿 · 복기 | 11월 말 ~ | 예정 |
| 구현 ③ 실전 운용 · 시각화 | 12월 초 ~ | 예정 |
| 테스트 | 11월 말 ~ 12월 중순 | 예정 |
| 결과 정리 · 최종 발표 | 12월 중순 ~ 말 | 예정 |

## 정한 것

- [기본 원칙 16개](docs/product/principles.md): 규칙 엔진에 박는 바꿀 수 없는 원칙과 멈춤 · 재개 규칙
- [stock-team 기획](docs/product/stock-team/prd.md): 주식 투자 에이전트 팀 PRD · [상세 스펙](docs/product/stock-team/spec.md) · [유저 플로우](docs/product/stock-team/flow.md) · [개발 스펙](docs/product/stock-team/dev.md)
- 개발 방식: 명세 먼저 작성 후 구현
- 실전은 검증된 템플릿 안에서만 운용

## 참고

- TradingAgents (arXiv 2412.20138) · AI Hedge Fund (github.com/virattt/ai-hedge-fund) · FinMem (arXiv 2311.13743)
- 시각화: Generative Agents (arXiv 2304.03442) · AI Town (github.com/a16z-infra/ai-town) · ChatDev (github.com/OpenBMB/ChatDev)

## 리버스 엔지니어링 자료

서버는 Rust로 다시 만들고 3D 사무실 화면만 웹으로 남깁니다. 아래 두 오픈소스에서 가격인하에 필요한 기능만 골라, 원본과 같은 동작으로 재현하기 위한 구조를 정리했습니다.

- [Hermes Agent](docs/research/hermes-agent.md) — 에이전트 실행기 (MIT)
- [DeskRPG](docs/research/deskrpg.md) — 3D 가상 사무실 (Sustainable Use License: 개인·비상업 용도, 고지 유지)
- [DeskRPG Hermes 플러그인](docs/research/deskrpg-hermes-plugin.md) — DeskRPG와 Hermes를 잇는 플러그인의 동작 명세 (LICENSE 없음: 코드는 쓰지 않고 명세만 참고)
- [OpenClaw](docs/research/openclaw.md) — Hermes에서 부족한 트리거 · 승인 한도 · 장기 기억 · OAuth 보완 (MIT)
- [원격 접속 보안](docs/research/remote-access-security.md) — Tailscale · TOTP · 승인 보호 · 비밀 저장 · 암호화 백업

## 투자 원칙 조사

- [주식 투자 철학 · 원칙](docs/research/stock-investing-principles.md) — 학파별 원칙, 공통 리스크 원칙, 자동매매 위험, 스타일별 차이, 국내 · 미국 시장 규칙, 가격인하 기본 원칙 후보 (특정 종목 추천이나 투자 조언이 아닌 시스템 설계용 정리)

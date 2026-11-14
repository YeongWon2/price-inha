# 토스증권 Open API 요약

공식 문서에서 2026-11-14에 확인한 내용이다. 정확한 모양은 항상 공식 명세가 기준이다.

- 안내: https://developers.tossinvest.com/llms.txt
- REST 명세(OpenAPI 3.1, v1.2.28): https://openapi.tossinvest.com/openapi-docs/latest/openapi.json
- 실시간 명세(AsyncAPI): https://openapi.tossinvest.com/openapi-docs/latest/asyncapi.json
- 개요(한도 · 오류 · 웹소켓): https://openapi.tossinvest.com/openapi-docs/overview.md

## 확인한 사실

| 항목 | 내용 |
|---|---|
| 기본 주소 | `https://openapi.tossinvest.com` |
| 인증 | OAuth 2.0 Client Credentials. `POST /oauth2/token`, `application/x-www-form-urlencoded`, `grant_type=client_credentials` · `client_id` · `client_secret` |
| 토큰 응답 | `access_token`(JWT) · `token_type`("Bearer") · `expires_in`(초). refresh token 없음 |
| 토큰 주의 | client마다 유효한 토큰은 1개. 다시 발급하면 이전 토큰은 바로 무효(`token-revoked`). 필요할 때만 다시 발급한다 |
| 허용 IP | 등록하지 않은 IP는 403 `ip-not-allowed` (토큰 발급은 OAuth 형식 `access_denied`) |
| 모의투자 | 없다. 주문 API는 실제 계좌에 실제 주문을 낸다 → 가격인하는 자체 모의계좌를 쓴다 |
| 현재가 | `GET /api/v1/prices?symbols=005930,000660` (최대 200개, 콤마). 한도 그룹 `MARKET_DATA` 초당 15회 |
| 현재가 응답 | `{"result":[{"symbol","timestamp"(없을 수 있음),"lastPrice"(문자열 십진수),"currency"("KRW"|"USD")}]}` |
| 국내 장 시간 | `GET /api/v1/market-calendar/KR?date=YYYY-MM-DD`. `result.today / previousBusinessDay / nextBusinessDay` 각각 `date`, `integrated{preMarket, regularMarket, afterMarket}`(휴장이면 null). 세션은 `startTime` · `endTime`(KST) |
| 미국 장 시간 | `GET /api/v1/market-calendar/US?date=`. 날마다 `dayMarket, preMarket, regularMarket, afterMarket`(휴장이면 null). 시각은 KST |
| 장 시간 한도 | 그룹 `MARKET_INFO` 초당 3회 |
| 한도 헤더 | `X-RateLimit-Limit` · `X-RateLimit-Remaining` · `X-RateLimit-Reset`, 429에는 `Retry-After`(초) |
| 오류 모양 | `{"error":{"requestId","code","message","data"?}}`. 예: `invalid-token`, `expired-token`, `token-revoked`, `rate-limit-exceeded`, `maintenance` |
| 실시간 | `wss://openapi-ws.tossinvest.com/ws/v1`, 같은 토큰. 계정마다 연결 2개, 연결마다 구독 100건. 이번 기반 작업에서는 쓰지 않는다 |

## 이번에 쓰지 않는 것

- 주문 · 계좌 · 보유 API (T7 이후, 그리고 실전 주문은 승격 뒤에만).
- 웹소켓 실시간 (REST로 주기적으로 받는 것부터 한다).

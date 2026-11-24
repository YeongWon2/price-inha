# 투자 AI 에이전트 팀 3종 조사 (TradingAgents · ai-hedge-fund · FinRobot)

조사 방식: 공개 저장소를 `git clone --depth 1`로 받아 읽기와 정적 분석만 했다(받은 코드는 실행하지 않았고 install도 하지 않았다). 코드는 옮기지 않고 동작 · 수치 · 근거(파일:줄)만 적는다.
받은 위치: `.../scratchpad/refs/src2/<이름>/` (아래 "조사 대상 판" 참고).

## 0. 먼저 알아야 할 것: 두 저장소의 최신판은 예전 판과 전혀 다른 물건이다

| 저장소 | 최신(받은 HEAD) | 사용자가 말한 구조(토론 · 웹앱)가 있는 판 | 이 문서에서 쓴 이름 |
|---|---|---|---|
| TauricResearch/TradingAgents (Apache-2.0) | `81501e0`, v1.0.0 (2026-11-24). 파이썬 토론 그래프가 사라지고 **TypeScript 터미널 채팅 에이전트**로 다시 만들어짐. 에이전트 4명(Equity · Macro · Quant · Risk)이 병렬로 조사해 보고하고 **주문은 내지 않는다**(`README.md` "does not place trades") | **v0.7.1**(파이썬). 분석가 4 → 강세/약세 → 리서치 매니저 → 트레이더 → 리스크 3인 → 포트폴리오 매니저 | `TA v0.7.1`(토론 구조), `TA v1.0.0`(현재 화면 · 한도) |
| virattt/ai-hedge-fund (MIT) | `5c553b5`, v2.7.2. `src/`와 `app/`(웹앱)이 **삭제됨**. 새 엔진 `hedge_fund/` + 텍스추얼(Textual) 터미널 앱(`aihf`) | **v2.0.0**(`src/` 투자자 에이전트 + `app/` React Flow 웹앱) | `AHF v1`(=v2.0.0 태그: 신호 · 웹앱), `AHF v2`(=HEAD: 신호 값 · 점수표 · 승인 단계) |
| AI4Finance-Foundation/FinRobot (Apache-2.0) | `63e1884`. V0(autogen) · V1(equity 웹) · V2(`finrobot_desktop`, PydanticAI + React/Tauri) 3층 | 그대로 | `FinRobot` |

그래서 `TradingAgents-v0.7.1/`, `ai-hedge-fund-v2.0.0/`를 각각 새 빈 폴더에 `--branch <태그>`로 한 번 더 받았다(역시 신뢰하지 않는 자료로 읽기만 함). 아래 근거 표기는 `TA07:`(TradingAgents v0.7.1 `tradingagents/` 기준), `TA1:`(v1.0.0), `AHF1:`(v2.0.0), `AHF2:`(HEAD `hedge_fund/`), `FR:`(`finrobot_desktop/`).

**한 줄 결론**: 세 저장소 어디에도 "리스크 담당의 거부권"은 없다. 리스크는 (a) 토론 참가자 의견(TA), (b) 결정론적 **한도 계산**(AHF)이다. 우리 "리스크 담당 거부권"은 우리가 정하는 규칙이고, 원본에서 가져올 수 있는 것은 "한도를 얼마나 깎았는지 전 → 후를 기록해 보여 주는 방식"이다(F-3).

---

## A. 팀 구조와 논의 흐름

### A-1. TradingAgents v0.7.1 (토론 그래프, 가장 정통)

차례(그래프 간선, `TA07: graph/setup.py:157-185`):

1. 분석가 4명(시장 · 소셜/감성 · 뉴스 · 펀더멘털)이 **동시에** 각자 보고서를 쓴다. 각자 도구 호출은 개인 메시지 기록에서 따로 돈다(`:53-90`). 선택한 분석가만 넣을 수 있다(`:110-122`). 같은 시점에 "Memory Log" 노드가 지난 결정을 정산한다(`:158-166`).
2. 모든 분석가가 끝나야 **강세 연구원(Bull)이 먼저** 말한다(`:166`) → 약세(Bear) → … 번갈아.
3. 토론이 끝나면 **리서치 매니저**가 5단계 평가(Buy/Overweight/Hold/Underweight/Sell) + 근거 + 실행 지침을 낸다(`TA07: agents/schemas.py:95-143`).
4. **트레이더**가 그 계획을 거래 제안(Buy/Hold/Sell 3단계 + 진입가 · 손절가 · 비중, 셋 다 선택값)으로 바꾼다(`schemas.py:76-87, 146-185`).
5. **리스크 3인**(공격 → 보수 → 중립 순)이 트레이더 제안을 놓고 서로 반박한다(`:176-183`, 순서는 `graph/conditional_logic.py:29-33`).
6. **포트폴리오 매니저**가 최종 결정(5단계 등급 + 요약 + 논지)을 낸다(`:185`).

토론 라운드 수와 끝나는 조건(`graph/conditional_logic.py:12-33`, 기본값 `default_config.py:133-134`):
- 강세/약세: `count >= 2 * max_debate_rounds`이면 리서치 매니저로. 기본 `max_debate_rounds=1` → **강세 1번 + 약세 1번만** 말하고 끝. 직전 발언자가 Bull이면 다음은 Bear, 아니면 Bull.
- 리스크: `count >= 3 * max_risk_discuss_rounds`이면 포트폴리오 매니저로. 기본 1 → **3명이 한 번씩**.
- 분석가 도구 호출 상한 `max_tool_rounds=20`. 넘으면 "이제 보고서를 쓰라"는 문구(`WRAP_UP`)를 강제로 넣어 끝낸다(`graph/setup.py:73-83, 87-89`; `default_config.py:137`). 도구를 끝없이 부르는 모델이 그래프를 멈추지 못하게 하려는 장치.
- 토론 상태에는 `bull_history`, `bear_history`, 전체 `history`, 직전 응답 `current_response`, 발언 수 `count`만 있다(`agents/state.py:8-17`). 리스크는 `latest_speaker`와 3명 각자의 직전 응답(`:21-42`).

토론 방식: 각 연구원은 네 보고서 + 지금까지 토론 기록 + **상대의 마지막 주장**을 받아 상대 논점에 직접 반박하는 대화체로 쓴다(`agents/researchers/bull_researcher.py:33-52`, 프롬프트 "debating effectively rather than just listing data"). 상대 발언이 없으면 "opening"으로 시작(`opponent_argument_or_opening`).

**거부권**: 없다. 리스크 3인은 의견만 내고, 포트폴리오 매니저가 읽고 결정한다. 포트폴리오 매니저 프롬프트는 "충돌만으로는 Hold의 이유가 안 된다. 더 강한 쪽을 택하라"고 못 박는다(`agents/managers/portfolio_manager.py` 프롬프트, 같은 문구가 `schemas.py:104-111, 230-238`). 결정을 읽지 못하면 Hold로 두지 않고 `REVIEW`(사람 확인)로 표시한다(`agents/rating.py` 머리말 1-12, `RATING_REVIEW`).

최종 결정 형식(`schemas.py:221-262`): `rating`(5단계) · `executive_summary`(2~4문장: 진입 · 비중 · 위험 수준 · 기간) · `investment_thesis`(근거 중심 상세) · `price_target`(선택) · `time_horizon`(선택). **숫자 확신도 칸은 없다.** 확신도는 감성 분석가에게만 있다(`SentimentReport`: 6단계 band + 점수 0~10 + `confidence` low/medium/high, `schemas.py:296-363`).

### A-2. TradingAgents v1.0.0 (현재 HEAD)

토론이 없다. 메인 에이전트가 계획 → 데이터 도구 → 계산 도구, 깊이가 필요하면 전문 에이전트 4명(Equity Research · Macro Strategy · Quant Research · Risk Analysis, `specs/agents/*.md`)에게 각자 맥락으로 위임해 **병렬** 조사(`CHANGELOG.md` 1.0.0). 에이전트 한 명의 한도는 `steps`(리스크는 12, `specs/agents/risk.md` 머리), 도구 호출은 한 단계에 4개 동시 · 16개 상한(`TA1: src/contracts/limits.ts` CALLS). 모든 수치에 출처 번호를 달고 답 끝에 번호 매긴 출처 목록을 붙인다(`README.md` Usage; `src/cli/Transcript.tsx:279-286`). 계획 모드(Shift+Tab): 연구 계획을 먼저 보여 주고 고친 뒤 실행.

### A-3. ai-hedge-fund v1 (v2.0.0)

그래프(`AHF1: src/main.py:103-129`): `start` → 선택한 **투자자 에이전트들 병렬**(버핏 · 그레이엄 · 애크먼 · 우드 · 멍거 · 번 · 파브라이 · 린치 · 피셔 · 드러켄밀러 · 다모다란 · 탈레브 · 준준왈라 + 기능 분석가 4: fundamentals · growth · news_sentiment · sentiment) → **리스크 매니저 1개** → **포트폴리오 매니저** → 끝. 토론 없음, 한 방향 흐름.
- 리스크 매니저는 LLM이 아니다. 종목별 연환산 변동성으로 비중 한도를 정한다: 변동성 15% 미만 최대 25%, 15~30% 20%→12.5%, 30~50% 15%→5%, 50% 이상 최소(`src/agents/risk_manager.py:270-292`), 거기에 활성 포지션과의 평균 상관 배수(0.8↑ 0.70, 0.6↑ 0.85, 0.4↑ 1.00, 0.2↑ 1.05, 그 미만 1.10; `:309-317`)를 곱해 달러 한도를 만든다(`:162-171`). 결과는 `remaining_position_limit`.
- 포트폴리오 매니저는 현금 · 한도 · 포지션 · 마진으로 **허용 행동과 최대 수량을 코드로 먼저 계산**(`compute_allowed_actions`, `portfolio_manager.py` :98 이후)하고, LLM은 그 안에서 고른다. 즉 한도는 거부권이 아니라 **한계선**이다. 결정 형식(`:13-21`): `action`(buy/sell/short/cover/hold) · `quantity` · `confidence`(0~100 정수) · `reasoning`.
- 신호가 없는 분석가는 `neutral, 0`으로 대체(`ben_graham.py:340`).

### A-4. ai-hedge-fund v2 (HEAD)

`FUND = 전략들에 자본을 나눔 → STRATEGY = 모델들의 혼합 → MODEL = 신호 하나`(`hedge_fund/README.md` 상단). 사람 흐름: `데이터 → 알파 모델 → 포트폴리오 → 리스크 → 체결 → 장부(ledger)`. 결정은 T일 종가에 내리고 T+1 종가에 체결(백테스트 · 모의 동일). 리스크 한도가 걸릴 때마다 `ClampEvent{limit, ticker, before, after}`로 기록해 "왜 줄었는지" 설명 가능(`AHF2: risk/limits.py:97-107`; 종류 9가지: max_position_pct · gross · net · drawdown · halve_drawdown · adv · beta · sector · volatility). 어떤 실패라도 펀드를 **halt**(킬 스위치)시키고 사람이 풀어야 한다(`paper/tick.py:17-18, 176`).

### A-5. FinRobot V2

리드 에이전트가 Data → Analysis → Modeling → Synthesis → Report 5단계 파이프라인을 돌린 뒤, **Bull ↔ Bear → Judge**(`README.md:111-122`). 구현은 단발: 강세/약세 각각 **주장 최대 5개**(하한 없음, 주장마다 근거 id 필수)를 내고, 재판관(Judge)이 두 목록과 "엇갈린 지점"을 받아 판정(`finrobot/engine/agents/instructions/{bull,bear,judge}_agent.md`). 반론 왕복 라운드는 없다. 판정 형식: `call`(BUY/HOLD/SELL, 반드시 방향을 정함. "기권/REVIEW 없음") · `conviction`(0~1) · `swing_factor`(승패를 가르는 핵심 가정 한 문장) · `change_my_mind`(무엇이 바뀌면 판정이 뒤집히는지 한 문장). 불확실하면 거부하지 말고 확신도를 낮추라고 지시. 재판관은 숫자를 인용하지 않는다(숫자는 근거 id로만).

---

## B. 사용자에게 보여 주는 것

### B-1. TA v0.7.1 터미널 (`cli/display.py`, rich Layout)

화면 4구획(`:161-171`): 위 `header`(높이 3, 환영 패널) / 가운데 왼쪽 `progress`(비율 2) + 오른쪽 `messages`(3) 합이 `upper`(3), 그 아래 `analysis`(5) / 아래 `footer`(높이 3).
- **진행 표**(`:197-261`): 열 `Team · Agent · Status`. 팀 5개(Analyst Team / Research Team / Trading Team / Risk Management / Portfolio Management) 아래 에이전트가 줄지어 있고, 상태는 `pending`(노랑) · `in_progress`(파랑 스피너) · `completed`(초록) · `error`(빨강). 선택하지 않은 분석가는 줄에서 뺀다.
- **메시지 · 도구 로그**(`:270-312`): `Time · Type · Content`. 도구 호출과 메시지를 한 표에.
- **현재 보고서**(`:320-332`): 지금 쓰이고 있는 보고서 구획을 마크다운으로.
- **하단 통계 줄**(`:353-376`): `Agents 3/12 | LLM: n | Tools: n | Tokens: 1.2k↑ 3.4k↓ | Reports 2/7 | ⏱ 02:31`. 토큰을 아직 못 받으면 `Tokens: --`.
- **끝난 뒤 보고서**(`:395-437`): I 분석가 보고서 → II 리서치 결정 → III 트레이딩 계획 → IV 리스크 결정 → V 포트폴리오 매니저 결정, 각 구획이 색 테두리 패널.
- **저장**: 구획별 폴더 `1_analysts … 5_portfolio` + `complete_report.md`(`reporting.py:49-71, 105-136`), 그리고 HTML 보고서: 머리에 티커 + 등급(상승/하락/중립 색) + 목차(`report_html.py` masthead · contents).

### B-2. TA v1.0.0 터미널 (`src/cli/Transcript.tsx`, Ink)

- 도구/에이전트 한 줄: 왼쪽 기호, 이름과 질문, 결과, 오른쪽에 **출처와 걸린 시간**(`:157-167`).
- 하위 에이전트 실행 줄: 하나면 한 줄 + 그 에이전트의 최신 호출, 여러 개면 "N개 에이전트 실행 중" 머리 + 각자 줄(`:503-535, 793-881`). 끝나면 완료 표시와 걸린 시간(`:868-881`).
- 메인 에이전트 **계획 단계 목록**: 활성 단계부터 보이게 잘라 보여 주고 "더 있음 +N"(`:540-573`).
- 답 끝: 번호 붙은 출처 목록, 입출력 토큰(`:279-286, 332-333`). `/status`, `/export`(PDF · 마크다운), `/resume`.

### B-3. AHF v1 웹앱 (`app/frontend`, React Flow 노드 캔버스 + SSE)

- **구성**: 노드 캔버스에 `portfolio-start` 노드(실행 방식 Single Run/Backtest 선택, 초기 현금 `100000`, 보유 종목 티커 · 수량 · 가격 입력, `nodes/components/portfolio-start-node.tsx:38-39, 286-330`) → 에이전트 노드 여러 개 → `portfolio-manager` 노드 → 출력 노드. 하단 패널 탭: output / backtest-output / debug console / problems / terminal(`components/panels/bottom/tabs/`). 좌측 흐름 목록 · 우측 컴포넌트 목록 사이드바.
- **에이전트 노드**: 상태 칸 `IDLE | IN_PROGRESS | COMPLETE | ERROR`(`nodes/utils.ts:3`), 진행 중이면 그라데이션 애니메이션, 아래에 상태 메시지 + `(티커)`(`agent-node.tsx:36-37, 95-104`). 노드를 열면 `AgentOutputDialog`: 메시지 줄 `[티커] 텍스트`, 티커 선택 드롭다운(`agent-output-dialog.tsx:139, 162-182`).
- **포트폴리오 매니저 노드**: 상태 + "View Investment Report" 버튼(`portfolio-manager-node.tsx:130-132`) → **투자 보고서 대화창**(`investment-report-dialog.tsx`):
  - 위: 추천 거래 표 `Ticker · Price · Action · Quantity · Confidence`(확신도 ≥50 초록, 아니면 노랑 배지; `:61-94, 128-135`).
  - 아래: "Analyst Signals" 종목별 · 에이전트별 카드 = 제목 + **신호 배지(bullish 초록 / bearish 빨강 / neutral 외곽선) + 확신도 배지** + 이유 글(문자열이면 그대로, 객체면 JSON)(`:35-91 이후`).
  - 백테스트 표: `Date · Ticker · Action · Quantity · Price · Shares · Position Value · Bullish · Bearish · Neutral`(`backtest-output.tsx:106-115`).
- **통신**: 서버가 SSE로 `start` / `progress`(agent · ticker · status · timestamp · analysis) / `error` / `complete`(data) 이벤트를 보냄(`app/backend/models/events.py:16-44`, `routes/hedge_fund.py:63-113`). 에이전트가 `progress.update_status(agent, ticker, "Done", analysis=…)`를 부르는 만큼 노드가 바뀐다(`src/utils/progress.py:44-62`).
- **터미널판**(v1 CLI, `src/utils/display.py`): 종목별로 "AGENT ANALYSIS" 표 `Agent · Signal · Confidence · Reasoning`(`:105`), "TRADING DECISION" 표 `Action · Quantity · Confidence · Reasoning`(`:142-148`), 끝에 "PORTFOLIO SUMMARY" `Ticker · Action · Quantity · Confidence · Bullish · Bearish · Neutral`(`:200-209`) — 신호 투표 수(강세 n명 · 약세 n명 · 중립 n명)를 결정 옆에 같이 보여 준다.

### B-4. AHF v2 텍스추얼 앱 (`hedge_fund/tui/`)

- **실행판(run board)**: 분석가 한 줄씩 `상태기호 · 분석가 · 티커 · 판정 · 논지(실시간 타이핑)`(`report.py:130-166`). 판정(신호 · 확신도)이 먼저 도착하고 이유 글이 뒤에 채워진다. 대기 `⋯ queued` · 완료 `✓ Done`.
- **세션 보고서**: 왼쪽 레일에 신호를 **한 줄씩**(문단이 길어서 표에 넣으면 화면이 넘치므로)(`report.py:203-215`), 오른쪽에 선택한 신호의 전문(`signal_detail`: 종목 · 분석가 · 보는 기간, 판정 · 확신도 % · 값 `+0.35` · 소속 전략, 그 아래 전체 논지, `:290-318`). 아래 상시 띄운 책(book) 요약 줄(`:357`).
- **승인 단계**(`tui/paper.py:351-420`): Enter는 "실행"이라 거래 전에 **무엇을 할지를 먼저 보여 주고 예/아니오**. 질문 "Run <펀드> through <날짜>?", 본문 = 체결될 결정 · 목표 비중 · 새 결정 여부(`run_plan`), 키 `enter run / esc cancel`. 승인 전엔 장부와 브로커를 건드리지 않는다.
- **점수표 화면**: `scorecard_view.py`(아래 D).
- **키**: s 세션 · h halt · r resume · d 삭제 · space 러너(`paper.py:92-99`).

### B-5. FinRobot V2 데스크톱 (`finrobot_desktop/desktop/src`)

- **오른쪽 채팅 패널**(`layout/RightChatPanel/`): AiChatTab · MessageList · MessageBubble · StatusIndicator · SessionsDrawer + **CoverageTriageStrip**("눈여겨볼 것" 띠: 코드로 정한 이상 징후만 한정 개수로 올리고 누르면 질문 문구가 채워진다, `CoverageTriageStrip.tsx:1-9, 63`).
- **StatusIndicator**: 스트림 상태에서 뽑은 단계 + **실제 경과 초**, 도구 실행 중이면 그 도구 이름. "무작위 동사 · 토큰 수 · 반짝임 없음"을 의도적으로(`chat/StatusIndicator.tsx:3-6, 114-144`).
- **실행 단계**(`stores/runStreamStore.ts:30-65`): SSE `run.started`(총 단계 수만 옴) → `step.started` / `step.completed` / `step.retry`. 단계마다 시작 시각(경과 초 카운터), 재시도 횟수("재시도 n/3"로 멈춘 게 아님을 보여 줌), 끝났어도 검증이 실패하면 초록 체크 대신 오류 표시(BUG-058).
- **보고서**: 13장 연구보고서 + 12개월 목표가와 범위 + 모델 확신도 + 데이터 주의 사항, 장 이동 목차(`README.md:87-91`). 판정 배지는 BUY/HOLD/SELL 방향만; 확신도 등급 `high/medium/low/very_low`는 **색이 아닌 다른 채널**(칩 글자 + 광택/투명도)로 표시해 매수/매도 색과 섞이지 않게 한다(`utils/verdict.ts:12-25`).

---

## C. 담당별 신호 자료 모양 (근거 카드 칸 설계 근거)

| 출처 | 칸 | 값 범위 | 이유 칸 | 근거 |
|---|---|---|---|---|
| AHF v1 투자자 에이전트 | `signal`, `confidence`, `reasoning` | bullish/bearish/neutral, 0~100 실수, 글 | 글 한 덩어리(객체도 가능) | `ben_graham.py:14-17, 78`; 프롬프트 `:316-331` |
| AHF v1 → 포트폴리오 매니저로 전달 | 압축해 `{sig, conf}`만 | 〃 | 이유는 안 넘김 | `portfolio_manager.py:56-63` |
| AHF v1 포트폴리오 결정 | `action, quantity, confidence(0~100 정수), reasoning` | 5행동 | 글 | `:13-17` |
| AHF v2 `Signal` | `model_name, ticker, date, value(-1.0~+1.0), reasoning, components{}, metadata{signal, confidence, abstained}, horizon_days` | 연속값 | 글 + 수치 분해 | `AHF2: models.py:14-28`, `signals/base.py:21-30`(`abstained` 플래그: "의견을 못 냄"을 따로 표시) |
| AHF v2 판정 단어 | bullish/bearish/neutral/abstain 4종, 기호 + 색 | | | `tui/report.py:55-70` |
| TA v0.7.1 감성 분석가 | `overall_band`(6단계) + `overall_score`(0~10) + `confidence`(low/medium/high) + `narrative` | | 서술 5항목 + 마지막에 요약 표 | `schemas.py:296-363`; 모든 분석가 프롬프트에 "끝에 핵심 정리 마크다운 표" `market_analyst.py:51` |
| TA v0.7.1 매니저/트레이더 | 5단계 / 3단계 등급 + `rationale`/`reasoning` + (진입가 · 손절 · 비중: 없으면 "not provided"로 **이름을 남김**) | | 글 | `schemas.py:95-185` |
| FinRobot 재판 | `call`(3), `conviction`(0~1), `swing_factor`, `change_my_mind` | | **주장 + 근거 id 목록**(숫자는 쓰지 않고 id만) | `judge_agent.md`, `bull_agent.md` |

패턴: ① 방향(3~5단계) + ② 확신도(수치 0~100/0~1 또는 low/med/high) + ③ 이유 글이 모든 곳에서 공통. 모자란 칸을 필요로 할 때 붙이는 것은 ④ 보는 기간(`horizon_days`/`time_horizon`) ⑤ 기권 표시(`abstained`) ⑥ 근거 id(FinRobot, TA v1 출처 번호). 종합 단계에서는 이유를 떼고 `{방향, 확신도}`만 넘기는 압축이 쓰이므로(AHF v1) "한 줄 이유"는 카드에는 남기되 팀장 입력엔 점수만 줄 수도 있다.

---

## D. 메모리 · 복기

### D-1. TA v0.7.1 (가장 구체적)

- 저장: 결정이 나올 때마다 **줄 머리표 `[날짜 | 티커 | 등급 | pending]` + `DECISION:` 본문**을 한 개의 추가 전용 마크다운 로그 파일에 덧붙인다(`TA07: memory/log.py:32-60`, 구분자는 `<!-- ENTRY_END -->`). 같은 날짜+티커 항목이 있으면 다시 쓰지 않는다(`:44-52`, 중복 계산 방지). LLM 호출 없음.
- 정산: 다음 실행 때(또는 `settle_all_pending`) 보유 기간이 **지난 항목만** 채점: 기준일 이후 `holding_period_days=5` 거래일의 원수익률 − 벤치마크(미국 SPY, 도쿄 `.T`는 `^N225` 등 접미사 지도) 수익률 = **알파**(`memory/settlement.py:60-125`, 기본값 `default_config.py:180`). 오늘 봉은 아직 장중이라 쓰지 않는다(`:44-58`). 기간이 안 끝났으면 `pending`으로 둔다.
- 반성: 위 결과 + 원래 결정을 주고 **정확히 2~4문장 평문**을 쓰게 한다. 순서: ① 그 기간 알파가 방향 판단에 대해 보여 주는 것(수치 인용, 기간이 너무 짧아 판단 불가면 그렇게 쓴다) ② 논지 중 뒷받침/반박된 부분 ③ 다음 비슷한 분석에 쓸 교훈 하나(`memory/reflection.py:13-34`). 완료되면 머리표가 `[날짜|티커|등급|원수익|알파|보유일]`로 바뀌고 `REFLECTION:`이 붙는다(`log.py:344-352`).
- 읽기: 포트폴리오 매니저 프롬프트에만 주입(분석가 · 토론자는 안 읽음, `graph/setup.py:158-159`). 같은 티커 최근 **5건은 결정 전문 + 반성**, 다른 티커 최근 **3건은 반성만**(없으면 결정 300자)(`log.py:93-134, 354-360`). `as_of` 날짜를 주면 그날까지 결과가 확정된 항목만(미래 누출 방지, `:97-107`). 보관 상한 `memory_log_max_entries`로 오래된 결정부터 버림(`:276-308`).
- 화면에는 정산 결과가 따로 없다. 있는 것은 로그 파일(사람이 읽는 마크다운)과 HTML 보고서의 "Memory log" 메모(`report_html.py` 의 `note` 클래스).

### D-2. TA v1.0.0

`engine/memory.ts` 머리말: 사용자에 대한 메모 · 분석이 낸 결정 · "결정의 첫 거래 세션들이 지난 뒤 결과와 짧은 교훈"을 **사람이 읽는 추가 전용 파일**로 남긴다. 읽을 때는 "오늘까지 일어나고 기록된 결과/교훈만" 보인다(`src/engine/memory.ts:3-8`). 같은 설계가 TS로 옮겨졌다.

### D-3. AHF v2 "점수표(scorecard)"가 복기 역할

반성 글은 없고 **담당(분석가)별 성적표**가 있다(`AHF2: scorecard/models.py:9-49`): 분석가별 `horizon_days`(그 기간에 채점) · `n_views` · `n_directional` · `n_abstained` · `n_pending`(아직 기간이 안 지난 견해) · `hit_rate`(방향이 초과수익 부호와 맞은 비율) · `slugging`(맞았을 때 평균 이익 / 틀렸을 때 평균 손실) · `ic_mean/ic_std/ic_t_stat`; 전략별 `pnl · return_pct · win_rate · t_stat · share_of_pnl · max_drawdown_pct`; 펀드 전체 `fund_pnl`, `trading_cost`. 이 성적이 다음 결정의 **자본/가중치 배수**로 되먹임된다(기록이 짧으면 사전 가중치로 수축, `scorecard/allocation.py:1-12`). 뷰: `tui/scorecard_view.py`.
세션 기록은 해시로 연결된 장부 파일(`ledger/<세션>.json`)이고 `CycleRecord`가 한 세션의 신호 · 목표 · 클램프 · 주문 · 체결 · 포지션 · NAV를 다 담는다(`pipeline/models.py:105-131`). 하루/세션 정리 화면 = 세션 목록(날짜, 그날 변화, 한 일; `report.py:387-408` `session_row`) + 선택하면 개요(`session_overview`) + Enter로 전체 보고서.

### D-4. AHF v1 / FinRobot

AHF v1 · FinRobot 모두 지난 매매에서 배우는 메모리 없음(FinRobot은 세션 목록 drawer와 "커버리지 triage"만). 확인한 한도: FinRobot의 `finrobot_equity`(V1) · `finrobot_autogen`(V0)의 메모리는 읽지 않았다.

---

## E. 비용 · 호출 수 표시

- **TA v0.7.1**: 콜백으로 LLM 호출 수 · 도구 호출 수 · 입력/출력 토큰을 **합계 카운터**로만 센다(`cli/stats_handler.py:9-76`; 호출은 chat 모델 시작 시 +1, 토큰은 응답의 `usage_metadata`에서). 화면은 하단 한 줄(`display.py:353-376`)뿐. **예산 대비 잔량 바는 없다.** 토큰을 모르면 `--` 표시.
- **TA v1.0.0**: 한도가 있다(`docs/usage.md:165-182`): `question.steps | tokens | time`(질문 전체, 에이전트 포함), `agents.<이름>.steps|tokens|time`(에이전트별), `output.stepTokens | finalTokens`. 기본은 **스텝만** 막음(메인 30, 에이전트 각자: 예 리스크 12 · 기본 15). 시간 한도를 주면 "답을 쓸 시간이 남게 일찍 조사를 멈춘다". 한도에 걸리면 종료 코드 3("부분 답일 수 있음"). 한도 후 마지막 답용으로 출력 토큰 16,384와 120초를 따로 남긴다(`src/contracts/limits.ts` MODEL.finalOutputTokens / finalGraceMs). 컨텍스트 창 안에 들어가게 오래된 데이터 결과를 "id 포인터"로 잘라내는 예산 관리(`src/agents/budget.ts:1-50`). 화면: 답 끝 입출력 토큰(`Transcript.tsx:332-333`), 각 호출 줄에 걸린 시간.
- **AHF**: LLM 토큰 · 호출 카운터는 **없다**(`hedge_fund` 와 `src` · `app/frontend` 검색 결과, 비용은 거래 비용(`brokers/costs.py`)뿐). 에이전트 노드는 상태 글만.
- **FinRobot**: 의도적으로 토큰 수를 보여 주지 않고 경과 초와 재시도 횟수만(`StatusIndicator.tsx:6`, `runStreamStore.ts:42-43`).

즉 "담당별 잔량 바"는 세 저장소에 **원형이 없다**. 근거로 쓸 수 있는 재료는 TA v0.7.1의 카운터 4종, TA v1.0.0의 3축 한도(스텝 · 토큰 · 시간)와 담당별 한도 키, "마지막 답용 예비분"이다.

---

## F. 우리에게 넣기를 권하는 것 (상위 8)

우리 구성: 3D 사무실, 담당 6명(뉴스 · 전략 · 리스크 · 복기 · 매매 · 팀장), 회의실 논의 → 점수 → 팀장 판단 → 주문 안 → 사용자 승인, 오른쪽 채팅 패널, 머리 위 말풍선. 아래는 원본이 하는 일이고, 우리 쪽 적용은 제안이다(스펙과 다르면 멈추고 묻는 원칙에 따라 결정은 사람이 한다).

| # | 권장 | 근거 저장소 · 원본 모양 | 우리 쪽에 넣는 법(한 줄) |
|---|---|---|---|
| 1 | **담당별 신호 칸 = 방향 + 확신도 + 이유 글 (+ 보는 기간 · 기권)** | AHF v1 `{signal, confidence 0~100, reasoning}` (`ben_graham.py:14-17`); AHF v2 `Signal{value -1~+1, reasoning, horizon_days, abstained}` (`models.py:14-28`); TA 감성 `band + score 0~10 + confidence low/med/high` | 근거 카드의 담당 한 줄 = `점수 · 방향 · 확신도(3등급) · 한 줄 이유 · 기간`, "의견 없음"은 0점이 아니라 별도 `기권` 표시로 |
| 2 | **팀장 결정 칸 = 등급 + 2~4문장 요약 + 논지 + 뒤집힐 조건** | TA `PortfolioDecision`(등급 · executive_summary · thesis · price_target? · time_horizon?, `schemas.py:221-262`); FinRobot `conviction 0~1 · swing_factor · change_my_mind` (`judge_agent.md`) | 팀장 카드에 `결정 · 확신도 · 요약 · 근거 · "이게 바뀌면 뒤집힘" 한 줄`, 모호하면 관망 대신 **낮은 확신도로 방향을 정하고** 판단 불가만 별도 `확인 필요`(TA `REVIEW`)로 사람에게 |
| 3 | **리스크 담당은 "한도 전 → 후"를 기록해 보인다** (거부권은 우리가 정할 규칙, 원본엔 없음) | AHF v1 변동성 × 상관 한도로 허용 수량을 코드가 먼저 계산(`risk_manager.py:270-317`, `portfolio_manager.py` `compute_allowed_actions`); AHF v2 `ClampEvent{limit, ticker, before, after}` (`risk/limits.py:97-107`); 실패 시 halt (`paper/tick.py:17-18`) | 주문 안 카드에 `리스크 한도 적용: 비중 12% → 8% (사유: 변동성)`을 줄로 남기고, 거부권이 필요하면 "한도 0 + 사유" 한 가지 모양으로 통일, 이상 시 전체 정지 버튼 |
| 4 | **회의실 논의는 고정 횟수 + 상대 발언에 답하는 순서** | TA v0.7.1 강세/약세 번갈아 `count >= 2×N`, 기본 N=1, 상대 마지막 주장에 반박(`conditional_logic.py:15-21`, `bull_researcher.py`); 리스크 3인 순환 `3×N`(`:25-33`); 도구 상한 20 후 "보고서 쓰라" 강제(`setup.py:73-89`) | 회의실 라운드를 "담당당 1~2발언, 끝 조건은 총 발언 수"로 고정하고 말풍선은 발언 1개 = 1풍선, 담당마다 호출 상한 도달 시 "지금까지로 정리해 말하라"를 자동 투입 |
| 5 | **진행 단계 표 + 담당별 상태 + 하단 요약 줄** | TA v0.7.1 `Team · Agent · Status`(pending/in_progress/completed/error) + `Agents 3/12 | LLM | Tools | Tokens | Reports | 경과` (`display.py:197-261, 353-376`); AHF v1 `IDLE/IN_PROGRESS/COMPLETE/ERROR` + 상태 메시지 + `(티커)` (`nodes/utils.ts:3`, `agent-node.tsx:95-104`); FinRobot `step.retry n/3` + 경과 초 (`runStreamStore.ts:42-43`) | 머리 위 말풍선 위에 상태 점(대기/진행/완료/오류), 채팅 패널 상단에 `단계 n/5 · 담당 3/6 · ⏱ 경과` 한 줄, 재시도 중이면 "재시도 2/3"을 보여 멈춘 게 아님을 알림 |
| 6 | **담당별 잔량 바 = 호출 · 토큰 · 시간 3축 한도 + 최종 답 예비분** (원형 없음, 재료만) | TA v1.0.0 `question.*` / `agents.<이름>.*` 한도, 시간 한도 시 일찍 멈춰 답 쓸 시간 남김, 마지막 답 예비 16,384 토큰/120초(`docs/usage.md:165-182`, `limits.ts`); TA v0.7.1 카운터(`stats_handler.py`); FinRobot은 토큰 수를 일부러 숨김 | 담당별 바 = `사용/한도`(호출 수 기준, 토큰은 툴팁), 한도 도달 시 "정리 모드"로 전환해 팀장 몫은 항상 남겨 두고 중단 시 `부분 결과` 표시 |
| 7 | **어제 메모 카드 = 결정 → N일 뒤 정산(수익률 · 초과수익) → 2~4문장 교훈, 팀장에게만 주입** | TA v0.7.1 로그 `[날짜|티커|등급|원수익|알파|보유일]` + `DECISION:` + `REFLECTION:`(`log.py:344-360`), 5거래일 뒤 정산 · 기간이 짧으면 판단 보류라고 쓰기(`reflection.py:13-34`), 읽기 = 같은 종목 5건 전문 + 다른 종목 3건 교훈만, `as_of` 누출 방지(`log.py:93-134`), 중복 방지 | 복기 담당 카드 칸 = `날짜 · 종목 · 판단 · 수익률 · 초과수익 · 보유일 · 교훈 한 줄`, 기간 안 끝난 건 `정산 대기`로 표시, 다음 팀장 입력에 최근 같은 종목 몇 건 + 다른 종목 교훈 몇 건만 |
| 8 | **담당별 적중률 + 주문 안 승인 화면(실행 전에 무엇을 할지 보여 주고 예/아니오)** | AHF v2 `AnalystScore{hit_rate, slugging, ic, n_pending, horizon_days}`(`scorecard/models.py:9-26`)이 가중치로 되먹임(`allocation.py:1-12`); 승인 모달 "Run <fund> through <date>?" + 계획 본문 + enter/esc, 승인 전 장부 · 브로커 무변경(`paper.py:351-420`); TA 트레이더 제안(진입가 · 손절 · 비중, 없으면 "not provided", `schemas.py:146-185`); AHF v1은 허용 수량을 코드로 계산한 뒤 LLM이 그 안에서 선택 | 주문 안 승인 창에 `종목 · 방향 · 수량 · 진입/손절 · 리스크 한도 전→후 · 팀장 확신도 · 담당별 투표(강세 n · 약세 n · 중립 n; AHF v1 요약 표 `display.py:200-209`)`를 한 화면에, 승인 전엔 모의계좌에 아무 것도 쓰지 않고, 담당 카드에 `적중률 n/m (대기 k)`를 붙이는 것은 데이터가 쌓인 뒤로 |

보조로 얻은 소규모 원칙(위 8개에 끼워 넣기 좋은 것):
- **모든 수치는 근거 id/출처 번호로**: FinRobot은 LLM이 숫자를 직접 쓰지 못하게 하고 id만 인용, 표시층이 값을 채운다(`bull_agent.md`); TA v1.0.0은 답 끝 번호 출처 목록. 카드의 "한 줄 이유"에서 숫자는 근거 id 링크로.
- **신호 목록은 한 줄 레일 + 상세는 옆 창**(AHF v2 `report_nav`, 문단이 길어 표를 깨기 때문). 근거 카드도 접힘 상태를 한 줄 기본으로.
- **확신도는 방향 색과 다른 채널로**(FinRobot `verdict.ts`): 매수 · 매도 색을 확신도가 오염하지 않게 칩 글자/투명도로.
- **분석 먼저 병렬 → 토론 순차**(TA v0.7.1): 뉴스 · 전략 담당은 동시에 일하고, 회의실 토론은 모두 끝난 뒤.

---

## 확인하지 못한 것 · 주의

- 세 저장소 모두 **실행하지 않았다**. 화면 구성은 코드와 README로 재구성한 것이고 실제 모양(색, 간격)은 못 봤다. `assets/demo.gif`, `figs/*.png`는 열어 보지 않았다.
- TA v0.7.1에 **웹 화면이 없다**(터미널 + HTML 보고서만). 사용자가 말한 "웹 화면"은 AHF v2.0.0의 `app/`과 FinRobot 데스크톱에 해당한다.
- AHF v2.0.0 프런트엔드는 React Flow 캔버스의 노드 구조와 대화창 · 표 칸을 코드(JSX 문자열)로 읽었다. 레이아웃 좌표 · 색은 확인 안 함. 보고서 대화창 `:35-91 이후` 구간의 세부(카드 안 JSON 렌더 등)는 부분만 읽었다.
- AHF v1의 `compute_allowed_actions` 하단(공매도 마진 계산)과 포트폴리오 매니저 프롬프트 전문은 요약만 확인.
- AHF v2 `tui/scorecard_view.py`, `tui/home.py`, `tui/builder.py`는 열어 보지 않았다(점수표 필드는 `scorecard/models.py`로 확인).
- FinRobot은 가볍게만: 에이전트 지시문 3개, 실행 스트림 저장소, 채팅 패널 · 상태 표시, 판정 유틸을 읽었다. 토론이 **한 번의 단발 호출인지 반론 라운드가 있는지**는 구현 오케스트레이션 코드(`engine/pipelines/equity_research.py` 안의 호출부)를 끝까지 따라가지 못해 지시문 기준으로만 적었다. V0 · V1 폴더는 읽지 않았다.
- "리스크 거부권 없음"은 위 세 저장소(TA v0.7.1 · v1.0.0, AHF v1 · v2, FinRobot 지시문)에서 찾지 못했다는 뜻이다. 모든 파일을 끝까지 읽은 것은 아니다.
- 라이선스: TA Apache-2.0, AHF MIT, FinRobot Apache-2.0은 `LICENSE` 첫 줄로 확인. 우리는 코드를 옮기지 않고 동작만 참고한다.

---

## 부록 1. 조사 대상 판

| 폴더(`.../refs/src2/`) | 저장소 · 판 | 커밋(받은 시점 HEAD) |
|---|---|---|
| `TradingAgents/` | TauricResearch/TradingAgents main (v1.0.0, TS) | `81501e0` |
| `TradingAgents-v0.7.1/` | 같은 저장소 태그 v0.7.1 (파이썬 토론 그래프) | `c454491`(태그) |
| `ai-hedge-fund/` | virattt/ai-hedge-fund main (v2.7.2, `hedge_fund/`) | `5c553b5` |
| `ai-hedge-fund-v2.0.0/` | 같은 저장소 태그 v2.0.0 (`src/` + `app/`) | `284a650` |
| `ai-hedge-fund-v2.7.2/` | 태그 v2.7.2 (HEAD와 같은 구조라 쓰지 않음. 빈 확인용으로 받음) | `5c553b5` |
| `FinRobot/` | AI4Finance-Foundation/FinRobot main | `63e1884` |

## 부록 2. `rea` (6.7.0)가 해 준 것 / 못 한 것

| 항목 | 한 일 | 결과 |
|---|---|---|
| `rea doctor` | 환경 점검 | 전체 `healthy: false`(Hopper 미설치 등 이 조사와 무관한 분석 엔진, 확인만). 이 조사에 쓴 명령은 문제 없이 동작 |
| `rea --help` · `rea import-reference-source --help` · `rea analyze-javascript-application --help` | 사용법 확인 | `import-reference-source <root> [--detail full\|summary] [--outline-depth n]`, `analyze-javascript-application <path>`(ASAR 또는 풀어 둔 앱 경로, Electron 중심) |
| `rea import-reference-source --detail summary` (5개 폴더: TA, TA v0.7.1, AHF, AHF v2.0.0, FinRobot) | 소스 트리를 "역사적 참고 자료(historical-reference)"로 가져와 목록 · 해시 · 언어 · 매니페스트 · 관계 수를 만듦. 전부 종료 코드 0, 구문 분석 실패 0 | **파일 목록 · 언어 분포 · 해시 · 크기(TA 234 · TA v0.7.1 213 · AHF 151 · AHF v2.0.0 341 · FinRobot 1011 파일)**. 관계(import 간선) 수: TA 830, AHF v2.0.0 498, FinRobot 1030(TS/JS 계열만), **TA v0.7.1 · AHF HEAD는 0**. 파이썬 쪽은 목록 · 해시까지만 주고 함수 · 의미 분석은 없음 |
| `rea analyze-javascript-application TradingAgents --artifact-format directory` | TS 소스를 AST로 읽어 모듈 그래프를 만듦(28초, 종료 코드 0) | 노드 1354 · 간선 3175, 의미 그래프 노드 10만 · 관계 88k. 그러나 Electron 전용 요약(창 · preload · IPC)은 전부 0이라 Node CLI에는 새 정보가 거의 없었다. 결과 JSON이 1.7억 바이트여서 요약값만 봄 |
| 파이썬 분석 | **못 함.** `rea`에 파이썬 소스를 읽는 명령이 보이지 않고, `import-reference-source`도 파이썬은 목록 · 해시만 | 파이썬(TA v0.7.1 · AHF 전부)은 `grep` · `sed` · 파일 읽기로 했다 |
| 웹앱 분석 | AHF v2.0.0 `app/frontend`(React)에는 `analyze-javascript-application`을 따로 돌리지 않았다 | 코드를 직접 읽음(위 B-3). FinRobot `desktop/`도 같은 방식 |

산출 JSON(참고용, 이 문서에 필요한 것만 옮김): `.../scratchpad/refs/out/rea-import-<이름>.json`, `rea-js-TradingAgents.json`.

# template 모듈 (T8)

크레이트 `pi-template`, 폴더 `crates/template/`. 구성 요소 A6 가운데 템플릿 값 · 절대 상한 · 승격 기준(M3). 내부 의존 없음. 순수 계산(파일 · 네트워크 · 시계 없음).
근거: `dev.md` C4 · C14 · M3 · E5 · E26 · E36 · N2 · N4 · V27~V30, `principles.md` "바꿀 수 없는 원칙" · "템플릿에서 조정하는 값", `spec.md` F6.

## 원본 동작 근거

- 템플릿 = 프로필 배포(설정 묶음을 통째로 고르고, 사본 위에서 값을 바꾼다): `docs/research/hermes-agent.md` 3.8 "프로필 배포 = 템플릿" · "프로필별 모델", `docs/research/deskrpg.md` 5-3, `docs/research/deskrpg-hermes-plugin.md` 4-1 · 4-2 · 4-5. 대조표 `docs/dev/conformance.md` D24.
- 값과 상한은 원본에 없음(우리 고유). 기본값의 출처는 `docs/research/stock-investing-principles.md` 8.1 · 8.2 · 9장이다(추정값이라 문서 주석에 "출처 · 미확정"을 적는다).

## 만드는 것 (1차)

1. `Style`(단타 · 스윙 · 중장기)과 `preset(style) -> RiskParams`. 값은 조사 9장 표에서 가져온다(C4 · V27).
2. `RiskParams`: principles.md "템플릿에서 조정하는 값"의 칸 전부. 승인 대기 시간(N2: 손절 1분 · 익절 5분 · 사기 5분)과 미체결 취소 시간(N4: 단타 1분 · 스윙 10분 · 중장기 장 마감)도 이 칸에 든다. 돈과 비율은 십진수로 둔다.
3. `AbsoluteCaps`: R 2% · 하루 손실 5% · 최대 낙폭 20% · 종목 25% · 업종 50%. 이 값은 바꿀 수 없다.
4. `adjust(current, changes) -> Result<RiskParams, OverCap>` (C14 · E5): 하나라도 상한을 넘으면 전체를 거부하고 지금 값을 그대로 둔다. 오류에는 넘은 칸과 상한 값이 담긴다(V28: 3%를 넣으면 2%가 보인다).
5. `PromotionCriteria`(표본 수 · 기간 · 비용 뺀 손익 · 최대 낙폭 · 표본 밖 성과, P14)와 `evaluate(stats) -> Passed | Short(모자란 기준들)` (E26). 기본값(추정): 매매 30건 · 거래일 20일 · 비용 뺀 손익 > 0 · 최대 낙폭 ≤ 템플릿 한도의 절반(결정 2026-11-16 30번).
6. 템플릿 상태 이름(모의중 · 승격대기 · 실전중 · 멈춤)과 허용 전이 표를 값으로 공개한다. 전이를 실제로 강제하는 것(E36: 승인 번호 필요, 전체 정지 → 실전중이 멈춤으로)은 store 9차가 한다.
7. 직렬화: 판 번호가 붙은 JSON. 저장은 store가 한다.

## 2차 작업 (1차 검토의 참고 — 2026-11-16 조정자 결정)

1. **아래쪽 경계도 거부한다**: `adjust`와 `from_json`은 음수·0인 비율(R%, 일일 손실, 최대 낙폭, 종목·업종 비중), 100%를 넘는 비율, 아래 끝이 위 끝보다 큰 범위(`PctRange`·`CountRange`), 0 이하의 개수·시간을 거부한다(지금 값 그대로). 오류는 위반 칸과 이유를 담는다.
2. **승격 낙폭 기준은 지금 한도를 따른다**: 결정 30번 "최대 낙폭 ≤ 템플릿 한도의 절반"은 평가할 때 그 템플릿의 지금 최대 낙폭 한도에서 센다(한도를 바꾸면 기준도 따라간다).
3. 업종 비중이 종목 비중(확신 종목 비중 포함)보다 작아지는 조정은 거부한다.

## 3차 후보 (2차 검토의 참고 — 아직 시작하지 않음)

1. `promotion.max_drawdown_pct` 저장 칸을 없애거나 `Option`으로(지금은 `evaluate_promotion`이 쓰지 않아 낡을 수 있다). 다른 모듈은 승격 판정을 `RiskParams::evaluate_promotion`으로만 부른다.
2. 승격 임계값(비용 뺀 손익 · 표본 밖 손익)의 하한.
3. 손절 방식이 모두 꺼진 조합을 거부한다.

## 4차 작업 (후보 일괄 처리)

3차 후보 가운데 공개 API를 바꾸지 않고 되는 것을 했다.

1. 승격 임계값의 하한: 비용 뺀 손익 · 표본 밖 손익 기준이 음수이면 거부한다(`ParamField::Promotion`, `InvalidReason::NotPositive`). 0은 받는다.
2. 손절 방식이 모두 꺼진 조합 거부: 고정 % · ATR · 논리 · 장 마감 청산이 모두 없고 최대 보유 기간도 정하지 않음이면 거부한다(`ParamField::Stop`, 새 이유 `InvalidReason::NoStopMethod`, `non_exhaustive`라 더하기다). 보유 기간 제한(시간 손절)이 있으면 받는다.
3. 넘김(하지 않음): `promotion.max_drawdown_pct`를 `Option`으로 바꾸거나 없애는 것은 공개 API 변경이라 하지 않았다(평가는 이미 지금 한도의 절반을 쓴다).

## 8차 작업 (후보 일괄 처리)

3차 후보 1번(`promotion.max_drawdown_pct` 낡을 수 있음)을 공개 타입을 바꾸지 않고 처리했다.

1. 템플릿 JSON 읽기: `params.promotion.max_drawdown_pct` 칸이 빠져도 읽힌다. 빠지면 평가가 쓰는 값(`params.max_drawdown_pct / 2`)으로 채운다. 칸이 있는 옛 JSON은 그대로이고, 최대 낙폭 한도(`params.max_drawdown_pct`) 자체가 빠지면 지금처럼 `Error::Json`이다. 쓰기는 그대로 칸을 담는다.
2. 테스트를 `tests/suite/main.rs` 한 바이너리로 옮기고 `d()` 도우미를 `suite/common.rs` 하나로 모았다.

## 9차 작업 (2026-11-21 발표 자료 대조)

근거: `docs/dev/decisions.md` "2026-11-21 발표 자료 대조 뒤 결정" 9번, `prd.md` R14-1, `principles.md` P15("위험을 키우는 변경은 냉각 기간 뒤, 줄이는 변경은 즉시"), `spec.md` F5 7a · F6 3 · 조건표 "개선안이 위험을 키우는가", `dev.md` E5 · E28 · E35 · V42.
이 모듈은 순수 계산 그대로다(승인 · 저장 · 시계 · 달력 없음). "승인 번호가 있어야 하고 다음 거래일부터"를 실제로 지키는 길은 복기 개선안과 같은 길(store 개선안 → approval 규칙 개선 승인 → journal 냉각 → 다음 거래일 반영)이고, 그 연결은 journal · server 차수다. 이 차수는 그 길에 넣을지 바로 적용할지를 가르는 **판정과 표**를 이 모듈 한 곳에 둔다.

### (a) 공개 API 바뀜 (새 파일 `src/risk.rs`, `adjust`는 그대로)
- `#[non_exhaustive] pub enum RiskField`: 아래 표의 칸마다 하나.
- `pub fn widened_fields(before: &RiskParams, after: &RiskParams) -> Vec<RiskField>`: 위험을 키운 칸들(`RiskField` 순서, 겹치지 않게).
- `pub fn increases_risk(before: &RiskParams, after: &RiskParams) -> bool` = `!widened_fields(..).is_empty()`.
- `pub enum AdjustPlan { ApplyNow(RiskParams), NeedsApproval { after: RiskParams, widened: Vec<RiskField> } }`.
- `pub fn plan_adjust(current: &RiskParams, changes: &[Change]) -> Result<AdjustPlan, OverCap>`.

### (b) 동작 규칙 · 경계
1. `plan_adjust`는 먼저 `adjust`를 부른다. 상한 · 아래쪽 경계를 어기면 지금처럼 `OverCap`이고 승인이 있어도 바뀌지 않는다(절대 상한은 그대로, E5 · E28 "원칙의 절대 상한은 넘기지 않는다").
2. 넘지 않으면 `before = current`, `after = adjust 결과`로 `widened_fields`를 본다. 비었으면 `ApplyNow(after)`(줄이거나 같거나 위험과 무관한 칸만 바뀜 → 바로), 아니면 `NeedsApproval`.
3. 한 묶음에 키우는 칸과 줄이는 칸이 섞이면 **묶음 전체가** `NeedsApproval`이다(`adjust`가 묶음을 통째로 받거나 거부하는 것과 같게, 업종 ≥ 종목 같은 칸 사이 조건이 반만 적용되어 깨지지 않게). 화면은 줄이는 칸만 따로 보내면 바로 된다고 안내한다. 이 선택은 사용자 확인 대상이다.
4. 위험을 키우는 쪽의 표(비교는 `before` → `after`):

| 칸 | 위험을 키우는 쪽 | 출처 |
|---|---|---|
| `risk_per_trade_pct` (P1) | 오름 | journal `increases_risk`와 같음 |
| `daily_loss_limit_pct` (P3) | 오름 | 같음 |
| `max_drawdown_pct` (P4) | 오름 | 같음 |
| `symbol_weight_pct` (P8) | 오름 | 같음 |
| `conviction_weight_pct` | `unwrap_or(종목 비중)`이 오름(값이 없어지면 종목 비중으로 셈) | 같음 |
| `sector_weight_pct` (P8) | 오름 | 같음 |
| `auto_order_limit_pct` | 오름(승인 없이 나가는 금액이 커짐) | 같음 |
| `min_cash_pct` | 내림 | 같음 |
| `max_positions.high` | 오름 | 같음 |
| `max_trades_per_day` | 오름 | 같음 |
| `loss_streak` | 없어짐 · 정지까지의 손실 횟수 오름 · 쉬는 기간 줄어듦(그날만 < 거래일 n일) | 같음 |
| `stop.fixed_loss_pct` | 없어짐 · 아래 끝 내려감 · 위 끝 오름 | 같음 |
| `stop.atr` | 없어짐 · 배수 오름 | 같음 |
| `stop.thesis_break`(논리 손절) · `stop.flatten_before_close`(장 마감 청산) | `true`이던 것이 `false`로 | 추정(손절을 넓힘) |
| `max_holding` | 길어짐(`SameDay` < `TradingDays(n)` < `Unlimited`, n이 커짐) | 추정(template 4차 2번이 시간 손절로 셈) |
| `chase_limit_pct` | 오름 · 정해져 있던 것이 없어짐 | 추정(한도를 넓힘) |
| `promotion`(승격 기준) | `min_trades` · `min_trading_days` 내림, `net_pnl_above` 내림, `out_of_sample_pnl_above` 내림 · 있던 것이 `None`으로 | 추정(실전에 더 쉽게 오름, P14) |
| `universe` · `trading_window` · `entry_signals` · `triggers` · `rebalance` · `take_profit` · `approval_wait` · `unfilled_cancel` | 키우지 않는 것으로 본다 | 추정(한도 · 손절이 아니다. 조건표 "한도를 넓히거나 손절을 넓히는 변경이면 예") |

   "추정" 줄은 사용자 확인 대상이다. "같음" 줄은 `crates/journal/src/improve.rs:28-53`의 판정과 같다. journal이 이 함수를 쓰게 바꾸는 것(두 표를 하나로)은 journal 차수다.
5. `ApprovalWait` 같은 시간 칸을 짧게 하는 것, 거래 시간대를 넓히는 것은 위 표대로 키우지 않음이다. 바뀌면 표와 테스트를 함께 고친다.

### (c) 원본 동작 근거
- 원본에 없음(값과 상한은 우리 고유, 1차와 같다) — 사용자 결정 2026-11-21 9번, `principles.md` P15, PRD R14-1.
- "위험을 줄이는 쪽은 바로, 키우는 쪽은 더 확인": `docs/research/remote-access-security.md` 5.3(비대칭 표).
- 템플릿 = 프로필 사본 위에서 값 바꾸기는 1차 근거 그대로(`docs/research/hermes-agent.md` 3.8).

### (d) 테스트로 확인할 것 (검증 주석: C14 E5 V28, 그리고 새로 E28 · P15를 적는다 — 필수 번호 줄에 더하는 것은 N1 커밋과 함께 조정자가 한다)
- 표의 칸마다 키우는 변경 하나 · 줄이는 변경 하나: `widened_fields`가 그 칸 하나만 / 빈 목록.
- 세 프리셋(`Style::ALL`) 각각에서 같은 값으로 바꾸면 `ApplyNow`(지금 값과 같음).
- 상한을 넘는 키우는 변경(R 3%)은 `NeedsApproval`이 아니라 `OverCap`(2%가 보임, V28), 지금 값 그대로.
- 줄이는 칸과 키우는 칸을 한 묶음에 넣으면 `NeedsApproval`이고 `widened`에 키운 칸만.
- 확신 비중 `None` ↔ 값, 고정 손절 범위의 아래 · 위 끝, 쉬는 기간 `RestOfDay` ↔ `TradingDays(n)`의 경계를 표로.
- 속성 테스트: 어떤 `changes`에도 `plan_adjust`의 결과 값은 `adjust`의 결과와 같다(판정만 더해짐).

### (e) 잠긴 옛 테스트와 부딪히는 곳
없음. `adjust` · `from_json` · `RiskParams` · `Change`의 모양과 동작을 바꾸지 않는다(새 함수만 더한다).

### 물을 것
- 표의 "추정" 줄(논리 손절 · 장 마감 청산 끄기, 최대 보유 기간 늘리기, 추격 폭, 승격 기준 낮추기)을 위험을 키우는 변경으로 볼지.
- 섞인 묶음을 통째로 승인 대기로 둘지(추천), 줄이는 칸은 바로 · 키우는 칸만 대기로 쪼갤지.

## 하지 않는 것

저장, 승인, 주문 점검(rules), 실전 계좌, 화면.

## 테스트 주의

프리셋 값은 조사 표와 끝자리까지 비교한다. 상한 경계(정확히 2%는 받고 2.0001%는 거부)를 시험한다. 거부된 조정이 지금 값을 바꾸지 않는지 확인한다.

## 필수 번호

C4 C14 M3 E5 E26 N2 N4 V27 V28 P14

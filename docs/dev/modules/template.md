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
- 끝남: 아래쪽 경계도 거부한다: adjust와 from_json은 음수·0인 비율(R%, 일일 손실, 최대 낙폭, 종목·업종 비중), 100%를 넘는 비율, 아래 끝이… — 커밋 3c19890(red) · ecbb905(green) · 전문: `git show 1a73576:docs/dev/modules/template.md`

## 3차 후보 (2차 검토의 참고 — 아직 시작하지 않음)
- 끝남: promotion.max_drawdown_pct 저장 칸을 없애거나 Option으로(지금은 evaluate_promotion이 쓰지 않아 낡을 수 있다). 다른… — 커밋 63c242d(red) · 32f41d8(green) · 전문: `git show 1a73576:docs/dev/modules/template.md`

## 4차 작업 (후보 일괄 처리)
- 끝남: 3차 후보 가운데 공개 API를 바꾸지 않고 되는 것을 했다. — 커밋 49f6a29(red) · 77aa4e8(green) · 전문: `git show 1a73576:docs/dev/modules/template.md`

## 8차 작업 (후보 일괄 처리)
- 끝남: 3차 후보 1번(promotion.max_drawdown_pct 낡을 수 있음)을 공개 타입을 바꾸지 않고 처리했다. — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/template.md`

## 9차 작업 (2026-11-21 발표 자료 대조)
- 끝남: 근거: docs/dev/decisions.md "2026-11-21 발표 자료 대조 뒤 결정" 9번, prd.md R14-1, principles.md P15(… — 커밋 55b89a5(red) · 1a933e0(green) · 전문: `git show 1a73576:docs/dev/modules/template.md`

## 10차 작업 (2026-11-21 스윙에 뉴스 구독 — 사용자 결정 19)
- 끝남: 스윙 프리셋(presets.rs swing())의 구독 트리거를 [Quote, Disclosure, News]로 바꾼다. 다른 칸과 다른 프리셋은 그대로. — 커밋 09909a5(red) · 1423533(green) · 전문: `git show 1a73576:docs/dev/modules/template.md`

## 11차 작업 (2026-11-24 조정자, 남은 후보 — 안전, 사용자 "남은 일 한 개도 없이")
- 끝남: ATR 손절의 봉 단위를 넓히는 것도 위험 증가로 센다: widened_fields의 AtrStop 판정에 봉 단위를 더한다. 분봉 → 일봉은 같은 배수라도 손… — 커밋 c8b8e90(red) · 0d98967(green) · 전문: `git show 1a73576:docs/dev/modules/template.md`

## 12차 작업 (2026-11-25 조정자, 결정 17 — 템플릿 화면의 아래 함수)

근거: `docs/product/stock-team/additions.md`의 C28 · E58 · T37(차수 표 3번), 스펙 R10-1 · R10-2 · R11-2 · F6. server의 템플릿 화면(뒤 차수)이 쓸 **읽기 · 판정 함수**를 이 크레이트에 둔다(저장은 store, 화면 · 경로는 server가 한다 — 이 크레이트는 저장소를 읽지 않는다).

1. **템플릿 보기 모양**: 템플릿 값(RiskParams와 그 밖의 칸)을 화면이 그릴 수 있는 칸 목록으로 낸다 — 칸 이름(안정된 영문 키) · 지금 값 · 절대 상한(있는 칸만, `AbsoluteCaps`) · 바꿀 수 있는가. 순서는 지시서 9차 (b) 4 표의 순서.
2. **모자란 승격 기준**: `PromotionCriteria`와 `PromotionStats`를 받아 기준별로 `{기준, 필요한 값, 지금 값, 채웠는가}` 목록을 낸다(지금 `Verdict`가 통과 · 미달만 주면 그 옆에 둔다). 표본 밖 기준이 없으면 그 줄은 내지 않는다.
3. **판 사이 차이**: 두 `RiskParams`를 받아 바뀐 칸 목록 `{칸, 앞 값, 뒤 값, 넓힘인가}`를 낸다(판 이력 화면용). 넓힘 판정은 `widened_fields`와 같은 답.
4. **실전 템플릿 멈추기의 전이**: 상태 전이 표(`ALLOWED_TRANSITIONS`)에 사용자가 실전중 템플릿을 멈추는 까닭(`TransitionReason`)이 없으면 더한다(실전중 → 멈춤, 사용자 멈춤). 이미 있으면 그대로 두고 시험만 둔다.

- 원본 동작 근거: 원본에 없음 — 스펙 R10-1 · R10-2 · R11-2 · F6-5a1 · F6-9a, additions.md C28 · E58, 결정 17.
- 시험으로 확인할 것: 프리셋 셋의 칸 목록이 순서대로 · 상한이 있는 칸에 상한 값 / 승격 기준이 하나 · 여럿 모자랄 때 줄마다 필요한 값 · 지금 값 · 채움 / 판 차이가 바뀐 칸만 · 넓힘 표시가 `widened_fields`와 같음 / 실전중 → 멈춤(사용자 멈춤)이 허용되고 다른 전이는 그대로.

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| `crates/template/tests/suite/` 의 상태 전이 시험 | 유지 | 지금 허용된 전이는 그대로 — 사용자 멈춤 까닭이 더해져도 허용 목록의 다른 줄은 바뀌지 않는다(전이 목록을 정확히 세는 단언이 있으면 N1이 멈추고 알린다) |
| `crates/template/tests/round9_risk.rs` · `round10_swing.rs` · `round11_atr.rs` · 그 밖 | 유지 | 12차는 읽기 · 판정 함수만 더한다 |

## 하지 않는 것

저장, 승인, 주문 점검(rules), 실전 계좌, 화면.

## 테스트 주의

프리셋 값은 조사 표와 끝자리까지 비교한다. 상한 경계(정확히 2%는 받고 2.0001%는 거부)를 시험한다. 거부된 조정이 지금 값을 바꾸지 않는지 확인한다.

## 필수 번호

C4 C14 M3 E5 E26 N2 N4 V27 V28 P14 E28 P15

// 계좌 · 운용 화면의 그리기(server 30차 2번): 화면 모델의 값으로 카드 · 표 DOM을 만든다. 서버를 부르지 않는다.
// 공통 부품(ui/parts.ts)과 토큰만 쓴다. 3D 라이브러리에 닿지 않는다(N21).
import type { AccountReply, TeamReply } from "./model";

/** 모의계좌 안내 줄(`data-role="paper-notice"`, 글은 `PAPER_NOTICE`). */
export function renderNotice(): HTMLElement {
  throw new Error("todo");
}

/**
 * 팀 상태 카드(`data-role="team"`): 상태 배지(`data-role="team-state"`) · 오늘 할 일(`data-role="team-plan"`) ·
 * 쉬는 이유(`data-role="team-reason"`, 있을 때만). 전체 정지면 까닭(`FULL_STOP_TEXT`)과
 * `button[data-action="restart"]`를 더한다(그 밖의 상태에는 단추가 없다). 단추를 누르면 `onRestart`를 부른다.
 */
export function renderTeam(team: TeamReply, onRestart: () => void): HTMLElement {
  void team;
  void onRestart;
  throw new Error("todo");
}

/**
 * 계좌 카드(`data-role="account"`): 통화마다(`[data-currency]`) 총 평가액(`data-role="equity-total"`) · 쓸 수 있는
 * 현금(`data-role="cash"`) · 오늘 손익(`data-role="day-pnl"`, `data-trend`) · 낙폭 글(`data-role="drawdown"`)과
 * 한도 대비 막대(`data-role="drawdown-bar"`, `data-ratio`, 한도를 모르면 막대 없음).
 */
export function renderAccount(account: AccountReply): HTMLElement {
  void account;
  throw new Error("todo");
}

/**
 * 보유 표(`data-role="holdings"`): 줄마다 `tr[data-symbol]`, 평가 손익 칸은 `data-trend`. 보유가 없으면
 * 빈 상태(`data-role="empty"`, `EMPTY_HOLDINGS`).
 */
export function renderHoldings(account: AccountReply): HTMLElement {
  void account;
  throw new Error("todo");
}

// 계좌 · 운용 화면의 그리기(server 30차 2번): 화면 모델의 값으로 카드 · 표 DOM을 만든다. 서버를 부르지 않는다.
// 공통 부품(ui/parts.ts)과 토큰만 쓴다. 3D 라이브러리에 닿지 않는다(N21).
import { el } from "../ui/call";
import { badge, button, card, emptyState, table } from "../ui/parts";
import type { AccountReply, CashLine, EquityLine, TeamReply } from "./model";
import {
  EMPTY_HOLDINGS,
  FULL_STOP_TEXT,
  OWNER_STOP_TEXT,
  PAPER_NOTICE,
  STOP_LABEL,
  canRestart,
  canStop,
  drawdownRatio,
  holdingRows,
  moneyText,
  percentText,
  planText,
  restReason,
  signedMoneyText,
  teamStateText,
  teamTone,
  trendOf,
} from "./model";

/** 낙폭 막대가 경고 빛깔로 바뀌는 한도 대비 비율(한도의 3/4). */
const NEAR_LIMIT = 0.75;

/** 보유 표의 머리 칸. */
const HOLDING_COLUMNS = ["종목", "수량", "평균 단가", "지금 값", "평가", "평가 손익"] as const;

/** 통화 이름. */
const CURRENCY_NAME: Readonly<Record<string, string>> = { KRW: "원화", USD: "달러" };

/** 모의계좌 안내 줄(`data-role="paper-notice"`, 글은 `PAPER_NOTICE`). */
export function renderNotice(): HTMLElement {
  const node = el("p", "acc-notice", PAPER_NOTICE);
  node.dataset.role = "paper-notice";
  node.setAttribute("role", "note");
  return node;
}

/**
 * 팀 상태 카드(`data-role="team"`): 상태 배지(`data-role="team-state"`) · 오늘 할 일(`data-role="team-plan"`) ·
 * 쉬는 이유(`data-role="team-reason"`, 있을 때만). 전체 정지면 까닭(`FULL_STOP_TEXT`)과
 * `button[data-action="restart"]`를 더한다(그 밖의 상태에는 단추가 없다). 단추를 누르면 `onRestart`를 부른다.
 * (40차) 내가 내린 정지(`stop_cause` `"owner_stop"`)면 까닭 뒤에 `OWNER_STOP_TEXT`를 덧붙인다(`data-role="owner-stop"`).
 * (40차 2번) 전체 정지가 아니고 `onStop`을 주면 정지 자리([`renderStop`])를 단다.
 */
export function renderTeam(team: TeamReply, onRestart: () => void, onStop?: () => void): HTMLElement {
  const state = badge(teamStateText(team.state), teamTone(team.state));
  state.dataset.role = "team-state";
  const plan = el("p", "acc-plan", planText(team));
  plan.dataset.role = "team-plan";
  const head = el("div", "acc-team-head");
  head.append(state, plan);
  const node = card("팀 상태", head);
  node.classList.add("acc-team");
  node.dataset.role = "team";
  node.dataset.state = team.state;

  const reason = restReason(team);
  if (reason !== null) {
    const line = el("p", "acc-reason");
    line.dataset.role = "team-reason";
    line.append(el("span", "acc-label", "쉬는 이유"), el("span", "", reason));
    node.append(line);
  }

  if (canRestart(team.state)) {
    const stop = el("div", "acc-stop");
    const restart = button("다시 시작", "danger");
    restart.dataset.action = "restart";
    restart.addEventListener("click", () => onRestart());
    stop.append(el("p", "acc-stop-text", FULL_STOP_TEXT));
    if (team.stop_cause === "owner_stop") {
      const owner = el("p", "acc-stop-text", OWNER_STOP_TEXT);
      owner.dataset.role = "owner-stop";
      stop.append(owner);
    }
    stop.append(restart);
    node.append(stop);
  } else if (onStop && canStop(team.state)) {
    node.append(renderStop(onStop));
  }
  return node;
}

/**
 * (server 40차 2번) 전체 정지 자리(`data-role="stop"`): 위험 단추 모양의 `button[data-action="stop"]`(글
 * `STOP_LABEL`). 단추를 누르면 `onStop`을 부른다. 팀 상태 카드([`renderTeam`])가 `canStop(state)`일 때만 단다 —
 * 이미 전체 정지면 이 자리 없이 지금의 "다시 시작"만 보인다.
 */
export function renderStop(onStop: () => void): HTMLElement {
  const node = el("div", "acc-stop-action");
  node.dataset.role = "stop";
  const stop = button(STOP_LABEL, "danger");
  stop.dataset.action = "stop";
  stop.addEventListener("click", () => onStop());
  node.append(stop);
  return node;
}

/** 이름 · 값 한 쌍(`data-role`은 값에 단다). */
function stat(label: string, value: string, role: string): { box: HTMLElement; value: HTMLElement } {
  const box = el("div", "acc-stat");
  const shown = el("dd", "acc-stat-value", value);
  shown.dataset.role = role;
  box.append(el("dt", "acc-label", label), shown);
  return { box, value: shown };
}

/** 낙폭 글과 한도 대비 막대(한도를 모르면 막대 없음). */
function drawdownBlock(line: EquityLine): HTMLElement {
  const box = el("div", "acc-drawdown");
  const limit = line.drawdown_limit_pct;
  const words = limit === null
    ? `${percentText(line.drawdown_pct)} · 한도 모름`
    : `${percentText(line.drawdown_pct)} / 한도 ${percentText(limit)}`;
  const label = el("div", "acc-drawdown-label");
  const value = el("span", "acc-drawdown-value", words);
  value.dataset.role = "drawdown";
  label.append(el("span", "acc-label", "최고 대비 낙폭"), value);
  box.append(label);

  const ratio = drawdownRatio(line.drawdown_pct, limit);
  if (ratio === null) return box;
  const bar = el("div", "acc-bar");
  bar.dataset.role = "drawdown-bar";
  bar.dataset.ratio = String(ratio);
  bar.dataset.level = ratio >= 1 ? "over" : ratio >= NEAR_LIMIT ? "near" : "ok";
  bar.setAttribute("role", "meter");
  bar.setAttribute("aria-label", "낙폭 한도 대비");
  bar.setAttribute("aria-valuemin", "0");
  bar.setAttribute("aria-valuemax", "100");
  bar.setAttribute("aria-valuenow", String(Math.round(ratio * 100)));
  const fill = el("div", "acc-bar-fill");
  fill.style.width = `${(ratio * 100).toFixed(1)}%`;
  bar.append(fill);
  box.append(bar);
  return box;
}

/** 통화 하나의 칸: 총 평가액 · 쓸 수 있는 현금 · 오늘 손익 · 낙폭. */
function currencyPanel(line: EquityLine, cash: CashLine | undefined): HTMLElement {
  const panel = el("section", "acc-currency");
  panel.dataset.currency = line.currency;
  const name = CURRENCY_NAME[line.currency] ?? line.currency;
  const head = el("div", "acc-currency-head");
  head.append(el("h3", "acc-currency-name", `${name} · ${line.currency}`), el("span", "acc-label", "총 평가액"));
  const total = el("p", "acc-total", moneyText(line.total, line.currency));
  total.dataset.role = "equity-total";

  const stats = el("dl", "acc-stats");
  const available = stat("쓸 수 있는 현금", moneyText(cash?.available ?? null, line.currency), "cash");
  const pnl = stat("오늘 손익", signedMoneyText(line.day_pnl, line.currency), "day-pnl");
  pnl.value.dataset.trend = trendOf(line.day_pnl) ?? "flat";
  stats.append(available.box, pnl.box);
  // 묶인 돈 · 결제 전 돈은 있을 때만 보인다(평가액의 현금 = 쓸 수 있는 + 묶인 + 결제 전).
  if (cash && trendOf(cash.held) === "up") {
    stats.append(stat("주문에 묶인 돈", moneyText(cash.held, line.currency), "cash-held").box);
  }
  if (cash && trendOf(cash.unsettled) === "up") {
    stats.append(stat("결제 전 돈", moneyText(cash.unsettled, line.currency), "cash-unsettled").box);
  }

  panel.append(head, total, stats, drawdownBlock(line));
  return panel;
}

/**
 * 계좌 카드(`data-role="account"`): 통화마다(`[data-currency]`) 총 평가액(`data-role="equity-total"`) · 쓸 수 있는
 * 현금(`data-role="cash"`) · 오늘 손익(`data-role="day-pnl"`, `data-trend`) · 낙폭 글(`data-role="drawdown"`)과
 * 한도 대비 막대(`data-role="drawdown-bar"`, `data-ratio`, 한도를 모르면 막대 없음).
 */
export function renderAccount(account: AccountReply): HTMLElement {
  const grid = el("div", "acc-currencies");
  for (const line of account.equity ?? []) {
    grid.append(currencyPanel(line, (account.cash ?? []).find((cash) => cash.currency === line.currency)));
  }
  const node = card("모의계좌", grid);
  node.classList.add("acc-account");
  node.dataset.role = "account";
  return node;
}

/**
 * 보유 표(`data-role="holdings"`): 줄마다 `tr[data-symbol]`, 평가 손익 칸은 `data-trend`. 보유가 없으면
 * 빈 상태(`data-role="empty"`, `EMPTY_HOLDINGS`).
 */
export function renderHoldings(account: AccountReply): HTMLElement {
  const rows = holdingRows(account);
  let body: HTMLElement;
  if (rows.length === 0) {
    body = emptyState(EMPTY_HOLDINGS);
    body.dataset.role = "empty";
  } else {
    const grid = table(
      HOLDING_COLUMNS,
      rows.map((row) => [row.symbol, row.quantity, row.average, row.last, row.value, row.pnl]),
    );
    grid.classList.add("acc-holdings");
    grid.tBodies[0].querySelectorAll("tr").forEach((tr, index) => {
      const row = rows[index];
      tr.dataset.symbol = row.symbol;
      // 폰 폭에서 줄을 접을 때 칸 이름을 앞에 보인다(account.css).
      [...tr.cells].forEach((cell, at) => (cell.dataset.label = HOLDING_COLUMNS[at]));
      if (row.trend !== null) tr.cells[tr.cells.length - 1].dataset.trend = row.trend;
    });
    body = el("div", "acc-table-wrap");
    body.append(grid);
  }
  const node = card("보유 종목", body);
  node.dataset.role = "holdings";
  return node;
}

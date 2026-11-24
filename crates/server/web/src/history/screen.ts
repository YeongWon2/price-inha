// 히스토리 화면(server 29차 2번 · F7-7 · F7-9 · C30 · V83 · V84 · E57): 탭 셋 — 매매(목록 · 근거 펼치기 · 더 보기 ·
// 상태 거르기) · 하루 정리(날짜 고르기) · 승인 기록(목록 · 더 보기 · 상태 거르기).
// 서버는 `paths.ts`의 경로로만 부른다. 3D 라이브러리에 닿지 않는다(N21).
//
// 화면 계약(시험이 고르는 표시, 표 · 줄은 view.ts):
// - 띄우면 세션을 얻고(`POST /api/session`) 매매 탭을 고른 채 매매 목록(`GET /api/trades`, 처음 쪽)을 읽어 그린다.
// - 탭 단추를 누르면 그 판만 보인다. 하루 정리 · 승인 기록 판은 처음 고를 때 읽는다(하루 정리는 날짜 없이 —
//   오늘, 승인 기록은 처음 쪽). 다시 고르면 다시 읽지 않아도 된다.
// - 매매 줄의 펼치기 단추 → `GET /api/trades/{id}` → 그 줄 안에 근거 요약과 상태 흐름. 다시 누르면 접힌다.
//   따로 있는 근거 경로(매매 하나 아래 basis)는 부르지 않는다(매매 하나에 근거가 함께 온다).
// - "더 보기"는 응답의 `next`가 있을 때만 있고, 누르면 `before=next`로 다음 쪽을 읽어 목록 뒤에 이어 붙인다.
//   거르기 값이 있으면 같은 값을 함께 보낸다.
// - 상태 거르기를 바꾸면(`change`) `before` 없이 처음 쪽부터 다시 읽어 목록을 바꾼다(빈 값은 거르지 않음):
//   매매 `?state=`, 승인 기록 `?status=`.
// - 하루 정리의 날짜를 바꾸면(`change`) `GET /api/daily-summary?date=YYYY-MM-DD`로 읽어 그린다. 날짜 칸의 값은
//   보이는 정리의 `date`다.
// - 읽기에 실패하면 그 판에 "불러오지 못함 · 다시 시도" 오류 줄(`role="alert"`)과 `button[data-action="retry"]`(E57),
//   다시 시도하면 같은 것을 다시 읽는다. 메뉴 · 다른 판은 그대로다.
// - 탭 줄은 좌우 방향키 · Home · End로 오간다(옮긴 탭을 고르고 포커스를 둔다).
import "../ui/admin.css";
import "./history.css";
import { API, tradePath } from "../paths";
import { LOAD_FAILED, call, el } from "../ui/call";
import { button, card, emptyState, errorLine, loading } from "../ui/parts";
import {
  approvalRows,
  basisLines,
  flowRows,
  sideText,
  tradeRows,
  type ApprovalsReply,
  type DailySummary,
  type TradeDetail,
  type TradesReply,
} from "./model";
import {
  TABS,
  renderApprovalFilter,
  renderApprovalRows,
  renderDetail,
  renderHead,
  renderSummary,
  renderTabs,
  renderTradeFilter,
  renderTradeRows,
  selectTab,
  type HistoryTab,
} from "./view";

/** JSON 몸통을 읽는다. 실패(상태 · 모양)면 던진다. */
async function read<T>(url: string): Promise<T> {
  const reply = await call("GET", url);
  if (!reply.ok || typeof reply.body !== "object" || reply.body === null) throw new Error(`${url} ${reply.status}`);
  return reply.body as T;
}

/** 경로에 물음 값을 붙인다(빈 값은 뺀다). */
function withQuery(path: string, values: Record<string, string | number | null>): string {
  const query = new URLSearchParams();
  for (const [name, value] of Object.entries(values)) {
    if (value !== null && value !== "") query.set(name, String(value));
  }
  const text = query.toString();
  return text ? `${path}?${text}` : path;
}

/** 오류 줄과 다시 시도 단추. */
function failed(again: () => void): HTMLElement {
  const retry = button("다시 시도", "accent");
  retry.dataset.action = "retry";
  retry.addEventListener("click", again);
  const box = el("div", "his-failed");
  box.append(errorLine(LOAD_FAILED), retry);
  return box;
}

/** 빈 상태 줄(`data-role="empty"`). */
function empty(text: string): HTMLElement {
  const node = emptyState(text);
  node.dataset.role = "empty";
  return node;
}

/** 쪽 넘기는 목록 하나(매매 · 승인 기록): 거르기 · 목록 · 더 보기 · 오류. */
class PagedList<Reply> {
  readonly list: HTMLElement;
  private readonly body = el("div", "his-list-body");
  private readonly foot = el("div", "his-foot");
  private filter = "";
  loaded = false;

  constructor(
    private readonly spec: {
      path: string;
      filterName: string;
      role: string;
      head: HTMLElement;
      emptyText: string;
      rows: (reply: Reply) => HTMLElement[];
      next: (reply: Reply) => number | null;
    },
    filterBox: HTMLElement,
  ) {
    this.list = el("ul", "his-list");
    this.list.dataset.role = spec.role;
    const select = filterBox.querySelector("select");
    select?.addEventListener("change", () => {
      this.filter = select.value;
      void this.load(null);
    });
  }

  /** 판에 붙일 덩이. */
  parts(): HTMLElement[] {
    return [this.body, this.foot];
  }

  /** `before`가 없으면 처음 쪽부터 다시, 있으면 다음 쪽을 이어 붙인다. */
  async load(before: number | null): Promise<void> {
    if (before === null) this.body.replaceChildren(loading());
    const more = this.foot.querySelector<HTMLButtonElement>("button");
    if (more) more.disabled = true;
    const filter = this.filter;
    let reply: Reply;
    try {
      reply = await read<Reply>(withQuery(this.spec.path, { before, [this.spec.filterName]: filter }));
    } catch {
      const again = (): void => void this.load(before);
      if (before === null) this.body.replaceChildren(failed(again));
      else this.foot.replaceChildren(failed(again));
      return;
    }
    if (filter !== this.filter) return;
    this.loaded = true;
    const rows = this.spec.rows(reply);
    if (before === null) {
      this.list.replaceChildren(...rows);
      this.body.replaceChildren(rows.length === 0 ? empty(this.spec.emptyText) : this.wrap());
    } else {
      this.list.append(...rows);
    }
    this.renderMore(this.spec.next(reply));
  }

  private wrap(): HTMLElement {
    const box = el("div", "his-table");
    box.append(this.spec.head, this.list);
    return box;
  }

  private renderMore(next: number | null): void {
    if (next === null) {
      this.foot.replaceChildren();
      return;
    }
    const more = button("더 보기");
    more.dataset.action = "more";
    more.addEventListener("click", () => void this.load(next));
    this.foot.replaceChildren(more);
  }
}

/** 화면 하나의 상태와 그리기. */
class HistoryScreen {
  private readonly tabs: HTMLElement;
  private readonly panels: Record<HistoryTab, HTMLElement>;
  private readonly trades: PagedList<TradesReply>;
  private readonly approvals: PagedList<ApprovalsReply>;
  private readonly summaryBody = el("div", "his-summary-body");
  private readonly dateInput = el("input", "ui-input his-date");
  private summaryLoaded = false;
  /** 펼친 줄의 근거(다시 펼칠 때 다시 읽지 않는다). */
  private readonly details = new Map<number, HTMLElement>();

  constructor(main: HTMLElement) {
    const { tabs, panels } = renderTabs("trades");
    this.tabs = tabs;
    this.panels = panels;
    tabs.addEventListener("click", (event) => {
      const tab = event.target instanceof Element ? event.target.closest<HTMLElement>('[role="tab"]') : null;
      if (tab?.dataset.tab) this.choose(tab.dataset.tab as HistoryTab);
    });
    tabs.addEventListener("keydown", (event) => this.onTabKey(event));

    const tradeFilter = renderTradeFilter();
    this.trades = new PagedList<TradesReply>(
      {
        path: API.trades,
        filterName: "state",
        role: "trades",
        head: renderHead("his-trade-cols", ["번호", "종목", "상태", "만든 시각", "판단 시각", ""]),
        emptyText: "아직 매매가 없습니다. 팀이 종목을 논의하기 시작하면 여기에 쌓입니다.",
        rows: (reply) => renderTradeRows(tradeRows(reply)),
        next: (reply) => reply.next,
      },
      tradeFilter,
    );
    this.trades.list.addEventListener("click", (event) => this.onTradeClick(event));
    panels.trades.append(
      card("매매", this.lead("줄을 누르면 근거와 상태 흐름이 펼쳐집니다."), tradeFilter, ...this.trades.parts()),
    );

    const approvalFilter = renderApprovalFilter();
    this.approvals = new PagedList<ApprovalsReply>(
      {
        path: API.approvalLog,
        filterName: "status",
        role: "approvals",
        head: renderHead("his-approval-cols", ["승인 번호", "결과", "요약", "시각"]),
        emptyText: "아직 승인 요청이 없습니다.",
        rows: (reply) => renderApprovalRows(approvalRows(reply)),
        next: (reply) => reply.next,
      },
      approvalFilter,
    );
    panels.approvals.append(
      card("승인 기록", this.lead("요약의 비밀 값은 가려져 있습니다."), approvalFilter, ...this.approvals.parts()),
    );

    panels.summary.append(card("하루 정리", this.datePicker(), this.summaryBody));
    main.append(tabs, ...TABS.map(([name]) => panels[name]));
  }

  private lead(text: string): HTMLElement {
    return el("p", "his-lead", text);
  }

  private datePicker(): HTMLElement {
    const box = el("label", "his-filter");
    this.dateInput.type = "date";
    this.dateInput.dataset.role = "date";
    this.dateInput.addEventListener("change", () => void this.loadSummary(this.dateInput.value || null));
    box.append(el("span", "his-filter-label", "날짜"), this.dateInput);
    return box;
  }

  /** 첫 그림: 매매 목록. */
  async start(): Promise<void> {
    await this.trades.load(null);
  }

  private choose(name: HistoryTab): void {
    selectTab(this.tabs, this.panels, name);
    if (name === "summary" && !this.summaryLoaded) void this.loadSummary(null);
    if (name === "approvals" && !this.approvals.loaded) void this.approvals.load(null);
  }

  /** 좌우 방향키 · Home · End로 탭을 옮긴다. */
  private onTabKey(event: KeyboardEvent): void {
    const names = TABS.map(([name]) => name);
    const current = names.findIndex((name) => this.panels[name].hidden === false);
    const moves: Record<string, number> = {
      ArrowRight: (current + 1) % names.length,
      ArrowLeft: (current - 1 + names.length) % names.length,
      Home: 0,
      End: names.length - 1,
    };
    if (!(event.key in moves)) return;
    event.preventDefault();
    const name = names[moves[event.key]];
    this.choose(name);
    this.tabs.querySelector<HTMLElement>(`[data-tab="${name}"]`)?.focus();
  }

  /** 하루 정리를 읽는다(`date`가 없으면 오늘). */
  private async loadSummary(date: string | null): Promise<void> {
    this.summaryBody.replaceChildren(loading());
    try {
      const summary = await read<DailySummary>(withQuery(API.dailySummary, { date }));
      this.summaryLoaded = true;
      this.dateInput.value = summary.date;
      if (date === null) this.dateInput.max = summary.date;
      this.summaryBody.replaceChildren(renderSummary(summary));
    } catch {
      this.summaryBody.replaceChildren(failed(() => void this.loadSummary(date)));
    }
  }

  private onTradeClick(event: Event): void {
    const toggle =
      event.target instanceof Element ? event.target.closest<HTMLButtonElement>('button[data-action="toggle"]') : null;
    const item = toggle?.closest<HTMLElement>("[data-trade]");
    if (toggle && item) void this.toggle(Number(item.dataset.trade), item, toggle);
  }

  /** 줄을 펼치거나 접는다. 처음 펼칠 때 매매 하나를 읽는다. */
  private async toggle(id: number, item: HTMLElement, toggle: HTMLButtonElement): Promise<void> {
    const open = toggle.getAttribute("aria-expanded") === "true";
    const shown = this.details.get(id);
    if (open) {
      toggle.setAttribute("aria-expanded", "false");
      if (shown) shown.hidden = true;
      return;
    }
    toggle.setAttribute("aria-expanded", "true");
    if (shown) {
      shown.hidden = false;
      return;
    }
    item.querySelector(".his-detail-wait")?.remove();
    const wait = el("div", "his-detail-wait");
    wait.append(loading());
    item.append(wait);
    try {
      const detail = await read<TradeDetail>(tradePath(id));
      const side = `방향 ${sideText(detail.side)} · 수량 ${detail.quantity === null ? "없음" : `${detail.quantity}주`}`;
      const node = renderDetail(basisLines(detail), flowRows(detail), side);
      node.id = `history-trade-${id}`;
      this.details.set(id, node);
      wait.replaceWith(node);
      node.hidden = toggle.getAttribute("aria-expanded") !== "true";
    } catch {
      toggle.setAttribute("aria-expanded", "false");
      wait.replaceChildren(errorLine("근거를 불러오지 못했습니다. 줄을 다시 눌러 주세요."));
    }
  }
}

/**
 * 본문 자리(`main`)에 히스토리 화면을 그린다: 세션을 얻고 매매 목록을 읽어 그린다.
 * 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
 */
export async function mountHistory(main: HTMLElement | null): Promise<void> {
  if (!main) return;
  const screen = new HistoryScreen(main);
  try {
    await call("POST", API.session);
  } catch {
    // 세션을 얻지 못해도 읽기를 해 본다: 실패하면 오류 줄과 다시 시도가 보인다.
  }
  await screen.start();
}

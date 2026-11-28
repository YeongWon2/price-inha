// 사무실 근거 패널(server 50차 · F7-7 · C23 · V68 · V69): 논의 줄의 종목 표시를 누르면 그 종목의 가장 최근 매매의
// 근거 카드를, 매매 번호로 열면 그 매매의 근거 카드를 채팅 옆 패널에 보인다(히스토리 화면과 같은 카드).
// 서버는 `paths.ts`의 경로로만 부른다: 매매 목록 `GET /api/trades?max=` · 근거 `GET /api/trades/{id}/basis`.
//
// 모양(시험이 고르는 표시): 패널 `section[data-role="basis-panel"]`(처음엔 숨김), 제목 `[data-part="title"]`,
// 닫기 `button[data-action="close-basis"]`, 본문 `[data-role="basis-body"]`(카드 · 안내 글 · 오류 줄 `role="alert"`).
import "./basis-panel.css";
import { API, tradeBasisPath } from "../paths";
import { readBasisCard } from "../ui/basis-card";
import { renderBasisCard } from "../ui/basis-card-view";
import { discussionEndKind, discussionEndText, type EndFields } from "../ui/discussion-end";

/** `fetch`와 같은 모양(시험에서 바꿔 끼운다). */
export type FetchLike = (input: string, init?: RequestInit) => Promise<Response>;

/** 근거 패널. */
export interface BasisPanel {
  readonly element: HTMLElement;
  /** 그 매매의 근거 카드를 연다. */
  openTrade(id: number): Promise<void>;
  /** 그 종목의 가장 최근 매매의 근거 카드를 연다. 매매가 없으면 안내 글. */
  openSymbol(symbol: string): Promise<void>;
  /** 닫는다(숨긴다). */
  close(): void;
}

/** 종목의 최근 매매를 찾을 때 읽는 매매 목록 한 쪽의 크기. */
const LOOKUP_MAX = 50;
const TITLE = "근거 카드";
const FAILED = "근거를 불러오지 못했어요. 다시 눌러 주세요.";

function make<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

async function readJson(fetcher: FetchLike, url: string): Promise<unknown> {
  const reply = await fetcher(url, { credentials: "same-origin", cache: "no-store" });
  if (!reply.ok) throw new Error(`${url} ${reply.status}`);
  return reply.json();
}

/** 매매 목록 응답에서 그 종목의 첫(가장 최근) 매매 번호. */
function latestOf(list: unknown, symbol: string): number | null {
  const trades = typeof list === "object" && list !== null ? (list as { trades?: unknown }).trades : null;
  if (!Array.isArray(trades)) return null;
  for (const item of trades as unknown[]) {
    if (typeof item !== "object" || item === null) continue;
    const row = item as { id?: unknown; symbol?: unknown };
    if (row.symbol === symbol && typeof row.id === "number") return row.id;
  }
  return null;
}

/** (69차) 근거 응답이 끝난 논의(지켜봄 · 끝나지 못함)면 카드 위에 둘 줄 `[data-role="discussion-end"]`. */
function endLine(reply: unknown): Node[] {
  const fields = typeof reply === "object" && reply !== null ? (reply as EndFields) : {};
  const kind = discussionEndKind(fields);
  const text = discussionEndText(fields);
  if (kind === null || text === null) return [];
  const line = make("p", "basis-panel-note", text);
  line.dataset.role = "discussion-end";
  line.dataset.ended = kind;
  return [line];
}

/** 자리(`host`) 끝에 근거 패널을 붙인다. */
export function mountBasisPanel(host: HTMLElement, fetcher: FetchLike): BasisPanel {
  const element = make("section", "basis-panel");
  element.dataset.role = "basis-panel";
  element.setAttribute("aria-label", TITLE);
  element.hidden = true;
  const head = make("div", "basis-panel-head");
  const title = make("h2", "basis-panel-title", TITLE);
  title.dataset.part = "title";
  const close = make("button", "basis-panel-close", "닫기");
  close.type = "button";
  close.dataset.action = "close-basis";
  head.append(title, close);
  const body = make("div", "basis-panel-body");
  body.dataset.role = "basis-body";
  body.setAttribute("aria-live", "polite");
  element.append(head, body);
  host.append(element);

  const show = (heading: string, ...nodes: Node[]) => {
    title.textContent = heading;
    body.replaceChildren(...nodes);
    element.hidden = false;
  };
  const failed = () => {
    const line = make("p", "basis-panel-error", FAILED);
    line.setAttribute("role", "alert");
    show(TITLE, line);
  };
  const openTrade = async (id: number) => {
    try {
      const reply = await readJson(fetcher, tradeBasisPath(id));
      const card = readBasisCard(reply);
      show(`${TITLE} · 매매 ${id}번`, ...endLine(reply), renderBasisCard(card));
    } catch {
      failed();
    }
  };
  const openSymbol = async (symbol: string) => {
    let id: number | null;
    try {
      id = latestOf(await readJson(fetcher, `${API.trades}?max=${LOOKUP_MAX}`), symbol);
    } catch {
      failed();
      return;
    }
    if (id === null) {
      show(`${TITLE} · ${symbol}`, make("p", "basis-panel-note", `${symbol} 종목의 매매가 아직 없어요.`));
      return;
    }
    await openTrade(id);
  };
  const panel: BasisPanel = {
    element,
    openTrade,
    openSymbol,
    close: () => {
      element.hidden = true;
    },
  };
  close.addEventListener("click", () => panel.close());
  return panel;
}

/** 이벤트가 난 곳의 종목 표시(`.topic[data-symbol]`)의 종목. */
function symbolAt(event: Event): string | null {
  const target = event.target instanceof Element ? event.target.closest<HTMLElement>(".topic[data-symbol]") : null;
  return target?.dataset.symbol ?? null;
}

/** 채팅 줄(`log`)의 종목 표시(`.topic[data-symbol]`)를 누르거나 Enter를 치면 패널로 그 종목을 연다. */
export function bindTopicClicks(log: HTMLElement, panel: BasisPanel): void {
  log.addEventListener("click", (event) => {
    const symbol = symbolAt(event);
    if (symbol) void panel.openSymbol(symbol);
  });
  log.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    const symbol = symbolAt(event);
    if (!symbol) return;
    event.preventDefault();
    void panel.openSymbol(symbol);
  });
}

// 사무실 옆 패널의 "어제 메모" 카드(additions.md 23번 · R7-4 · C25 · V70).
//
// 화면 계약(시험이 고르는 표시):
// - `mountMemoCard(root, fetcher)`는 자리(`#memos`)에 카드(`[data-role="memo-card"]`, 제목 `h2` "어제 메모")를 단다.
//   `refresh()`가 `GET /api/memos?days=7`을 읽어 그린다(서버가 정한 순서 · 판정 그대로).
// - 정산된 메모를 많아야 셋 그린다(`[data-role="memo-list"] li[data-trade][data-verdict]`: 종목 · 결정 · 결과 ·
//   수익률 · 보유일 줄과 교훈). 정산 대기가 있으면 `[data-role="memo-pending"]` "정산 대기 N건". 정산된 메모가
//   없으면 빈 상태 글(`[data-role="empty"]`). 읽기에 실패하면 오류 줄(`role="alert"`).
// - `onEvent(kind)`: 정산이 끝났다는 `office.memo` 이벤트면 다시 읽는다(다른 것은 무시).
// 글은 글자로만 넣는다(마크업으로 읽지 않는다). 3D 라이브러리에 닿지 않는다.
import "./memo-card.css";
import { API } from "../paths";
import { cardMemos, pendingCount, readMemoList, type MemoRow } from "../ui/memo-model";

/** 같은 출처 요청 자리(시험이 바꿔 끼운다). */
export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

/** 정산이 끝났을 때 서버가 내는 이벤트(C25). */
export const KIND_OFFICE_MEMO = "office.memo";
/** 카드가 읽는 최근 날수(C25 `?days=7`). */
export const MEMO_CARD_DAYS = 7;

const EMPTY_TEXT = "아직 정산된 결정 메모가 없습니다. 팀이 사거나 판 결정은 5거래일 뒤 수익률과 교훈으로 정산됩니다.";
const FAILED_TEXT = "어제 메모를 불러오지 못했습니다. 잠시 뒤 다시 읽습니다.";

/** 단 카드. */
export interface MemoCard {
  /** 다시 읽어 그린다. */
  refresh(): Promise<void>;
  /** 실시간 이벤트를 받는다(`office.memo`면 다시 읽는다). */
  onEvent(kind: unknown): Promise<void>;
}

function node<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const made = document.createElement(tag);
  made.className = className;
  if (text !== undefined) made.textContent = text;
  return made;
}

/** 정산된 메모 한 줄. */
function memoItem(row: MemoRow): HTMLElement {
  const item = node("li", "memo-item");
  item.dataset.trade = String(row.trade);
  item.dataset.verdict = row.verdict;
  const head = node("div", "memo-head");
  const result = node("span", "memo-result", row.result);
  result.dataset.verdict = row.verdict;
  const facts = [row.symbol, row.decision, row.returnText, row.holdText].filter((part) => part !== "");
  head.append(result, node("span", "memo-facts", facts.join(" · ")));
  item.append(head, node("p", "memo-lesson", row.lesson));
  return item;
}

/** 카드 몸통을 그린다. */
function render(body: HTMLElement, rows: MemoRow[], pending: number): void {
  const parts: HTMLElement[] = [];
  if (rows.length === 0) {
    const empty = node("p", "memo-empty", EMPTY_TEXT);
    empty.dataset.role = "empty";
    parts.push(empty);
  } else {
    const list = node("ul", "memo-list");
    list.dataset.role = "memo-list";
    list.append(...rows.map(memoItem));
    parts.push(list);
  }
  if (pending > 0) {
    const waiting = node("p", "memo-pending", `정산 대기 ${pending}건`);
    waiting.dataset.role = "memo-pending";
    parts.push(waiting);
  }
  body.replaceChildren(...parts);
}

/** 자리에 어제 메모 카드를 단다(읽기는 `refresh()`). */
export function mountMemoCard(root: HTMLElement, fetcher: Fetcher): MemoCard {
  const card = node("div", "memo-card");
  card.dataset.role = "memo-card";
  const body = node("div", "memo-body");
  card.append(node("h2", "memo-title", "어제 메모"), body);
  root.replaceChildren(card);
  const url = `${API.memos}?days=${MEMO_CARD_DAYS}`;

  const refresh = async (): Promise<void> => {
    try {
      const reply = await fetcher(url, { cache: "no-store" });
      if (!reply.ok) throw new Error(`${url} ${reply.status}`);
      const list = readMemoList(await reply.json());
      render(body, cardMemos(list), pendingCount(list));
    } catch {
      const failed = node("p", "memo-failed", FAILED_TEXT);
      failed.setAttribute("role", "alert");
      body.replaceChildren(failed);
    }
  };
  const onEvent = async (kind: unknown): Promise<void> => {
    if (kind === KIND_OFFICE_MEMO) await refresh();
  };
  return { refresh, onEvent };
}

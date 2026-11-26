// 히스토리 화면의 "결정 메모" 탭(additions.md 23번 · R7-4 · C25 · E54 · V70).
//
// 화면 계약(시험이 고르는 표시):
// - `mountMemoTab(panel, fetcher)`는 판에 카드 "결정 메모"를 단다. 처음 `open()`에 `GET /api/memos?max=20`(첫 쪽)을
//   읽어 그린다. 다시 열면 다시 읽지 않는다(`loaded`).
// - 줄은 `[data-role="memos"] li[data-memo][data-trade][data-state]`이고 결정 날짜 · 종목 · 결정 · 상태 · 결과 ·
//   수익률 · 보유일, 그리고 교훈(정산됨) · 그때 까닭(정산 대기) · 정산 못 한 까닭(정산 못 함)을 보인다.
// - "더 보기"(`button[data-action="more"]`)는 `next`가 있을 때만 있고 누르면 `before=next`로 이어 붙인다.
// - 메모가 없으면 빈 상태 글(`[data-role="empty"]`), 읽기에 실패하면 오류 줄(`role="alert"`)과 다시 시도
//   (`button[data-action="retry"]`). `settled()`는 지금 읽는 것이 끝나면 풀린다(시험이 기다린다).
// 서버가 정한 순서 · 판정을 그대로 그린다. 글은 글자로만 넣는다. 3D 라이브러리에 닿지 않는다(N21).
import { API } from "../paths";
import { LOAD_FAILED, el } from "../ui/call";
import { memoRows, readMemoPage, type MemoRow } from "../ui/memo-model";
import { button, card, emptyState, errorLine, loading } from "../ui/parts";

/** 같은 출처 요청 자리(시험이 바꿔 끼운다). */
export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

/** 한 쪽의 줄 수. */
export const MEMO_PAGE_MAX = 20;

const EMPTY_TEXT = "아직 결정 메모가 없습니다. 팀이 사거나 팔기로 정하면 여기에 쌓이고, 5거래일 뒤 정산됩니다.";

/** 단 탭. */
export interface MemoTab {
  /** 처음 쪽을 읽었는가. */
  readonly loaded: boolean;
  /** 탭을 고를 때 부른다(처음 한 번만 읽는다). */
  open(): Promise<void>;
  /** 지금 읽는 것이 끝나면 풀린다. */
  settled(): Promise<void>;
}

function pageUrl(before: number | null): string {
  const query = new URLSearchParams();
  if (before !== null) query.set("before", String(before));
  query.set("max", String(MEMO_PAGE_MAX));
  return `${API.memos}?${query.toString()}`;
}

/** 줄 하나. */
function memoLine(row: MemoRow): HTMLElement {
  const item = el("li", "his-memo");
  item.dataset.memo = String(row.id);
  item.dataset.trade = String(row.trade);
  item.dataset.state = row.stateWord;
  const facts = [row.date, row.symbol, row.decision, row.state, row.result, row.returnText, row.holdText];
  item.append(el("div", "his-memo-head", facts.filter((part) => part !== "").join(" · ")));
  const note =
    row.stateWord === "settled" ? row.lesson : row.stateWord === "unsettled" ? `까닭: ${row.why}` : row.reason;
  if (note) item.append(el("p", "his-memo-note", row.stateWord === "pending" ? `그때 까닭: ${note}` : note));
  return item;
}

/** 판에 결정 메모 탭을 단다. */
export function mountMemoTab(panel: HTMLElement, fetcher: Fetcher): MemoTab {
  const list = el("ul", "his-list his-memos");
  list.dataset.role = "memos";
  const body = el("div", "his-list-body");
  const foot = el("div", "his-foot");
  panel.append(card("결정 메모", el("p", "his-lead", "팀의 사기 · 팔기 결정이 5거래일 뒤 어떻게 됐는지와 교훈입니다."), body, foot));
  let loaded = false;
  let pending: Promise<void> = Promise.resolve();

  const failed = (again: () => void): HTMLElement => {
    const retry = button("다시 시도", "accent");
    retry.dataset.action = "retry";
    retry.addEventListener("click", again);
    const box = el("div", "his-failed");
    box.append(errorLine(LOAD_FAILED), retry);
    return box;
  };

  const renderMore = (next: number | null): void => {
    if (next === null) {
      foot.replaceChildren();
      return;
    }
    const more = button("더 보기");
    more.dataset.action = "more";
    more.addEventListener("click", () => void run(next));
    foot.replaceChildren(more);
  };

  const load = async (before: number | null): Promise<void> => {
    if (before === null) body.replaceChildren(loading());
    try {
      const reply = await fetcher(pageUrl(before), { cache: "no-store" });
      if (!reply.ok) throw new Error(`memos ${reply.status}`);
      const page = readMemoPage(await reply.json());
      loaded = true;
      const rows = memoRows(page).map(memoLine);
      if (before === null) {
        list.replaceChildren(...rows);
        if (rows.length === 0) {
          const empty = emptyState(EMPTY_TEXT);
          empty.dataset.role = "empty";
          body.replaceChildren(empty);
        } else {
          body.replaceChildren(list);
        }
      } else {
        list.append(...rows);
      }
      renderMore(page.next);
    } catch {
      const again = (): void => void run(before);
      if (before === null) body.replaceChildren(failed(again));
      else foot.replaceChildren(failed(again));
    }
  };

  const run = (before: number | null): Promise<void> => {
    pending = load(before);
    return pending;
  };

  return {
    get loaded() {
      return loaded;
    },
    open: () => (loaded ? Promise.resolve() : run(null)),
    settled: () => pending,
  };
}

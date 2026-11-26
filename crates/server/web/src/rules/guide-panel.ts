// 규칙 화면의 "팀 지침(AI가 배운 것)" 묶음 하나의 상태와 서버 부르기(기능 L · 결정 30): 읽기(`GET /api/guidelines`) ·
// 끄기(`POST /api/guidelines/{id}/retire`, 그 컴퓨터만). 첫 화면을 가볍게(N21 · 관리 화면 묶음 60KB) 따로 불러온다.
//
// 화면 계약(시험이 고르는 표시) — 묶음 하나 `[data-role="guidelines"]`(카드 제목 "팀 지침(AI가 배운 것)") 안에:
// - 1층 규칙과 다르다는 한 줄 `[data-role="guidelines-note"]`.
// - 활성 목록 `[data-role="guidelines-active"]`: 줄 `[data-guideline="<번호>"]` 안에 종류 `[data-role="kind"]` · 글
//   `[data-role="text"]` · 조건 `[data-role="when"]` · 익절 비율 `[data-role="rate"]` · 쓰임 `[data-role="uses"]`,
//   그 컴퓨터면 `button[data-action="retire-guideline"]`. 비었으면 `[data-role="no-guidelines"]`.
// - 내린 것 `details[data-role="guidelines-retired"]`(접혀 있다): 줄마다 까닭 `[data-role="why"]`. 없으면 없다.
// - 원격이면 끄기 단추 대신 `[data-role="guidelines-readonly"]`. 끄기 결과 `[data-role="guidelines-result"]`.
// - 이 묶음만 못 읽으면 `[data-role="guidelines-load-failed"]`(`role="alert"`는 달지 않는다 — 다른 묶음은 그대로다).
import "./guide.css";
import { API, guidelineRetirePath } from "../paths";
import { call, el } from "../ui/call";
import { badge, button, card, emptyState, type BadgeTone } from "../ui/parts";
import {
  GUIDE_TITLE,
  KIND_LABEL,
  LAYER_NOTE,
  rateText,
  readGuidelines,
  retireMessage,
  usesText,
  whyText,
  type GuidelineKind,
  type GuidelineView,
  type GuidelinesReply,
  type RetireMessage,
} from "./guide-model";

const KIND_TONE: Readonly<Record<GuidelineKind, BadgeTone>> = {
  avoid: "warning",
  enter: "success",
  manage: "neutral",
};

/** `data-role`을 단 요소. */
function marked<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  role: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = el(tag, className, text);
  node.dataset.role = role;
  return node;
}

/** 지침 한 줄(활성 · 내린 것 공통). */
function guideItem(row: GuidelineView, retireButton: boolean): HTMLElement {
  const item = el("li", "rul-guide-item");
  item.dataset.guideline = row.id;
  const kind = badge(KIND_LABEL[row.kind], KIND_TONE[row.kind]);
  kind.dataset.role = "kind";
  const head = el("div", "rul-guide-head");
  head.append(kind, marked("span", "rul-guide-text", "text", row.text));
  const where = row.symbol ? ` · 종목 ${row.symbol}` : "";
  const stats = el("p", "rul-guide-stats");
  stats.append(marked("span", "", "rate", rateText(row)), marked("span", "", "uses", usesText(row)));
  item.append(head, marked("p", "rul-guide-when", "when", `언제: ${row.when}${where}`), stats);
  if (row.state === "retired") item.append(marked("p", "rul-guide-why", "why", whyText(row.retired_why)));
  if (retireButton) {
    const off = button("끄기");
    off.dataset.action = "retire-guideline";
    item.append(off);
  }
  return item;
}

/** 팀 지침 묶음. [`GuidePanel.element`]를 화면에 붙이고, 읽거나 끄면 그 안만 다시 그린다. */
export class GuidePanel {
  private readonly holder = el("div", "rul-guide-holder");
  private reply: GuidelinesReply | null = null;
  private result: RetireMessage | null = null;
  private busy = false;

  constructor() {
    this.holder.dataset.role = "guidelines";
    this.holder.addEventListener("click", (event) => this.onClick(event));
  }

  /** 화면에 붙일 자리(늘 같은 요소). */
  element(): HTMLElement {
    return this.holder;
  }

  /** 지금 지침을 읽어 다시 그린다. 못 읽으면 이 묶음만 못 읽음. */
  async load(): Promise<void> {
    try {
      const answer = await call("GET", API.guidelines);
      this.reply = answer.ok ? readGuidelines(answer.body) : null;
    } catch {
      this.reply = null;
    }
    this.render();
  }

  private render(): void {
    const body: Node[] = [marked("p", "rul-guide-note", "guidelines-note", LAYER_NOTE)];
    if (this.reply) {
      body.push(...this.listParts(this.reply));
    } else {
      const retry = button("다시 시도");
      retry.dataset.action = "retry-guidelines";
      const failed = "팀 지침을 불러오지 못했습니다. 잠시 뒤 다시 시도해 주세요.";
      body.push(marked("p", "rul-guide-failed", "guidelines-load-failed", failed), retry);
    }
    if (this.result) {
      const line = marked("p", `rul-guide-result rul-guide-${this.result.kind}`, "guidelines-result", this.result.text);
      body.push(line);
    }
    this.holder.replaceChildren(card(GUIDE_TITLE, ...body));
  }

  /** 활성 목록 · 원격 안내 · 내린 것 접기. */
  private listParts(reply: GuidelinesReply): Node[] {
    const parts: Node[] = [];
    if (reply.active.length === 0) {
      const empty = emptyState("아직 배운 지침이 없습니다. 복기 · 실수 · 결정 메모에서 같은 것이 되풀이되면 생깁니다.");
      empty.dataset.role = "no-guidelines";
      parts.push(empty);
    } else {
      const list = marked("ul", "rul-guide-list", "guidelines-active");
      list.append(...reply.active.map((row) => guideItem(row, reply.editable)));
      parts.push(list);
    }
    if (!reply.editable) {
      parts.push(marked("p", "rul-guide-readonly", "guidelines-readonly", "지침 끄기는 이 컴퓨터에서만 할 수 있습니다."));
    }
    if (reply.retired.length > 0) {
      const folded = marked("details", "rul-guide-retired", "guidelines-retired");
      const list = el("ul", "rul-guide-list");
      list.append(...reply.retired.map((row) => guideItem(row, false)));
      folded.append(el("summary", "rul-guide-summary", `내린 지침 ${reply.retired.length}개`), list);
      parts.push(folded);
    }
    return parts;
  }

  private onClick(event: Event): void {
    const target = event.target instanceof Element ? event.target.closest<HTMLButtonElement>("button[data-action]") : null;
    if (!target) return;
    if (target.dataset.action === "retry-guidelines") {
      void this.load();
      return;
    }
    const id = target.closest<HTMLElement>("[data-guideline]")?.dataset.guideline;
    if (target.dataset.action === "retire-guideline" && id) void this.retire(id, target);
  }

  /** 지침 하나를 끈다. 끄는 동안 그 단추를 막고, 끝나면 결과를 남기고 다시 읽는다. */
  private async retire(id: string, target: HTMLButtonElement): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    target.disabled = true;
    try {
      const reply = await call("POST", guidelineRetirePath(id));
      this.result = retireMessage(reply.status, reply.body);
    } catch {
      this.result = retireMessage(0, null);
    }
    this.busy = false;
    await this.load();
  }
}

// 머리 위 ✋ · ✓ 표시와 승인 알림음 · 아이콘 범례를 사무실 화면에 잇는다(server 34차 1~5번, additions.md R5-10 · C27 ·
// E49 · E50 · N12 · V62). 상태는 순수 모듈(approval-bubbles · done-mark · chime · legend)이 갖고, 여기서는
// 이벤트 · 다시 읽기를 그 모듈에 넣고, 3D 층에서는 말풍선 층(bubbles3d)이 자리 잡은 이름표 · 말풍선 · 말풍선 줄을
// 피해 같은 후보 규칙(speech.ts `bubbleCandidates`)으로 표시를 놓는다. 가벼운 화면에는 담당 줄에 표시를 붙인다.
//
// - ✋: 기다리는 승인이 있는 담당 머리 위. 시간으로 사라지지 않는다. 누르면 그 승인 카드를 연다(없으면 만든다).
// - 알림음: 새 승인 이벤트에 한 번. 다시 읽어 채운 것에는 내지 않는다. 화면을 한 번 누른 뒤부터 소리를 낸다.
// - ✓: 일하던 담당(대기 · 생각 중)이 쉬게 되면 2.0초 + 0.5초 흐려짐. 새 일을 시작하면 바로 지운다.
// - 끝난 요청은 남기지 않는다: 이벤트로 닫힌 번호는 다시 읽기에서도 넣지 않고, 다시 읽기에 없는 카드는 닫는다.

import { soundEnabled } from "../ui/sound-pref";
import type { AgentId } from "./actor";
import { applyApproval, type FetchLike } from "./approval";
import { ApprovalBubbles, type HandBubble, type WaitingApproval } from "./approval-bubbles";
import { Chime } from "./chime";
import { DONE_FADE_MS, DONE_SHOW_MS, DoneMarks, type DoneShown } from "./done-mark";
import { markOf, renderLegend } from "./legend";
import { bubbleCandidates, insideScreen, overlap, type Rect, type Size } from "./speech";

/** 개선안 승인 요청이 생겼다는 이벤트(team `KIND_PROPOSAL`). 승인 카드 이벤트가 없어 다시 읽어 맞춘다. */
export const KIND_RULE_PROPOSAL = "rule_proposal";

/** 일하는 중인 일 상태(이것에서 쉼으로 바뀌면 일이 끝난 것이다). */
const BUSY = new Set(["queued", "thinking"]);
/** 표시와 이웃 사각형 사이 틈(px). */
const MARK_GAP = 4;

/** 한 프레임의 담당 머리 자리(말풍선 층이 넘긴다). */
export interface MarkSpot {
  id: string;
  visible: boolean;
  head: { x: number; y: number };
}

/** 3D 층의 표시 층. 말풍선 층이 매 프레임 자리 잡기 뒤에 부른다. */
export interface MarksLayer {
  /** `taken`: 이미 놓인 이름표 · 말풍선 · 말풍선 줄. `own`: 담당마다 머리 위 말풍선 자리(있으면). */
  place(now: number, spots: readonly MarkSpot[], taken: readonly Rect[], own: ReadonlyMap<string, Rect>, screen: Size): void;
  dispose(): void;
}

/** 표시 하나를 놓을 담당. */
export interface MarkItem {
  agent: string;
  head: { x: number; y: number };
  size: Size;
  /** 그 담당의 머리 위 말풍선 자리(있으면 그 옆도 후보다). */
  own?: Rect;
}

/**
 * 표시 자리 고르기(넘긴 순서대로): 머리 위 3×3 후보([`bubbleCandidates`]) 다음에 자기 말풍선의 오른쪽 · 왼쪽 · 위.
 * 화면 안이고 `taken` · 앞서 놓은 표시와 겹치지 않는 첫 자리. 없으면 화면 안인 첫 후보(그것도 없으면 첫 후보).
 */
export function placeMarks(items: readonly MarkItem[], taken: readonly Rect[], screen: Size): { agent: string; rect: Rect }[] {
  const placed: { agent: string; rect: Rect }[] = [];
  for (const item of items) {
    const { width, height } = item.size;
    const candidates = bubbleCandidates(item.head.x, item.head.y, item.size);
    if (item.own) {
      const o = item.own;
      candidates.push(
        { x: o.x + o.width + MARK_GAP, y: o.y, width, height },
        { x: o.x - width - MARK_GAP, y: o.y, width, height },
        { x: o.x + (o.width - width) / 2, y: o.y - height - MARK_GAP, width, height },
      );
    }
    const free = (r: Rect) =>
      insideScreen(r, screen) && !taken.some((t) => overlap(t, r)) && !placed.some((p) => overlap(p.rect, r));
    const rect = candidates.find(free) ?? candidates.find((r) => insideScreen(r, screen)) ?? candidates[0];
    placed.push({ agent: item.agent, rect });
  }
  return placed;
}

/** 사무실 화면에 붙인 표시들. */
export interface OfficeMarks {
  /** `office.approval` 이벤트 하나(새 요청이면 알림음). */
  onApproval(payload: unknown): void;
  /** 다시 읽은 `pending_approvals`로 맞춘다. `ringNew`면 새로 보인 번호에 알림음(개선안 이벤트 뒤), 아니면 소리 없이. */
  sync(list: unknown, ringNew: boolean): void;
  /** 담당의 일 상태가 바뀌었다(실시간 이벤트). */
  work(agent: string, before: string, after: string, now: number): void;
  /** 3D 층의 표시 층을 만든다(장면마다 하나). */
  layer(stage: HTMLElement): MarksLayer;
  /** 가벼운 화면 담당 줄에 붙일 표시들. */
  badges(agent: string, now: number): HTMLElement[];
  /** 표시가 바뀌면 부른다(가벼운 화면 다시 그리기). */
  onChange(listener: () => void): void;
}

/** 붙일 곳: 승인 카드 칸 · 부르기 · 담당 이름 · 범례 단추 자리 · 손짓을 받을 곳. */
export interface MarksOptions {
  approvals: HTMLElement;
  fetcher: FetchLike;
  names: Record<string, string>;
  legendRoot: HTMLElement;
  gestures: Pick<Document, "addEventListener">;
}

function makeAudio(): AudioContext {
  const Audio = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Audio) throw new Error("no audio");
  return new Audio();
}

/** 표시들을 만들어 잇는다. */
export function createOfficeMarks(options: MarksOptions): OfficeMarks {
  const board = new ApprovalBubbles();
  const done = new DoneMarks();
  const chime = new Chime(makeAudio, soundEnabled);
  /** 이벤트로 끝난 번호(승인은 다시 열리지 않는다 — 늦게 온 다시 읽기가 되살리지 않게). */
  const closed = new Set<string>();
  const listeners: (() => void)[] = [];
  const changed = () => { for (const listener of listeners) listener(); };
  const nameOf = (agent: string) => options.names[agent] ?? agent;

  // 소리는 화면을 한 번 누른 뒤부터(E50): 첫 손짓에 막힌 오디오를 푼다.
  const unlock = () => { void chime.unlock(); };
  options.gestures.addEventListener("pointerdown", unlock, { once: true });
  options.gestures.addEventListener("keydown", unlock, { once: true });
  renderLegend(options.legendRoot);

  function open(approval: WaitingApproval): void {
    const root = options.approvals;
    applyApproval(root, { number: approval.number, summary: approval.summary, short_code: approval.short_code, status: "pending" },
      options.fetcher);
    const card = [...root.querySelectorAll<HTMLElement>("[data-approval]")].find((c) => c.dataset.approval === approval.number);
    if (!card) return;
    card.tabIndex = -1;
    card.dataset.focus = "1";
    card.scrollIntoView({ block: "nearest" });
    card.focus({ preventScroll: true });
    window.setTimeout(() => { delete card.dataset.focus; }, 1600);
  }

  function press(agent: string): void {
    board.press(agent as AgentId, open);
  }

  /** 기다리지 않는 카드는 닫는다(이벤트를 놓쳐도 끝난 카드가 남지 않게). */
  function closeStaleCards(waiting: ReadonlySet<string>): void {
    for (const card of options.approvals.querySelectorAll<HTMLElement>("[data-approval]")) {
      const number = card.dataset.approval ?? "";
      if (!waiting.has(number) && card.dataset.deciding !== "1") card.remove();
    }
  }

  function handOf(agent: string, now: number): HandBubble | undefined {
    return board.bubble(agent as AgentId, now);
  }

  function handButton(hand: HandBubble, agent: string): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "mark-hand";
    button.dataset.mark = "approval";
    button.style.color = hand.color;
    button.addEventListener("click", (event) => { event.stopPropagation(); press(agent); });
    paintHand(button, hand, agent);
    return button;
  }

  function paintHand(button: HTMLElement, hand: HandBubble, agent: string): void {
    if (button.textContent !== hand.text) button.textContent = hand.text;
    const label = `${nameOf(agent)} · 승인 기다림 ${hand.count}건 — 승인 카드 열기`;
    if (button.getAttribute("aria-label") !== label) {
      button.setAttribute("aria-label", label);
      button.title = label;
    }
  }

  function doneSpan(shown: DoneShown): HTMLElement {
    const span = document.createElement("span");
    span.className = "mark-done";
    span.dataset.mark = "done";
    span.textContent = shown.text;
    span.style.color = shown.color;
    span.style.opacity = String(shown.opacity);
    span.setAttribute("role", "img");
    span.setAttribute("aria-label", markOf("done").label);
    return span;
  }

  function layer(stage: HTMLElement): MarksLayer {
    const host = document.createElement("div");
    host.className = "overhead-marks";
    stage.appendChild(host);
    const shown = new Map<string, { el: HTMLElement; hand: HTMLElement | null; done: HTMLElement | null }>();

    function element(agent: string) {
      let one = shown.get(agent);
      if (!one) {
        const el = document.createElement("div");
        el.className = "overhead-mark";
        el.dataset.agent = agent;
        host.appendChild(el);
        one = { el, hand: null, done: null };
        shown.set(agent, one);
      }
      return one;
    }

    /** 이번 프레임의 표시로 고친다. 크기가 바뀌었으면 참. */
    function refresh(agent: string, hand: HandBubble | undefined, check: DoneShown | undefined): boolean {
      const one = element(agent);
      let resized = false;
      if (hand && !one.hand) { one.hand = handButton(hand, agent); one.el.prepend(one.hand); resized = true; }
      else if (!hand && one.hand) { one.hand.remove(); one.hand = null; resized = true; }
      else if (hand && one.hand) {
        const before = one.hand.textContent;
        paintHand(one.hand, hand, agent);
        resized ||= before !== one.hand.textContent;
      }
      if (check && !one.done) { one.done = doneSpan(check); one.el.append(one.done); resized = true; }
      else if (!check && one.done) { one.done.remove(); one.done = null; resized = true; }
      else if (check && one.done) { one.done.style.opacity = String(check.opacity); }
      one.el.style.display = "";
      return resized;
    }

    const sizes = new Map<string, Size>();

    return {
      place(now, spots, taken, own, screen): void {
        const items: MarkItem[] = [];
        const drawn = new Set<string>();
        for (const spot of spots) {
          const hand = handOf(spot.id, now);
          const check = done.shown(spot.id, now);
          if (!spot.visible || (!hand && !check)) continue;
          const resized = refresh(spot.id, hand, check);
          let size = sizes.get(spot.id);
          if (!size || resized) {
            const box = element(spot.id).el.getBoundingClientRect();
            size = { width: box.width || 28, height: box.height || 26 };
            sizes.set(spot.id, size);
          }
          drawn.add(spot.id);
          items.push({ agent: spot.id, head: spot.head, size, own: own.get(spot.id) });
        }
        for (const { agent, rect } of placeMarks(items, taken, screen)) {
          element(agent).el.style.transform = `translate(${Math.round(rect.x)}px, ${Math.round(rect.y)}px)`;
        }
        for (const [agent, one] of shown) {
          if (!drawn.has(agent)) {
            one.el.style.display = "none";
            if (one.done) { one.done.remove(); one.done = null; sizes.delete(agent); }
          }
        }
      },
      dispose(): void {
        host.remove();
        shown.clear();
      },
    };
  }

  return {
    onApproval(payload: unknown): void {
      const outcome = board.apply(payload, Date.now());
      const number = (payload as { number?: unknown } | null)?.number;
      if (typeof number !== "string") return;
      if (outcome === "new") void chime.ring(number);
      if (outcome === "closed") closed.add(number);
      if (outcome !== "ignored") changed();
    },
    sync(list: unknown, ringNew: boolean): void {
      if (!Array.isArray(list)) return;
      const before = new Set(board.waiting().map((w) => w.number));
      const live = list.filter((row) => !closed.has(String((row as { number?: unknown } | null)?.number ?? "")));
      board.sync(live);
      const now = board.waiting().map((w) => w.number);
      if (ringNew) { for (const number of now) if (!before.has(number)) void chime.ring(number); }
      else chime.quiet(now);
      closeStaleCards(new Set(now));
      changed();
    },
    work(agent: string, before: string, after: string, now: number): void {
      if (BUSY.has(after)) {
        if (done.shown(agent, now)) { done.start(agent, now); changed(); }
        return;
      }
      if (after === "idle" && BUSY.has(before)) {
        done.finish(agent, now);
        changed();
        window.setTimeout(changed, DONE_SHOW_MS + DONE_FADE_MS + 50);
      }
    },
    layer,
    badges(agent: string, now: number): HTMLElement[] {
      const out: HTMLElement[] = [];
      const hand = handOf(agent, now);
      if (hand) out.push(handButton(hand, agent));
      const check = done.shown(agent, now);
      if (check) out.push(doneSpan(check));
      return out;
    },
    onChange(listener: () => void): void {
      listeners.push(listener);
    },
  };
}

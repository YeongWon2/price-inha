// 3D 사무실의 머리 위 말풍선 층(server 22차 3 · 4번, 원본 DeskRPG 동작 — docs/research/deskrpg.md 10절).
// 매 프레임 머리 위치를 화면에 투영한 점(actors3d `heads`)으로 자리를 고르고(speech.ts `placeBubbles`), 자리가 없으면
// 화면 모서리의 세로 말풍선 줄에 쌓는다. 지나가며 잡담(smalltalk.ts)도 여기서 돌린다(화면만, 서버에 보내지 않는다).

import { Smalltalk, type Walker } from "./smalltalk";
import { bubbleWidth, placeBubbles, type HeadSpot, type Rect, type SpeechBoard } from "./speech";

/** 머리 하나: 투영한 머리 점과 이름표 자리. `visible`이 거짓이면 화면 밖(카메라 뒤 포함)이다. */
export interface Head {
  id: string;
  x: number;
  y: number;
  label: Rect;
  visible: boolean;
}

/** 말풍선 층. */
export interface BubbleLayer {
  /** 한 프레임: 잡담을 보고, 보일 말풍선을 놓는다. `now`는 말풍선 판과 같은 밀리초 시계. */
  update(now: number, heads: readonly Head[], walkers: readonly Walker[], canSee: (a: string, b: string) => boolean,
    width: number, height: number): void;
  /** 잡담 때문에 멈춰 있는가. */
  paused(id: string, now: number): boolean;
  dispose(): void;
}

interface Shown { el: HTMLElement; text: string; size: { width: number; height: number } }

export function createBubbleLayer(labels: HTMLElement, board: SpeechBoard, names: Record<string, string>): BubbleLayer {
  const smalltalk = new Smalltalk();
  const lane = document.createElement("div");
  lane.className = "bubble-lane";
  labels.appendChild(lane);
  const shown = new Map<string, Shown>();

  function element(id: string): Shown {
    let one = shown.get(id);
    if (!one) {
      const el = document.createElement("div");
      el.className = "bubble";
      el.dataset.agent = id;
      el.dataset.name = names[id] ?? id;
      labels.appendChild(el);
      one = { el, text: "", size: { width: 0, height: 0 } };
      shown.set(id, one);
    }
    return one;
  }

  function hide(one: Shown): void {
    one.el.style.display = "none";
    if (one.el.parentElement === lane) { labels.appendChild(one.el); }
  }

  function talkSmall(now: number, walkers: readonly Walker[], canSee: (a: string, b: string) => boolean): void {
    // 대화 · 상태 말풍선이 있는 담당은 잡담하지 않는다.
    const ready = walkers.map((w) => ({ ...w, busy: w.busy || board.busy(w.id, now) }));
    const started = smalltalk.tick(ready, canSee, now);
    if (started) {
      // 새 잡담이 시작되면 그 둘의 옛 잡담 말풍선은 지운다.
      board.clear(started.first, "smalltalk");
      board.clear(started.second, "smalltalk");
    }
    for (const say of smalltalk.speaking(now)) {
      // 한 프레임씩 이어 붙인다: 말하는 시간이 끝나면 곧 사라진다.
      board.smalltalk(say.agent, say.text, now, now + 250);
    }
  }

  function update(now: number, heads: readonly Head[], walkers: readonly Walker[],
    canSee: (a: string, b: string) => boolean, width: number, height: number): void {
    talkSmall(now, walkers, canSee);
    const maxWidth = bubbleWidth(width);
    const spots: HeadSpot[] = [];
    const visible = new Set<string>();
    for (const head of heads) {
      const bubble = board.shown(head.id, now);
      const one = shown.get(head.id);
      if (!bubble) { if (one) { hide(one); } continue; }
      const el = element(head.id);
      const text = bubble.text;
      if (el.text !== text || el.el.style.maxWidth !== `${maxWidth}px`) {
        el.text = text;
        el.el.textContent = text;
        el.el.title = `${names[head.id] ?? head.id} · ${text}`;
        el.el.dataset.kind = bubble.kind;
        el.el.style.maxWidth = `${maxWidth}px`;
        el.el.style.display = "";
        el.size = { width: el.el.offsetWidth || maxWidth, height: el.el.offsetHeight || 24 };
      }
      if (!head.visible) { hide(el); continue; }
      visible.add(head.id);
      spots.push({ agent: head.id, x: head.x, y: head.y, bubble: el.size, label: head.label });
    }
    const placement = placeBubbles(spots, { width, height });
    for (const { agent, rect } of placement.placed) {
      const one = element(agent);
      if (one.el.parentElement === lane) { labels.appendChild(one.el); }
      one.el.style.display = "";
      one.el.style.transform = `translate(${Math.round(rect.x)}px, ${Math.round(rect.y)}px)`;
    }
    for (const agent of placement.lane) {
      const one = element(agent);
      one.el.style.display = "";
      one.el.style.transform = "";
      if (one.el.parentElement !== lane) { lane.appendChild(one.el); }
    }
    for (const agent of placement.hidden) { hide(element(agent)); }
    for (const [id, one] of shown) { if (!visible.has(id)) { hide(one); } }
  }

  return {
    update,
    paused: (id, now) => smalltalk.paused(id, now),
    dispose(): void {
      for (const one of shown.values()) { one.el.remove(); }
      shown.clear();
      lane.remove();
    },
  };
}

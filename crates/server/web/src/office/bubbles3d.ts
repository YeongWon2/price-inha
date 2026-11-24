// 3D 사무실의 이름표 · 머리 위 말풍선 · 말풍선 줄 층(server 22차 3 · 4번, 23차 1~5번 — 원본 DeskRPG `label-layout`,
// docs/research/deskrpg.md 10 · 11절). 매 프레임 담당마다 발 · 머리 투영점(actors3d `spots`)을 받아 layout.ts
// `layoutOffice`로 이름표 · 말풍선 · 말풍선 줄의 자리를 한 번에 정한다. 말풍선 글은 `이름 · 글`(speech.ts `bubbleText`),
// 글 없는 상태(생각 중 · 기다림 · 실패 표시만)는 머리 위가 아니라 줄에 `이름 · 💭`, 쉬는 중은 아무것도 없다.
// 지나가며 잡담(smalltalk.ts)도 여기서 돌린다(화면만, 서버에 보내지 않는다).

import { actorPriority, layoutOffice, type OfficeActor } from "./layout";
import { Smalltalk, type Walker } from "./smalltalk";
import { bubbleText, bubbleWhere, bubbleWidth, type Rect, type Size, type SpeechBoard } from "./speech";

/** 담당 하나의 이번 프레임 화면 자리(actors3d가 낸다). */
export interface ActorSpot {
  id: string;
  /** 발이나 머리가 화면 안인가(아니면 이름표 · 말풍선 모두 숨긴다). */
  visible: boolean;
  /** 지금 일(`idle` · `queued` · `thinking` · `failed`). */
  work: string;
  foot: { x: number; y: number };
  head: { x: number; y: number };
  tag: Size;
}

/** 말풍선 층. */
export interface BubbleLayer {
  /**
   * 한 프레임: 잡담을 보고, 이름표 · 말풍선 · 줄을 놓는다. `now`는 말풍선 판과 같은 밀리초 시계.
   * 이름표는 `placeTag`(그 자리 또는 `null`이면 숨김)로 놓는다.
   */
  update(now: number, spots: readonly ActorSpot[], walkers: readonly Walker[], canSee: (a: string, b: string) => boolean,
    width: number, height: number, placeTag: (id: string, rect: Rect | null) => void): void;
  /** 잡담 때문에 멈춰 있는가. */
  paused(id: string, now: number): boolean;
  dispose(): void;
}

interface Shown { el: HTMLElement; text: string; maxWidth: number; size: Size }

export function createBubbleLayer(labels: HTMLElement, board: SpeechBoard, names: Record<string, string>): BubbleLayer {
  const smalltalk = new Smalltalk();
  const rail = document.createElement("div");
  rail.className = "speech-rail";
  rail.style.display = "none";
  labels.appendChild(rail);
  const shown = new Map<string, Shown>();

  function element(id: string): Shown {
    let one = shown.get(id);
    if (!one) {
      const el = document.createElement("div");
      el.className = "bubble";
      el.dataset.agent = id;
      labels.appendChild(el);
      one = { el, text: "", maxWidth: 0, size: { width: 0, height: 0 } };
      shown.set(id, one);
    }
    return one;
  }

  function hide(one: Shown): void {
    one.el.style.display = "none";
    if (one.el.parentElement === rail) { labels.appendChild(one.el); }
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

  /** 말풍선 글을 바꾸고 크기를 잰다(머리 위 자리에서 잰다 — 줄 안이면 잠깐 꺼내 잰다). */
  function refresh(one: Shown, text: string, maxWidth: number): void {
    if (one.text === text && one.maxWidth === maxWidth) { return; }
    if (one.el.parentElement === rail) { labels.appendChild(one.el); }
    one.text = text;
    one.maxWidth = maxWidth;
    one.el.textContent = text;
    one.el.title = text;
    one.el.style.maxWidth = `${maxWidth}px`;
    one.el.style.display = "";
    one.size = { width: one.el.offsetWidth || maxWidth, height: one.el.offsetHeight || 24 };
  }

  function update(now: number, spots: readonly ActorSpot[], walkers: readonly Walker[],
    canSee: (a: string, b: string) => boolean, width: number, height: number,
    placeTag: (id: string, rect: Rect | null) => void): void {
    talkSmall(now, walkers, canSee);
    const maxWidth = bubbleWidth(width);
    const actors: OfficeActor[] = [];
    const drawn = new Set<string>();
    for (const spot of spots) {
      const bubble = board.shown(spot.id, now);
      const where = bubbleWhere(bubble);
      if (bubble && where !== "none") {
        const one = element(spot.id);
        refresh(one, bubbleText(names[spot.id] ?? spot.id, bubble), maxWidth);
        one.el.dataset.kind = bubble.kind;
        one.el.dataset.working = String(spot.work !== "idle");
        drawn.add(spot.id);
      }
      actors.push({
        agent: spot.id,
        visible: spot.visible,
        foot: spot.foot,
        head: spot.head,
        tag: spot.tag,
        bubble: where === "head" ? (shown.get(spot.id) as Shown).size : null,
        railed: where === "rail",
        priority: actorPriority(where === "head", spot.work),
      });
    }
    const layout = layoutOffice(actors, { width, height });
    const tags = new Map(layout.tags.map((t) => [t.agent, t.rect]));
    for (const spot of spots) { placeTag(spot.id, tags.get(spot.id) ?? null); }
    for (const { agent, rect } of layout.bubbles) {
      const one = element(agent);
      if (one.el.parentElement === rail) { labels.appendChild(one.el); }
      one.el.style.display = "";
      one.el.style.transform = `translate(${Math.round(rect.x)}px, ${Math.round(rect.y)}px)`;
    }
    const inRail = new Set(layout.rail?.agents ?? []);
    if (layout.rail) {
      const { rect, agents } = layout.rail;
      rail.style.display = "";
      rail.dataset.corner = layout.rail.corner;
      rail.style.transform = `translate(${Math.round(rect.x)}px, ${Math.round(rect.y)}px)`;
      rail.style.width = `${Math.round(rect.width)}px`;
      rail.style.maxHeight = `${Math.round(rect.height)}px`;
      agents.forEach((agent, i) => {
        const one = element(agent);
        one.el.style.display = "";
        one.el.style.transform = "";
        if (one.el.parentElement !== rail || rail.children[i] !== one.el) { rail.insertBefore(one.el, rail.children[i] ?? null); }
      });
    } else {
      rail.style.display = "none";
    }
    const placed = new Set(layout.bubbles.map((b) => b.agent));
    for (const [id, one] of shown) {
      if (!drawn.has(id) || (!placed.has(id) && !inRail.has(id))) { hide(one); }
    }
  }

  return {
    update,
    paused: (id, now) => smalltalk.paused(id, now),
    dispose(): void {
      for (const one of shown.values()) { one.el.remove(); }
      shown.clear();
      rail.remove();
    },
  };
}

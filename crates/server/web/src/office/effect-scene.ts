// 익절 · 손절 연출을 사무실 화면에 잇는다(server 24차 3번, additions.md R5-14 · C26 · E60 · N12 · V66). 줄 세우기는
// 순수 모듈(effects.ts)이 갖고, 여기서는 `office.effect` 이벤트를 줄에 넣고(탭이 숨어 있으면 버리고, 숨으면 줄을 비운다)
// 3D 층에서는 머리 위 표시 층(marks-scene `placeMarks`)과 같은 규칙으로 매매 담당 머리 위에 놓는다. 가벼운 화면에는
// 담당 줄에 붙인다. 이미 있는 표시 층(✋ · ✓)을 감싸 한 층처럼 장면에 넘긴다(장면 · 표시 층은 고치지 않는다).

import { EffectQueue, readEffect, type EffectShown } from "./effects";
import { placeMarks, type MarksLayer, type OfficeMarks } from "./marks-scene";

/** 연출 표시 요소 하나: 글자 · 수익률 글, 쉬운 뜻은 `aria-label`. 글은 textContent로만 넣는다. */
export function effectElement(shown: EffectShown, name: string): HTMLElement {
  const span = document.createElement("span");
  span.className = "mark-effect";
  span.dataset.mark = shown.mark;
  span.dataset.effect = shown.mark;
  span.style.color = shown.color;
  span.setAttribute("role", "img");
  span.setAttribute("aria-label", `${name} · ${shown.label}${shown.text ? ` ${shown.text}` : ""}`);
  const glyph = document.createElement("span");
  glyph.className = "mark-effect-glyph";
  glyph.textContent = shown.glyph;
  glyph.setAttribute("aria-hidden", "true");
  span.append(glyph);
  if (shown.text) {
    const text = document.createElement("span");
    text.className = "mark-effect-text";
    text.textContent = shown.text;
    text.setAttribute("aria-hidden", "true");
    span.append(text);
  }
  return span;
}

/** 사무실 화면의 연출. */
export interface OfficeEffects {
  /** `office.effect` 이벤트 하나. */
  onEffect(payload: unknown): void;
  /** 머리 위 표시(✋ · ✓)를 감싸 연출도 함께 그리게 한다. */
  wrap(marks: OfficeMarks): OfficeMarks;
}

/** 연출을 만든다. `names`는 담당 이름(쉬운 뜻에 쓴다). */
export function createEffects(names: Record<string, string>): OfficeEffects {
  const queue = new EffectQueue();
  const listeners: (() => void)[] = [];
  const changed = () => { for (const listener of listeners) listener(); };
  const nameOf = (agent: string) => names[agent] ?? agent;
  // 탭이 숨으면 쌓인 연출을 버린다(E60). 다시 보이면 새 연출부터 보인다.
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) { queue.clear(); changed(); }
  });

  function layer(stage: HTMLElement): MarksLayer {
    const host = document.createElement("div");
    host.className = "overhead-effects";
    stage.appendChild(host);
    let box: HTMLElement | null = null;
    let key = "";
    let size = { width: 64, height: 26 };
    const hide = () => { box?.remove(); box = null; key = ""; };
    return {
      place(now, spots, taken, _own, screen): void {
        const shown = queue.current(now);
        const spot = shown ? spots.find((s) => s.id === shown.agent && s.visible) : undefined;
        if (!shown || !spot) { hide(); return; }
        const next = `${shown.agent}|${shown.mark}|${shown.text}`;
        if (next !== key || !box) {
          hide();
          box = document.createElement("div");
          box.className = "overhead-effect";
          box.append(effectElement(shown, nameOf(shown.agent)));
          host.append(box);
          key = next;
          const rect = box.getBoundingClientRect();
          size = { width: rect.width || 64, height: rect.height || 26 };
        }
        const [placed] = placeMarks([{ agent: shown.agent, head: spot.head, size }], taken, screen);
        box.style.transform = `translate(${Math.round(placed.rect.x)}px, ${Math.round(placed.rect.y)}px)`;
      },
      dispose(): void {
        host.remove();
      },
    };
  }

  function wrap(marks: OfficeMarks): OfficeMarks {
    return {
      onApproval: (payload) => marks.onApproval(payload),
      sync: (list, ringNew) => marks.sync(list, ringNew),
      work: (agent, before, after, now) => marks.work(agent, before, after, now),
      layer(stage: HTMLElement): MarksLayer {
        const inner = marks.layer(stage);
        const own = layer(stage);
        return {
          place(now, spots, taken, ownRects, screen): void {
            inner.place(now, spots, taken, ownRects, screen);
            own.place(now, spots, taken, ownRects, screen);
          },
          dispose(): void { inner.dispose(); own.dispose(); },
        };
      },
      badges(agent: string, now: number): HTMLElement[] {
        const out = marks.badges(agent, now);
        const shown = queue.current(now);
        if (shown && shown.agent === agent) out.push(effectElement(shown, nameOf(agent)));
        return out;
      },
      onChange(listener: () => void): void {
        marks.onChange(listener);
        listeners.push(listener);
      },
    };
  }

  return {
    onEffect(payload: unknown): void {
      const effect = readEffect(payload);
      if (!effect) return;
      const now = Date.now();
      if (!queue.push(effect, now, document.hidden)) return;
      changed();
      // 가벼운 화면: 그 연출이 보이기 시작할 때 · 끝날 때 다시 그린다.
      const slot = queue.last();
      if (!slot) return;
      if (slot.start > now) window.setTimeout(changed, slot.start - now + 20);
      window.setTimeout(changed, slot.end - now + 20);
    },
    wrap,
  };
}

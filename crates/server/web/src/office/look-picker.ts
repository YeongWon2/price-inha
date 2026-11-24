// 담당 외형 고르기(server 20차 4번): 사무실 화면에서 담당을 고르고 원본 외형 50종 가운데 하나로 바꾼다.
// 고른 값은 서버에 남는다(다시 켜도 · 다른 기기에서도 같다). 원본처럼 분류(클래식 수트 · 비즈니스 캐주얼 ·
// 크리에이티브 · 리더십)로 묶어 이름과 한 줄 소개를 보인다. 이 파일은 사무실 첫 묶음에 들어가므로 3D · 자산을 부르지 않는다.

import { DEFAULT_LOOKS, LOOK_CATEGORIES, LOOKS, lookInfo } from "./looks";

/** 외형 고르기가 쓰는 것. */
export interface PickerOptions {
  panel: HTMLElement;
  agents: readonly string[];
  nameOf(id: string): string;
  current(): Record<string, string>;
  /** 서버에 바꾸기를 보낸다. 성공하면 서버가 준 담당 → 외형, 실패하면 거부. */
  choose(agent: string, look: string): Promise<Record<string, string>>;
  /** 바뀐 외형을 화면(3D)에 알린다. */
  applied(looks: Record<string, string>): void;
}

/** 외형 이름(모르면 id). */
export function lookName(id: string): string {
  const info = lookInfo(id);
  return info ? info.name : id;
}

/** 외형 고르기 칸을 만든다. 돌려준 함수로 연다(담당을 정해 열 수 있다). */
export function createLookPicker(o: PickerOptions): { open(agent?: string): void; close(): void } {
  let agent = o.agents[o.agents.length - 1];
  let busy = false;
  let message = "";

  function render(): void {
    const looks = o.current();
    o.panel.replaceChildren();
    const head = document.createElement("div");
    head.className = "looks-head";
    const title = document.createElement("strong");
    title.textContent = "담당 외형";
    const close = document.createElement("button");
    close.type = "button";
    close.textContent = "닫기";
    close.addEventListener("click", () => { o.panel.hidden = true; });
    head.append(title, close);
    const tabs = document.createElement("div");
    tabs.className = "looks-agents";
    tabs.setAttribute("role", "tablist");
    for (const id of o.agents) {
      const b = document.createElement("button");
      b.type = "button";
      b.setAttribute("role", "tab");
      b.setAttribute("aria-selected", String(id === agent));
      b.textContent = `${o.nameOf(id)} · ${lookName(looks[id] ?? DEFAULT_LOOKS[id] ?? "")}`;
      b.addEventListener("click", () => { agent = id; message = ""; render(); });
      tabs.append(b);
    }
    const note = document.createElement("p");
    note.className = "looks-note";
    note.setAttribute("role", "status");
    note.textContent = message;
    o.panel.append(head, tabs, note);
    for (const category of LOOK_CATEGORIES) {
      const group = document.createElement("section");
      group.className = "looks-group";
      const h = document.createElement("h3");
      h.textContent = category.label;
      const list = document.createElement("div");
      list.className = "looks-list";
      for (const look of LOOKS.filter((l) => l.category === category.id)) {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "look";
        b.disabled = busy;
        b.setAttribute("aria-pressed", String(looks[agent] === look.id));
        const name = document.createElement("strong");
        name.textContent = look.name;
        const sub = document.createElement("span");
        sub.textContent = look.subtitle;
        b.append(name, sub);
        b.addEventListener("click", () => { void pick(look.id); });
        list.append(b);
      }
      group.append(h, list);
      o.panel.append(group);
    }
  }

  async function pick(look: string): Promise<void> {
    if (busy) { return; }
    busy = true;
    message = "바꾸는 중…";
    render();
    try {
      const looks = await o.choose(agent, look);
      o.applied(looks);
      message = `${o.nameOf(agent)}: ${lookName(look)}(으)로 바꿨습니다.`;
    } catch {
      message = "바꾸지 못했습니다(연결을 확인하세요).";
    }
    busy = false;
    render();
  }

  return {
    open(which?: string) {
      if (which && o.agents.includes(which)) { agent = which; }
      message = "";
      o.panel.hidden = false;
      render();
    },
    close() { o.panel.hidden = true; },
  };
}

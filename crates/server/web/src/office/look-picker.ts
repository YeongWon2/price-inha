// 담당 외형 고르기(server 20차 4번): 사무실 화면에서 담당을 고르고 원본 외형 50종 가운데 하나로 바꾼다.
// 고른 값은 서버에 남는다(다시 켜도 · 다른 기기에서도 같다). 원본처럼 분류(클래식 수트 · 비즈니스 캐주얼 ·
// 크리에이티브 · 리더십)로 묶어 이름과 한 줄 소개를 보인다. 이 파일은 사무실 첫 묶음에 들어가므로 3D · 자산을 부르지 않는다.
// (다듬기 E 9번) `me`를 주면 맨 앞에 "나" 탭이 생긴다: 고르면 이름 칸 `input#me-name` · 이름 바꾸기
// `button[data-action="me-name"]`과 외형 목록(내 외형이 눌림)이 보이고, 바꾸기는 `me.save`로 간다.

import { DEFAULT_LOOKS, LOOK_CATEGORIES, LOOKS, lookInfo } from "./looks";
import { ME_TAB_LABEL, type MeLook, type MePick } from "./me-pick";

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
  /** (다듬기 E 9번) "나" 줄. 없으면 담당만 고른다. */
  me?: MePick;
}

/** "나"를 고른 자리(담당 id와 겹치지 않는다). */
const ME = "@me";
/** 실패 안내. */
const FAILED = "바꾸지 못했습니다(연결을 확인하세요).";

/** 외형 이름(모르면 id). */
export function lookName(id: string): string {
  const info = lookInfo(id);
  return info ? info.name : id;
}

function tabButton(text: string, selected: boolean, onClick: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.setAttribute("role", "tab");
  b.setAttribute("aria-selected", String(selected));
  b.textContent = text;
  b.addEventListener("click", onClick);
  return b;
}

/** 분류마다 외형 단추 묶음. `pressed`가 지금 외형, 누르면 `onPick`. */
function lookGroups(pressed: string | undefined, busy: boolean, onPick: (look: string) => void): HTMLElement[] {
  return LOOK_CATEGORIES.map((category) => {
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
      b.setAttribute("aria-pressed", String(pressed === look.id));
      const name = document.createElement("strong");
      name.textContent = look.name;
      const sub = document.createElement("span");
      sub.textContent = look.subtitle;
      b.append(name, sub);
      b.addEventListener("click", () => onPick(look.id));
      list.append(b);
    }
    group.append(h, list);
    return group;
  });
}

/** "나"의 이름 칸 · 이름 바꾸기 단추. */
function nameForm(now: MeLook | null, busy: boolean, onSave: (name: string) => void): HTMLElement {
  const form = document.createElement("div");
  form.className = "looks-me";
  const label = document.createElement("label");
  label.htmlFor = "me-name";
  label.textContent = "내 이름";
  const input = document.createElement("input");
  input.id = "me-name";
  input.type = "text";
  input.autocomplete = "off";
  input.value = now?.name ?? "";
  const save = document.createElement("button");
  save.type = "button";
  save.dataset.action = "me-name";
  save.textContent = "이름 바꾸기";
  save.disabled = busy;
  save.addEventListener("click", () => onSave(input.value.trim()));
  form.append(label, input, save);
  return form;
}

/** 외형 고르기 칸을 만든다. 돌려준 함수로 연다(담당을 정해 열 수 있다). */
export function createLookPicker(o: PickerOptions): { open(agent?: string): void; close(): void } {
  let agent = o.agents[o.agents.length - 1];
  let busy = false;
  let message = "";

  function head(): HTMLElement {
    const top = document.createElement("div");
    top.className = "looks-head";
    const title = document.createElement("strong");
    title.textContent = "담당 외형";
    const close = document.createElement("button");
    close.type = "button";
    close.textContent = "닫기";
    close.addEventListener("click", () => { o.panel.hidden = true; });
    top.append(title, close);
    return top;
  }

  function select(id: string): void { agent = id; message = ""; render(); }

  function render(): void {
    const looks = o.current();
    const me = o.me?.current() ?? null;
    o.panel.replaceChildren();
    const tabs = document.createElement("div");
    tabs.className = "looks-agents";
    tabs.setAttribute("role", "tablist");
    if (o.me) { tabs.append(tabButton(`${ME_TAB_LABEL} · ${me?.name ?? ""}`, agent === ME, () => select(ME))); }
    for (const id of o.agents) {
      const label = `${o.nameOf(id)} · ${lookName(looks[id] ?? DEFAULT_LOOKS[id] ?? "")}`;
      tabs.append(tabButton(label, id === agent, () => select(id)));
    }
    const note = document.createElement("p");
    note.className = "looks-note";
    note.setAttribute("role", "status");
    note.textContent = message;
    o.panel.append(head(), tabs, note);
    if (agent === ME) { o.panel.append(nameForm(me, busy, (name) => { if (name !== "") { void saveMe({ name }); } })); }
    const pressed = agent === ME ? me?.look : looks[agent];
    o.panel.append(...lookGroups(pressed, busy, (look) => { void (agent === ME ? saveMe({ look }) : pick(look)); }));
  }

  /** 바꾸기 하나를 잠근 채 돌린다: 됐으면 그 안내, 실패면 실패 안내. */
  async function run(work: () => Promise<string>): Promise<void> {
    if (busy) { return; }
    busy = true;
    message = "바꾸는 중…";
    render();
    try {
      message = await work();
    } catch {
      message = FAILED;
    }
    busy = false;
    render();
  }

  function pick(look: string): Promise<void> {
    return run(async () => {
      const looks = await o.choose(agent, look);
      o.applied(looks);
      return `${o.nameOf(agent)}: ${lookName(look)}(으)로 바꿨습니다.`;
    });
  }

  function saveMe(change: Partial<MeLook>): Promise<void> {
    const me = o.me;
    if (!me) { return Promise.resolve(); }
    return run(async () => {
      const next = await me.save(change);
      me.applied(next);
      return change.look ? `${ME_TAB_LABEL}: ${lookName(next.look)}(으)로 바꿨습니다.` : `이름을 ${next.name}(으)로 바꿨습니다.`;
    });
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

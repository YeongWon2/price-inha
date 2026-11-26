// (server 65차, 결정 34) 뜻 검색 카드의 "같은 집 다른 기기(사설망)의 Ollama 허용".
// - 켜기 칸 `input#semantic-lan`(checkbox): 켜야만 바꾸기 몸통에 `private_network: true`를 싣는다(끄면 지금 몸통 그대로).
// - 서버가 준 `outside` 줄("임베딩을 이 컴퓨터 밖(사설망 …)으로 보냄")을 `[data-role="semantic-outside"]`로 늘 보인다.
// 주소가 사설망인지는 화면이 판정하지 않는다(서버가 한다).
import { el } from "../ui/call";
import { field } from "../ui/parts";

/** 켜기 칸의 글. */
export const SEMANTIC_LAN_LABEL = "같은 집 다른 기기(사설망)의 Ollama 허용";

/** 켜기 칸과 "밖으로 보냄" 줄 자리. */
export interface SemanticLan {
  /** 카드에 넣을 마디(켜기 칸 · 줄 자리). */
  nodes: HTMLElement[];
  /** 켜기 칸이 켜졌나. */
  checked(): boolean;
  /** 서버 답으로 맞춘다. */
  paint(privateNetwork: boolean | undefined, outside: string | null | undefined): void;
}

/** 켜기 칸과 "밖으로 보냄" 줄을 만든다. */
export function semanticLan(privateNetwork: boolean | undefined, outside: string | null | undefined): SemanticLan {
  const holder = field(SEMANTIC_LAN_LABEL, "semantic-lan", "checkbox");
  const box = holder.querySelector("input") as HTMLInputElement;
  const line = el("div", "setup-extra-outside");
  const paint = (on: boolean | undefined, text: string | null | undefined): void => {
    box.checked = on === true;
    line.replaceChildren(...(typeof text === "string" && text !== "" ? [outsideLine(text)] : []));
  };
  paint(privateNetwork, outside);
  return { nodes: [holder, line], checked: () => box.checked, paint };
}

function outsideLine(text: string): HTMLElement {
  const node = el("p", "hint", text);
  node.dataset.role = "semantic-outside";
  return node;
}

// (server 65차, 결정 34) 설정 화면 뜻 검색 카드의 "같은 집 다른 기기(사설망) Ollama 허용": 켜야만 몸통에
// `private_network: true`를 싣고(끄면 지금 몸통 그대로), 서버가 준 "임베딩을 이 컴퓨터 밖(사설망 …)으로 보냄" 줄을
// 늘 보이며, 거부 까닭 `private_only`를 글로 보인다. 화면은 주소를 판정하지 않는다(서버가 한다).
import { afterEach, describe, expect, it, vi } from "vitest";
import { SEMANTIC_ERRORS, semanticCard } from "../../web/src/setup/extras";
import type { SemanticSettings } from "../../web/src/setup/extras";
import { SEMANTIC_LAN_LABEL } from "../../web/src/setup/semantic-lan";

const OUTSIDE = "임베딩을 이 컴퓨터 밖(사설망 192.168.0.2)으로 보냄";

interface Sent {
  path: string;
  body: unknown;
}

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** 카드를 띄우고, POST마다 `answer`로 답하는 가짜 fetch를 건다. */
function mount(initial: SemanticSettings, answer: (body: Record<string, unknown>) => Response): { card: HTMLElement; sent: Sent[] } {
  const sent: Sent[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const body = typeof init?.body === "string" ? (JSON.parse(init.body) as Record<string, unknown>) : {};
      sent.push({ path: new URL(String(input), "http://local.test").pathname, body });
      return answer(body);
    }),
  );
  const card = semanticCard(initial);
  document.body.append(card);
  return { card, sent };
}

function lanBox(card: HTMLElement): HTMLInputElement {
  const box = card.querySelector<HTMLInputElement>("#semantic-lan");
  expect(box, "#semantic-lan").not.toBeNull();
  return box as HTMLInputElement;
}

function toggle(card: HTMLElement): void {
  (card.querySelector('button[data-action="semantic-toggle"]') as HTMLButtonElement).click();
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

describe("뜻 검색 · 같은 집 다른 기기 Ollama", () => {
  it("켜지 않으면 몸통은 지금 그대로이고, 켜면 private_network를 싣는다", async () => {
    // 검증: D10 V23 V78
    const off: SemanticSettings = { enabled: false, base_url: "http://127.0.0.1:11434", model: "bge-m3" };
    const { card, sent } = mount(off, (body) => json(200, { ...body, private_network: body.private_network === true, outside: null }));
    expect(card.textContent).toContain(SEMANTIC_LAN_LABEL);
    expect(lanBox(card).checked).toBe(false);
    expect(card.querySelector('[data-role="semantic-outside"]')).toBeNull();
    toggle(card);
    await settle();
    expect(sent[0].body).toEqual({ enabled: true, base_url: "http://127.0.0.1:11434", model: "bge-m3" });

    lanBox(card).checked = true;
    (card.querySelector("#semantic-url") as HTMLInputElement).value = "http://192.168.0.2:11434";
    toggle(card);
    await settle();
    expect(sent[1].body).toEqual({ enabled: false, base_url: "http://192.168.0.2:11434", model: "bge-m3", private_network: true });
  });

  it("서버가 준 '이 컴퓨터 밖으로 보냄' 줄을 늘 보이고, 루프백으로 돌아오면 지운다", async () => {
    // 검증: D10 V23
    const lan: SemanticSettings = { enabled: true, base_url: "http://192.168.0.2:11434", model: "bge-m3", private_network: true, outside: OUTSIDE };
    const { card } = mount(lan, () => json(200, { enabled: false, base_url: "http://127.0.0.1:11434", model: "bge-m3", private_network: false, outside: null }));
    expect(lanBox(card).checked).toBe(true);
    expect(card.querySelector('[data-role="semantic-outside"]')?.textContent).toBe(OUTSIDE);
    toggle(card);
    await settle();
    expect(card.querySelector('[data-role="semantic-outside"]')).toBeNull();
    expect(lanBox(card).checked).toBe(false);
  });

  it("공인 주소 거부(private_only)는 글로 보인다", async () => {
    // 검증: D10 V23
    expect(SEMANTIC_ERRORS.private_only).toContain("사설망");
    const off: SemanticSettings = { enabled: false, base_url: "http://8.8.8.8:11434", model: "bge-m3" };
    const { card } = mount(off, () => json(400, { error: "private_only" }));
    lanBox(card).checked = true;
    toggle(card);
    await settle();
    expect(card.textContent).toContain(SEMANTIC_ERRORS.private_only);
  });
});

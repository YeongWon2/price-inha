// 폰 승인 카드 보강(server 50차 · C24 · R4-4 · V55): 승인 카드(`GET /api/approvals/{id}`)의 투표 수와 한도 적용
// 전 → 후가 카드에 보이고, 다른 승인으로 바뀌면 그 승인의 것으로 바뀐다(두 칸이 없으면 숨는다). 보내는 지문은 그대로다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { approvalPath, decidePath } from "../../web/src/paths";

const pages = import.meta.glob<string>("../../web/phone/index.html", { query: "?raw", import: "default", eager: true });
const ID = "3f9a0c1e5b7d2468ace0f1b3d5c7e9a1";
const FULL = "a1b2c3" + "d".repeat(58);
const ID2 = "7c1e5b7d2468ace0f1b3d5c7e9a13f9a";
const FULL2 = "e4f5a6" + "9".repeat(58);
const VOTES = { bull: 4, bear: 0, neutral: 1, abstain: 1 };
const CLAMPS = [{ limit: "symbol_weight", before: "12", after: "9", reason: "종목 비중 한도" }];

let worker: EventTarget;
let added: Array<[string, EventListenerOrEventListenerObject]> = [];
const posted: unknown[] = [];

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

async function fakeFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = String(input);
  const method = (init?.method ?? "GET").toUpperCase();
  if (method === "GET" && url === approvalPath(ID)) {
    return json(200, { number: ID, summary: "000660 · 금액 한도", fingerprint: FULL, status: "pending", votes: VOTES, clamps: CLAMPS });
  }
  if (method === "GET" && url === approvalPath(ID2)) {
    return json(200, { number: ID2, summary: "035420 · 금액 한도", fingerprint: FULL2, status: "pending" });
  }
  if (method === "POST" && url === decidePath(ID, "approve")) {
    posted.push(JSON.parse(String(init?.body)));
    return json(200, { result: "rejected", reason: "" });
  }
  return json(404, {});
}

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

const card = () => document.querySelector('[data-card="approval"]') as HTMLElement;
const extras = () => card().querySelector<HTMLElement>('[data-part="extras"]');

function open(id: string): void {
  worker.dispatchEvent(new MessageEvent("message", { data: { type: "open", link: `http://localhost/phone/approvals/${id}` } }));
}

async function mount(): Promise<void> {
  added = [];
  posted.length = 0;
  worker = Object.assign(new EventTarget(), { register: vi.fn(async () => ({ pushManager: {} })) });
  const html = Object.values(pages)[0];
  const parsed = new DOMParser().parseFromString(html, "text/html");
  document.body.replaceChildren(...[...parsed.body.childNodes].map((node) => document.importNode(node, true)));
  Object.defineProperty(navigator, "serviceWorker", { configurable: true, value: worker });
  vi.stubGlobal("Notification", { requestPermission: vi.fn(async () => "granted") });
  vi.stubGlobal("fetch", vi.fn(fakeFetch));
  const original = document.addEventListener.bind(document);
  vi.spyOn(document, "addEventListener").mockImplementation((type, listener, options) => {
    if (listener) added.push([type, listener]);
    original(type, listener, options);
  });
  vi.resetModules();
  await import("../../web/src/phone/main");
  vi.mocked(document.addEventListener).mockRestore();
}

afterEach(() => {
  for (const [type, listener] of added) document.removeEventListener(type, listener);
  vi.unstubAllGlobals();
});

describe("폰 승인 카드 보강", () => {
  it("카드에 투표 수와 한도 적용 전 → 후가 보이고, 보내는 지문은 카드가 받은 것 그대로다", async () => {
    // 검증: V55 C24 R4-4
    await mount();
    open(ID);
    await settle();
    expect(card().hidden).toBe(false);
    expect(extras(), "카드 안의 보강 자리").not.toBeNull();
    expect(extras()?.hidden).toBe(false);
    expect(extras()?.querySelector('[data-part="votes"]')?.textContent).toContain("강세 4 · 약세 0 · 중립 1 · 기권 1");
    expect(extras()?.querySelector('[data-part="clamp"]')?.textContent).toContain("12주 → 9주(종목 비중 한도)");
    expect(card().querySelector('[data-part="summary"]')?.textContent).toBe("000660 · 금액 한도");
    (document.querySelector('[data-action="approve"]') as HTMLButtonElement).click();
    await settle();
    expect(posted).toEqual([{ fingerprint: FULL }]);
  });

  it("두 칸이 없는 다른 승인으로 바뀌면 보강 자리가 비고 숨는다", async () => {
    // 검증: V55 C24
    await mount();
    open(ID);
    await settle();
    open(ID2);
    await settle();
    expect(card().querySelector('[data-part="summary"]')?.textContent).toBe("035420 · 금액 한도");
    expect(extras()?.hidden).toBe(true);
    expect(extras()?.childElementCount).toBe(0);
  });
});

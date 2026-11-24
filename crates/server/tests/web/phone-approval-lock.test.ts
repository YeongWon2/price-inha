// 폰 승인 카드 두 번 누름 막기(server 15차 2번): 보내는 동안 승인 · 거절 단추가 막히고 POST는 한 번,
// 실패하면 다시 켜지고, 성공하면 막힌 채 둔다. 시험마다 화면을 새로 세워 순서와 무관하다. (19차 5번) 보내는 동안
// 다른 승인이 열리면 앞 결정의 결과 글 · 단추 상태를 새 카드에 쓰지 않는다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { approvalPath, decidePath } from "../../web/src/paths";

const pages = import.meta.glob<string>("../../web/phone/index.html", { query: "?raw", import: "default", eager: true });
const ID = "3f9a0c1e5b7d2468ace0f1b3d5c7e9a1";
const FULL = "a1b2c3" + "d".repeat(58);
const ID2 = "7c1e5b7d2468ace0f1b3d5c7e9a13f9a";
const FULL2 = "e4f5a6" + "9".repeat(58);

interface Pending {
  id: string;
  resolve: (status: number) => void;
}

let pending: Pending[] = [];
let worker: EventTarget;
let added: Array<[string, EventListenerOrEventListenerObject]> = [];

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

const cards: Record<string, string> = { [ID]: FULL, [ID2]: FULL2 };

async function fakeFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = String(input);
  const method = (init?.method ?? "GET").toUpperCase();
  for (const [id, fingerprint] of Object.entries(cards)) {
    if (method === "GET" && url === approvalPath(id)) {
      return json(200, { number: id, summary: "삼성전자 10주 사기", fingerprint, status: "pending" });
    }
    if (method === "POST" && (url === decidePath(id, "approve") || url === decidePath(id, "reject"))) {
      const status = await new Promise<number>((resolve) => {
        pending.push({ id, resolve });
      });
      return json(status, { status: "approved" });
    }
  }
  return json(404, {});
}

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

const button = (verb: string) => document.querySelector(`[data-action="${verb}"]`) as HTMLButtonElement;
const card = () => document.querySelector('[data-card="approval"]') as HTMLElement;
const result = () => (card().querySelector('[data-part="result"]') as HTMLElement).textContent;

function open(id: string): void {
  worker.dispatchEvent(
    new MessageEvent("message", { data: { type: "open", link: `http://localhost/phone/approvals/${id}` } }),
  );
}

/** 폰 화면을 새로 세우고 승인 카드 `ID`를 연다. 시험마다 부른다. */
async function mount(): Promise<void> {
  pending = [];
  added = [];
  worker = Object.assign(new EventTarget(), {
    register: vi.fn(async () => ({ pushManager: {} })),
  });
  const html = Object.values(pages)[0];
  document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
  Object.defineProperty(navigator, "serviceWorker", { configurable: true, value: worker });
  vi.stubGlobal("Notification", { requestPermission: vi.fn(async () => "granted") });
  vi.stubGlobal("fetch", vi.fn(fakeFetch));
  // 이 묶음이 문서에 거는 클릭 듣기를 기억해 두었다가 시험이 끝나면 떼어, 앞 시험의 화면이 뒤 시험에 끼지 않게 한다.
  const original = document.addEventListener.bind(document);
  vi.spyOn(document, "addEventListener").mockImplementation((type, listener, options) => {
    if (listener) added.push([type, listener]);
    original(type, listener, options);
  });
  vi.resetModules();
  await import("../../web/src/phone/main");
  vi.mocked(document.addEventListener).mockRestore();
  open(ID);
  await settle();
}

afterEach(() => {
  for (const [type, listener] of added) document.removeEventListener(type, listener);
  for (const post of pending) post.resolve(500);
  pending = [];
});

describe("폰 승인 카드 두 번 누름", () => {
  it("보내는 동안 두 단추가 막히고 빨리 두 번 눌러도 POST는 한 번이다. 실패하면 다시 켜진다", async () => {
    // 검증: V10
    await mount();
    button("approve").click();
    button("approve").click();
    button("reject").click();
    await settle();
    expect(pending.length).toBe(1);
    expect(button("approve").disabled).toBe(true);
    expect(button("reject").disabled).toBe(true);

    pending[0].resolve(500);
    await settle();
    expect(button("approve").disabled).toBe(false);
    expect(button("reject").disabled).toBe(false);
  });

  it("다시 눌러 성공하면 단추는 막힌 채 두고 더는 보내지 않는다", async () => {
    // 검증: V10
    await mount();
    button("approve").click();
    await settle();
    expect(pending.length).toBe(1);
    pending[0].resolve(200);
    await settle();
    expect(button("approve").disabled).toBe(true);
    expect(button("reject").disabled).toBe(true);

    button("approve").click();
    button("reject").click();
    await settle();
    expect(pending.length).toBe(1);
  });
});

describe("폰 승인 카드: 보내는 동안 다른 승인이 열림 (19차 5번)", () => {
  it("보내는 동안 새 카드가 열리면 앞 결정이 성공해도 새 카드는 결과 글이 비고 단추가 켜져 있다", async () => {
    // 검증: V10
    await mount();
    button("approve").click();
    await settle();
    expect(pending.length).toBe(1);

    open(ID2);
    await settle();
    expect(card().dataset.approvalId).toBe(ID2);
    expect(result()).toBe("");
    expect(button("approve").disabled).toBe(false);
    expect(button("reject").disabled).toBe(false);

    pending[0].resolve(200);
    await settle();
    expect(card().dataset.approvalId).toBe(ID2);
    expect(result(), "앞 결정의 결과 글이 새 카드에 남지 않는다").toBe("");
    expect(button("approve").disabled, "새 카드의 단추는 켜져 있다").toBe(false);
    expect(button("reject").disabled).toBe(false);
  });

  it("보내는 동안 새 카드가 열리면 앞 결정이 실패해도 새 카드에 실패 글을 쓰지 않는다", async () => {
    // 검증: V10
    await mount();
    button("reject").click();
    await settle();
    open(ID2);
    await settle();

    pending[0].resolve(500);
    await settle();
    expect(card().dataset.approvalId).toBe(ID2);
    expect(result(), "앞 결정의 실패 글이 새 카드에 남지 않는다").toBe("");
    expect(button("approve").disabled).toBe(false);
    expect(button("reject").disabled).toBe(false);
  });
});

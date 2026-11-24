// 사무실 승인 카드 두 번 누름 막기(server 15차 2번): 보내는 동안 그 카드의 승인 · 거절 단추가 막히고 POST는 한 번,
// 실패하면 다시 켜지고, 성공하면 막힌 채 둔다.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { applyApproval, type FetchLike } from "../../web/src/office/approval";

const FULL = "a1b2c3" + "d".repeat(58);
const N = "3f9a0c1e5b7d2468ace0f1b3d5c7e9a1";

let posts = 0;
let release: ((status: number) => void) | null = null;

const fetcher = vi.fn<FetchLike>(async (url, init) => {
  const method = (init?.method ?? "GET").toUpperCase();
  if (method === "GET") {
    return new Response(JSON.stringify({ number: N, fingerprint: FULL, status: "pending" }), { status: 200 });
  }
  posts += 1;
  const status = await new Promise<number>((resolve) => {
    release = resolve;
  });
  return new Response(JSON.stringify({ status: "approved", url }), { status });
});

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

let root: HTMLElement;
const verb = (v: string) => root.querySelector(`button[data-verb="${v}"]`) as HTMLButtonElement;

beforeEach(() => {
  document.body.innerHTML = "";
  root = document.createElement("section");
  document.body.appendChild(root);
  posts = 0;
  release = null;
  applyApproval(root, { number: N, summary: "삼성전자 10주 사기", short_code: "a1b2c3", status: "pending" }, fetcher);
});

describe("사무실 승인 카드 두 번 누름", () => {
  it("보내는 동안 두 단추가 막히고 빨리 두 번 눌러도 POST는 한 번이다. 실패하면 다시 켜진다", async () => {
    // 검증: V10
    verb("approve").click();
    verb("approve").click();
    verb("reject").click();
    await settle();
    expect(posts).toBe(1);
    expect(verb("approve").disabled).toBe(true);
    expect(verb("reject").disabled).toBe(true);

    release?.(500);
    await settle();
    expect(verb("approve").disabled).toBe(false);
    expect(verb("reject").disabled).toBe(false);

    verb("reject").click();
    await settle();
    expect(posts).toBe(2);
  });

  it("성공하면 단추는 막힌 채 두고 더는 보내지 않는다", async () => {
    // 검증: V10
    verb("approve").click();
    await settle();
    release?.(200);
    await settle();
    expect(verb("approve").disabled).toBe(true);
    expect(verb("reject").disabled).toBe(true);

    verb("approve").click();
    verb("reject").click();
    await settle();
    expect(posts).toBe(1);
  });
});

// 19차 6번(시험만): 지문이 카드와 다르면 보내지 않고 단추가 다시 켜진다 · 두 카드는 서로 막지 않는다.
describe("사무실 승인 카드 막기 보강", () => {
  const N2 = "7c1e5b7d2468ace0f1b3d5c7e9a13f9a";
  const OTHER = "ffffff" + "0".repeat(58);

  it("지문이 카드와 달라 보내지 않았으면 단추가 다시 켜진다", async () => {
    // 검증: V10
    const sent: string[] = [];
    const mismatching = vi.fn<FetchLike>(async (_url, init) => {
      if ((init?.method ?? "GET").toUpperCase() === "POST") {
        sent.push("post");
        return new Response("{}", { status: 200 });
      }
      return new Response(JSON.stringify({ number: N, fingerprint: OTHER, status: "pending" }), { status: 200 });
    });
    document.body.innerHTML = "";
    root = document.createElement("section");
    document.body.appendChild(root);
    applyApproval(root, { number: N, summary: "삼성전자 10주 사기", short_code: "a1b2c3", status: "pending" }, mismatching);

    verb("approve").click();
    await settle();
    expect(sent).toEqual([]);
    expect(verb("approve").disabled).toBe(false);
    expect(verb("reject").disabled).toBe(false);
    expect((root.querySelector('[data-part="result"]') as HTMLElement).textContent).toContain("지문이 카드와 달라요");
  });

  it("한 카드가 보내는 동안 다른 카드의 단추는 막히지 않고 따로 보낼 수 있다", async () => {
    // 검증: V10
    const pendingPosts: Array<{ url: string; resolve: (status: number) => void }> = [];
    const two = vi.fn<FetchLike>(async (url, init) => {
      if ((init?.method ?? "GET").toUpperCase() === "GET") {
        const number = url.includes(N2) ? N2 : N;
        return new Response(JSON.stringify({ number, fingerprint: FULL, status: "pending" }), { status: 200 });
      }
      const status = await new Promise<number>((resolve) => {
        pendingPosts.push({ url, resolve });
      });
      return new Response(JSON.stringify({ status: "approved" }), { status });
    });
    document.body.innerHTML = "";
    root = document.createElement("section");
    document.body.appendChild(root);
    applyApproval(root, { number: N, summary: "첫째", short_code: "a1b2c3", status: "pending" }, two);
    applyApproval(root, { number: N2, summary: "둘째", short_code: "a1b2c3", status: "pending" }, two);
    const button = (number: string, v: string) =>
      root.querySelector(`[data-approval="${number}"] button[data-verb="${v}"]`) as HTMLButtonElement;

    button(N, "approve").click();
    await settle();
    expect(pendingPosts.length).toBe(1);
    expect(button(N, "approve").disabled).toBe(true);
    expect(button(N2, "approve").disabled, "다른 카드는 막히지 않는다").toBe(false);
    expect(button(N2, "reject").disabled).toBe(false);

    button(N2, "reject").click();
    await settle();
    expect(pendingPosts.length, "다른 카드도 따로 보낼 수 있다").toBe(2);
    expect(button(N, "reject").disabled, "한 카드의 결정이 다른 카드에 번지지 않는다").toBe(true);
    for (const post of pendingPosts) post.resolve(200);
    await settle();
  });
});

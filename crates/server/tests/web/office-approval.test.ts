// 사무실 승인 카드(server 11차 4번 · 10c 14번): office.approval 이벤트로 그리기 · pending이 아니면 닫기 ·
// 요약은 글로만 · 승인/거절은 폰과 같은 경로와 같은 지문 대조.
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  KIND_OFFICE_APPROVAL,
  applyApproval,
  decideApproval,
  type FetchLike,
} from "../../web/src/office/approval";
import { decidePath } from "../../web/src/paths";

const FULL = "a1b2c3" + "d".repeat(58);
// 승인 번호는 서버의 `ApprovalNumber`(무작위 16바이트 hex 32자) 글자다. 짧은 글자도 같은 번호로 다룬다.
const N = "3f9a0c1e5b7d2468ace0f1b3d5c7e9a1";
const M = "b7";

function event(over: Partial<Record<string, unknown>> = {}): Record<string, unknown> {
  return { number: N, summary: "삼성전자 10주 사기", short_code: "a1b2c3", status: "pending", ...over };
}

/** 부른 것을 적고, 경로별로 답하는 가짜 fetch. */
function fakeFetch(fingerprint = FULL, decideStatus = 200, id = N) {
  const calls: { url: string; method: string; body: unknown; init?: RequestInit }[] = [];
  const fetcher = vi.fn<FetchLike>(async (url, init) => {
    const method = (init?.method ?? "GET").toUpperCase();
    const body = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    calls.push({ url, method, body, init });
    if (method === "GET" && url === `/api/approvals/${encodeURIComponent(id)}`) {
      return new Response(
        JSON.stringify({ number: id, summary: "삼성전자 10주 사기", fingerprint, short_code: fingerprint.slice(0, 6), status: "pending" }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      );
    }
    if (method === "POST") {
      return new Response(JSON.stringify({ status: decideStatus === 200 ? "approved" : "x" }), { status: decideStatus });
    }
    return new Response("{}", { status: 404 });
  });
  return { calls, fetcher };
}

let root: HTMLElement;
beforeEach(() => {
  document.body.innerHTML = "";
  root = document.createElement("section");
  root.id = "approvals";
  document.body.appendChild(root);
});

const card = (n: string) =>
  [...root.querySelectorAll<HTMLElement>("[data-approval]")].find((c) => c.dataset.approval === n) ?? null;

describe("사무실 승인 카드", () => {
  it("이벤트 종류 이름은 서버와 같다", () => {
    // 검증: V10 C7
    expect(KIND_OFFICE_APPROVAL).toBe("office.approval");
  });

  it("pending 이벤트로 카드를 그린다(요약 · 지문 앞 6자 · 승인/거절 단추)", () => {
    // 검증: V10 C7 D11
    applyApproval(root, event(), fakeFetch().fetcher);
    const c = card(N);
    expect(c).not.toBeNull();
    expect(c!.querySelector('[data-part="summary"]')!.textContent).toBe("삼성전자 10주 사기");
    expect(c!.querySelector('[data-part="code"]')!.textContent).toBe("a1b2c3");
    expect(c!.querySelector('button[data-verb="approve"]')).not.toBeNull();
    expect(c!.querySelector('button[data-verb="reject"]')).not.toBeNull();
    expect(c!.textContent).not.toContain(FULL);
  });

  it("같은 번호의 이벤트는 카드 한 장을 고친다", () => {
    // 검증: V10 C7
    const { fetcher } = fakeFetch();
    applyApproval(root, event(), fetcher);
    applyApproval(root, event({ summary: "고친 요약" }), fetcher);
    applyApproval(root, event({ number: M, short_code: "ffffff" }), fetcher);
    expect([...root.querySelectorAll<HTMLElement>("[data-approval]")].filter((c) => c.dataset.approval === N).length).toBe(1);
    expect(card(N)!.querySelector('[data-part="summary"]')!.textContent).toBe("고친 요약");
    expect(card(M)).not.toBeNull();
  });

  it.each(["approved", "rejected", "expired", "blocked"])("상태가 %s가 되면 카드를 닫는다", (status) => {
    // 검증: V10 C7
    const { fetcher } = fakeFetch();
    applyApproval(root, event(), fetcher);
    applyApproval(root, event({ number: M }), fetcher);
    applyApproval(root, event({ status }), fetcher);
    expect(card(N)).toBeNull();
    expect(card(M)).not.toBeNull();
  });

  it("없는 번호가 닫히거나 모양이 틀린 몸통은 아무것도 하지 않는다", () => {
    // 검증: V10 E3
    const { fetcher } = fakeFetch();
    applyApproval(root, event({ status: "approved" }), fetcher);
    for (const bad of [
      null,
      "x",
      3,
      {},
      event({ number: "" }),
      event({ number: 7 }),
      event({ number: undefined }),
      event({ number: null }),
      event({ summary: 1 }),
      event({ status: undefined }),
    ]) {
      expect(() => applyApproval(root, bad, fetcher)).not.toThrow();
    }
    expect(root.querySelectorAll("[data-approval]").length).toBe(0);
  });

  it("요약에 <script>가 들어 있어도 HTML로 해석하지 않고 글 그대로 보인다", () => {
    // 검증: V10 V46 D11
    const nasty = '<script>window.__pwned = 1</script><img src=x onerror="window.__pwned = 2"><b>굵게</b>';
    applyApproval(root, event({ summary: nasty }), fakeFetch().fetcher);
    const summary = card(N)!.querySelector('[data-part="summary"]')!;
    expect(summary.textContent).toBe(nasty);
    expect(summary.children.length).toBe(0);
    expect(root.querySelector("script, img, b")).toBeNull();
    expect((window as unknown as { __pwned?: number }).__pwned).toBeUndefined();
  });

  it("승인은 폰과 같은 경로로 전체 지문을 보낸다(카드의 앞 6자와 맞을 때)", async () => {
    // 검증: V10 C7 D11
    const { calls, fetcher } = fakeFetch();
    applyApproval(root, event(), fetcher);
    await expect(decideApproval(root, N, "approve", fetcher)).resolves.toBe("sent");
    expect(calls.map((c) => `${c.method} ${c.url}`)).toEqual([
      `GET /api/approvals/${N}`,
      `POST /api/approvals/${N}/approve`,
    ]);
    expect(calls[1].url).toBe(decidePath(N, "approve"));
    expect(calls[1].body).toEqual({ fingerprint: FULL });
    expect(new Headers(calls[1].init?.headers).get("Content-Type")).toBe("application/json");
  });

  it("거절도 같은 길로 보낸다", async () => {
    // 검증: V10 C7
    const { calls, fetcher } = fakeFetch();
    applyApproval(root, event(), fetcher);
    await expect(decideApproval(root, N, "reject", fetcher)).resolves.toBe("sent");
    expect(calls.at(-1)!.url).toBe(`/api/approvals/${N}/reject`);
    expect(calls.at(-1)!.body).toEqual({ fingerprint: FULL });
  });

  it("받은 지문의 앞 6자가 카드와 다르면 보내지 않는다", async () => {
    // 검증: V10 C7 E7
    const { calls, fetcher } = fakeFetch("ffffff" + "0".repeat(58));
    applyApproval(root, event(), fetcher);
    await expect(decideApproval(root, N, "approve", fetcher)).resolves.toBe("mismatch");
    expect(calls.filter((c) => c.method === "POST")).toEqual([]);
  });

  it("서버가 거절하면(409) 실패로 알리고 던지지 않는다", async () => {
    // 검증: V10 E7
    const { fetcher } = fakeFetch(FULL, 409);
    applyApproval(root, event(), fetcher);
    await expect(decideApproval(root, N, "approve", fetcher)).resolves.toBe("failed");
    expect(card(N)!.querySelector('[data-part="result"]')!.textContent).not.toBe("");
  });

  it("번호 글자는 주소 글자로 감싸서 부른다", async () => {
    // 검증: V10 C7
    const odd = "a b/c";
    const { calls, fetcher } = fakeFetch(FULL, 200, odd);
    applyApproval(root, event({ number: odd }), fetcher);
    expect(card(odd)).not.toBeNull();
    await expect(decideApproval(root, odd, "approve", fetcher)).resolves.toBe("sent");
    expect(calls.map((c) => `${c.method} ${c.url}`)).toEqual([
      "GET /api/approvals/a%20b%2Fc",
      "POST /api/approvals/a%20b%2Fc/approve",
    ]);
  });

  it("카드의 단추를 누르면 그 답을 보낸다", async () => {
    // 검증: V10 C7
    const { calls, fetcher } = fakeFetch();
    applyApproval(root, event(), fetcher);
    card(N)!.querySelector<HTMLButtonElement>('button[data-verb="reject"]')!.click();
    await vi.waitFor(() => expect(calls.some((c) => c.method === "POST")).toBe(true));
    expect(calls.find((c) => c.method === "POST")!.url).toBe(`/api/approvals/${N}/reject`);
  });
});

// 사무실 승인 카드(server 35차 6번): 승인 · 거절 답이 서버에서 실패(5xx)하거나 닿지 않으면 기다리는 승인 목록을
// 다시 읽는 일(`resync`)을 한 번 부른다(✋를 서버와 맞춘다). 보냄 · 지문 다름에는 부르지 않는다. 카드의 단추도 같다.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { applyApproval, decideApproval, type FetchLike } from "../../web/src/office/approval";

const FULL = "a1b2c3" + "d".repeat(58);
const N = "3f9a0c1e5b7d2468ace0f1b3d5c7e9a1";

function event(): Record<string, unknown> {
  return { number: N, summary: "삼성전자 10주 사기", short_code: "a1b2c3", status: "pending" };
}

/** 승인 내용 읽기(`readStatus`)와 답 보내기(`decideStatus`)를 그 상태로 답하는 가짜 fetch. `offline`이면 던진다. */
function fakeFetch(readStatus: number, decideStatus: number, fingerprint = FULL, offline = false) {
  const posts: string[] = [];
  const fetcher = vi.fn<FetchLike>(async (url, init) => {
    if (offline) throw new TypeError("network down");
    const method = (init?.method ?? "GET").toUpperCase();
    if (method === "POST") {
      posts.push(url);
      return new Response(JSON.stringify({ error: "x" }), { status: decideStatus });
    }
    const body = { number: N, summary: "삼성전자 10주 사기", fingerprint, short_code: fingerprint.slice(0, 6), status: "pending" };
    return new Response(JSON.stringify(body), { status: readStatus, headers: { "Content-Type": "application/json" } });
  });
  return { fetcher, posts };
}

let root: HTMLElement;
beforeEach(() => {
  document.body.textContent = "";
  root = document.createElement("section");
  root.id = "approvals";
  document.body.append(root);
});

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("사무실 승인 카드 · 답 실패 뒤 다시 읽기", () => {
  for (const status of [500, 502, 503]) {
    it(`답 보내기가 ${status}이면 실패이고 대기 목록 다시 읽기를 한 번 부른다`, async () => {
      // 검증: V62 E50 C27 R5-10
      const { fetcher, posts } = fakeFetch(200, status);
      applyApproval(root, event(), fetcher);
      const resync = vi.fn();
      const outcome = await decideApproval(root, N, "approve", fetcher, resync);
      expect(outcome).toBe("failed");
      expect(posts.length).toBe(1);
      expect(resync).toHaveBeenCalledTimes(1);
      const approve = root.querySelector<HTMLButtonElement>('button[data-verb="approve"]');
      expect(approve?.disabled, "실패하면 단추를 다시 켠다").toBe(false);
    });
  }

  it("승인 내용 읽기가 5xx여도 실패이고 다시 읽기를 부른다(답은 보내지 않는다)", async () => {
    // 검증: V62 E50 C27
    const { fetcher, posts } = fakeFetch(503, 200);
    applyApproval(root, event(), fetcher);
    const resync = vi.fn();
    expect(await decideApproval(root, N, "reject", fetcher, resync)).toBe("failed");
    expect(posts).toEqual([]);
    expect(resync).toHaveBeenCalledTimes(1);
  });

  it("서버에 닿지 않아도 실패이고 다시 읽기를 부른다", async () => {
    // 검증: V62 E50
    const { fetcher } = fakeFetch(200, 200, FULL, true);
    applyApproval(root, event(), fetcher);
    const resync = vi.fn();
    expect(await decideApproval(root, N, "approve", fetcher, resync)).toBe("failed");
    expect(resync).toHaveBeenCalledTimes(1);
  });

  it("보냈거나 지문이 달라 보내지 않았으면 다시 읽기를 부르지 않는다", async () => {
    // 검증: V62 C27
    const sent = fakeFetch(200, 200);
    applyApproval(root, event(), sent.fetcher);
    const resync = vi.fn();
    expect(await decideApproval(root, N, "approve", sent.fetcher, resync)).toBe("sent");
    expect(resync).not.toHaveBeenCalled();

    root.textContent = "";
    const other = fakeFetch(200, 200, "ffffff" + "e".repeat(58));
    applyApproval(root, event(), other.fetcher);
    expect(await decideApproval(root, N, "approve", other.fetcher, resync)).toBe("mismatch");
    expect(resync).not.toHaveBeenCalled();
  });

  it("다시 읽기가 던져도 결과와 단추는 그대로다", async () => {
    // 검증: V62 E50
    const { fetcher } = fakeFetch(200, 503);
    applyApproval(root, event(), fetcher);
    const resync = vi.fn(() => {
      throw new Error("resync failed");
    });
    expect(await decideApproval(root, N, "approve", fetcher, resync)).toBe("failed");
    expect(resync).toHaveBeenCalledTimes(1);
    expect(root.querySelector<HTMLButtonElement>('button[data-verb="approve"]')?.disabled).toBe(false);
  });

  it("카드의 단추를 눌러 실패해도 카드를 그릴 때 준 다시 읽기를 부른다", async () => {
    // 검증: V62 E50 C27 R5-10
    const { fetcher } = fakeFetch(200, 500);
    const resync = vi.fn();
    applyApproval(root, event(), fetcher, resync);
    root.querySelector<HTMLButtonElement>('button[data-verb="approve"]')?.click();
    await settle();
    expect(resync).toHaveBeenCalledTimes(1);
  });

  it("다시 읽기를 주지 않으면 지금처럼 실패만 알린다", async () => {
    // 검증: V62
    const { fetcher } = fakeFetch(200, 503);
    applyApproval(root, event(), fetcher);
    expect(await decideApproval(root, N, "approve", fetcher)).toBe("failed");
    expect(root.querySelector('[data-part="result"]')?.textContent).toContain("503");
  });
});

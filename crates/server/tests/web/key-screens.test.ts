// 키 화면(`/keys`) · 설정 화면(`/`)의 키 칸 비우기(server 15차 4번): 한 규칙(`submitKeyFields`)을 쓴다.
// 받아들여질 때만 칸을 비우고, 거절되면 칸을 그대로 둔다. 따로 칸을 비우는 코드를 남기지 않는다.
import { beforeAll, describe, expect, it, vi } from "vitest";

const pages = import.meta.glob<string>("../../web/keys.html", { query: "?raw", import: "default", eager: true });
const screens = import.meta.glob<string>(["../../web/src/keys/main.ts", "../../web/src/setup/main.ts"], {
  query: "?raw",
  import: "default",
  eager: true,
});

let accept = false;
let posts = 0;

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

beforeAll(async () => {
  const html = Object.values(pages)[0];
  document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      if ((init?.method ?? "GET").toUpperCase() !== "POST") return new Response("{}", { status: 404 });
      posts += 1;
      return accept
        ? new Response(JSON.stringify({ registered: true }), { status: 200 })
        : new Response(JSON.stringify({ registered: false, error: "key_rejected" }), { status: 400 });
    }),
  );
  await import("../../web/src/keys/main");
  await settle();
});

describe("키 칸 비우기", () => {
  it("키 화면: 거절되면 칸이 그대로, 받아들여지면 칸이 빈다", async () => {
    // 검증: V46 C3 E4
    const input = document.getElementById("k-api-key") as HTMLInputElement;
    const button = document.getElementById("k-ai") as HTMLButtonElement;

    input.value = "typed-value";
    accept = false;
    button.click();
    await settle();
    expect(posts).toBe(1);
    expect(input.value).toBe("typed-value");

    accept = true;
    button.click();
    await settle();
    expect(posts).toBe(2);
    expect(input.value).toBe("");
  });

  it("키 화면 · 설정 화면은 submitKeyFields를 쓰고 따로 칸을 비우지 않는다", () => {
    // 검증: V46 C3
    const entries = Object.entries(screens);
    expect(entries.length).toBe(2);
    for (const [name, text] of entries) {
      expect(text.includes("submitKeyFields("), `${name}: submitKeyFields를 부른다`).toBe(true);
      expect(text.includes("clearKeyFields"), `${name}: clearKeyFields를 따로 쓰지 않는다`).toBe(false);
    }
  });
});

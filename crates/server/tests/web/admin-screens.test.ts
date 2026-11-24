// 관리 화면 틀 넷(server 26차 4번 · 6번 · F12 · R18-3 · N21 · V77): 계좌 · 템플릿 · 규칙 · 히스토리 화면은 이 차수에서
// 메뉴 · 제목 · "준비 중" 빈 상태만 그린다. 관리 화면 소스는 3D(three · scene3d)를 부르지 않고, 새 npm 의존성이 없다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { API } from "../../web/src/paths";
import { SOURCES, mainOf, reach } from "./source-graph";

const PAGES = import.meta.glob<string>(
  ["../../web/account/index.html", "../../web/templates/index.html", "../../web/rules/index.html", "../../web/history/index.html"],
  { query: "?raw", import: "default", eager: true },
);
const MAINS = import.meta.glob([
  "../../web/src/account/main.ts",
  "../../web/src/templates/main.ts",
  "../../web/src/rules/main.ts",
  "../../web/src/history/main.ts",
]);
const PACKAGE = import.meta.glob<string>("../../web/package.json", { query: "?raw", import: "default", eager: true });

const ADMIN = [
  ["account", "계좌", "/account/"],
  ["templates", "템플릿", "/templates/"],
  ["rules", "규칙", "/rules/"],
  ["history", "히스토리", "/history/"],
] as const;

const LABELS = ["사무실", "계좌", "템플릿", "규칙", "히스토리", "설정"];

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

// 27차 3번 · 28차 2번 · 29차 2번: 템플릿 · 규칙 · 히스토리 화면에는 내용이 생긴다(template-screen.test.ts ·
// rules-screen.test.ts · history-screen.test.ts) — "준비 중" 틀 단언은 나머지 하나(계좌)에만.
const FRAMES = ADMIN.filter(([screen]) => screen !== "templates" && screen !== "rules" && screen !== "history");

describe("관리 화면 틀", () => {
  for (const [screen, label, href] of FRAMES) {
    it(`${label} 화면은 메뉴 · 제목 · "준비 중" 빈 상태를 그린다`, async () => {
      // 검증: V75 F12
      const html = PAGES[`../../web/${screen}/index.html`];
      expect(html, `${screen}/index.html이 있다`).toBeTypeOf("string");
      vi.resetModules();
      document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
      const calls: string[] = [];
      vi.stubGlobal(
        "fetch",
        vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
          calls.push(String(url));
          const method = (init?.method ?? "GET").toUpperCase();
          if (method === "POST" && String(url) === API.session) return new Response("{}", { status: 200 });
          return new Response("{}", { status: 404 });
        }),
      );
      await MAINS[`../../web/src/${screen}/main.ts`]();
      await settle();

      const nav = document.querySelector("#menu nav");
      expect(nav, `${screen}: 메뉴`).not.toBeNull();
      expect([...(nav as HTMLElement).querySelectorAll("a")].map((a) => a.textContent?.trim())).toEqual(LABELS);
      const current = [...(nav as HTMLElement).querySelectorAll('[aria-current="page"]')];
      expect(current.map((a) => a.textContent?.trim())).toEqual([label]);
      expect(current[0].getAttribute("href")).toBe(href);

      const titles = [...document.querySelectorAll("h1")];
      expect(titles.length, `${screen}: 제목 하나`).toBe(1);
      expect(titles[0].textContent).toContain(label);

      const empty = [...document.querySelectorAll('[role="status"]')].filter((node) => node.textContent?.includes("준비 중"));
      expect(empty.length, `${screen}: "준비 중" 빈 상태`).toBe(1);
      expect((nav as HTMLElement).contains(empty[0])).toBe(false);

      // 이 차수의 틀은 내용을 읽지 않는다: 부르는 서버 경로는 세션뿐이다.
      for (const url of calls) expect(url, `${screen}: ${url}`).toBe(API.session);
    });
  }
});

describe("관리 화면은 가볍다", () => {
  it("관리 화면 main은 three · 사무실 3D(scene3d) · 사무실 소스를 부르지 않는다(소스 글자)", () => {
    // 검증: V77 N21
    for (const [screen] of ADMIN) {
      expect(mainOf(screen) in SOURCES, `${screen}: main.ts가 있다`).toBe(true);
      const { files, bare } = reach(mainOf(screen));
      expect([...bare].filter((name) => name === "three" || name.startsWith("three/")), screen).toEqual([]);
      expect([...files].filter((file) => file.includes("scene3d") || file.includes("/src/office/")), screen).toEqual([]);
      // 메뉴 부품을 거쳐야 이 검사가 뜻을 가진다.
      expect(files.has("../../web/src/ui/menu.ts"), `${screen}: 메뉴 부품에 닿는다`).toBe(true);
    }
  });

  it("사무실 main은 3D를 부른다(위 검사가 뜻을 가지는지)", () => {
    // 검증: V77
    const { files, bare } = reach(mainOf("office"));
    expect([...files].some((file) => file.includes("scene3d")) || bare.has("three")).toBe(true);
  });

  it("새 npm 의존성을 더하지 않는다", () => {
    // 검증: N21
    const pkg = JSON.parse(Object.values(PACKAGE)[0]) as {
      dependencies?: Record<string, string>;
      devDependencies?: Record<string, string>;
    };
    expect(Object.keys(pkg.dependencies ?? {}).sort()).toEqual(["three"]);
    expect(Object.keys(pkg.devDependencies ?? {}).sort()).toEqual(["jsdom", "typescript", "vite", "vitest"]);
  });
});

// 화면이 부르는 경로 목록(server 11차 2번 · 5번): 지금 화면과 같은 경로, 같은 출처만, 바깥 주소 없음.
import { describe, expect, it } from "vitest";
import { SCREEN_PATHS, SCREEN_ROOTS, approvalPath, decidePath } from "../../web/src/paths";

const sources = import.meta.glob<string>("../../web/src/**/*.ts", { query: "?raw", import: "default", eager: true });
const entries = import.meta.glob<string>(
  [
    "../../web/index.html",
    "../../web/keys.html",
    "../../web/office/index.html",
    "../../web/phone/index.html",
    "../../web/account/index.html",
    "../../web/templates/index.html",
    "../../web/rules/index.html",
    "../../web/history/index.html",
  ],
  { query: "?raw", import: "default", eager: true },
);

const APPROVALS = ["/api/approvals/{id}", "/api/approvals/{id}/approve", "/api/approvals/{id}/reject"];

describe("화면이 부르는 경로", () => {
  it("각 화면은 지금과 같은 경로를 부른다(사무실은 승인 카드 · 담당 외형 경로가 더해진다)", () => {
    // 검증: D11 D2 C2
    const sorted = (list: readonly string[]) => [...list].sort();
    expect(sorted(SCREEN_PATHS.setup)).toEqual(
      sorted([
        "/api/session",
        "/ws",
        "/api/setup",
        "/api/setup/keys",
        "/api/setup/symbols",
        "/api/setup/ai",
        "/api/setup/backup",
        "/api/setup/slots",
        "/api/setup/providers",
        "/api/access/key",
        "/api/access/unlock",
        "/api/access/log",
      ]),
    );
    expect(sorted(SCREEN_PATHS.keys)).toEqual(sorted(["/api/remote/keys", "/api/setup/slots", "/api/setup/providers"]));
    expect(sorted(SCREEN_PATHS.office)).toEqual(
      sorted([
        "/api/session",
        "/api/office",
        "/api/office/chat",
        "/api/office/looks",
        "/api/office/look",
        "/ws",
        ...APPROVALS,
      ]),
    );
    expect(sorted(SCREEN_PATHS.phone)).toEqual(
      sorted(["/api/push/key", "/api/push/subscribe", "/api/notices", "/api/daily-summary", ...APPROVALS]),
    );
    // 26차 4번: 관리 화면 넷(틀만)은 세션 경로만 부른다.
    for (const screen of ["account", "templates", "rules", "history"] as const) {
      expect(sorted(SCREEN_PATHS[screen]), screen).toEqual(["/api/session"]);
    }
    expect(Object.keys(SCREEN_PATHS).sort()).toEqual(
      ["setup", "keys", "office", "phone", "account", "templates", "rules", "history"].sort(),
    );
  });

  it("화면 주소 표는 여덟 화면의 주소다", () => {
    // 검증: D11 V75
    expect(SCREEN_ROOTS).toEqual({
      setup: "/",
      keys: "/keys",
      office: "/office/",
      phone: "/phone/",
      account: "/account/",
      templates: "/templates/",
      rules: "/rules/",
      history: "/history/",
    });
  });

  it("모든 경로는 같은 출처의 절대 경로다(바깥 주소 없음)", () => {
    // 검증: D11 D2
    for (const [screen, list] of Object.entries(SCREEN_PATHS)) {
      for (const path of list) {
        expect(path.startsWith("/") && !path.startsWith("//"), `${screen}: ${path}`).toBe(true);
        expect(path.includes("://"), `${screen}: ${path}`).toBe(false);
      }
    }
  });

  it("승인 경로는 폰과 같다(번호는 주소 글자로 감싼다)", () => {
    // 검증: V10 C7
    expect(approvalPath(7)).toBe("/api/approvals/7");
    expect(decidePath(7, "approve")).toBe("/api/approvals/7/approve");
    expect(decidePath("a/b", "reject")).toBe("/api/approvals/a%2Fb/reject");
  });

  it("여덟 쪽의 HTML이 있고 화면 소스 어디에도 바깥 주소가 없다", () => {
    // 검증: D11 D1
    expect(Object.keys(entries).length).toBe(8);
    const all = { ...sources, ...entries };
    for (const [name, text] of Object.entries(all)) {
      for (const outside of ["http://", "https://", 'src="//', 'href="//', "url(//"]) {
        expect(text.includes(outside), `${name}: ${outside}`).toBe(false);
      }
    }
  });

  it("화면 소스에 적힌 서버 경로는 모두 표에 있다", () => {
    // 검증: D11
    const known = Object.values(SCREEN_PATHS).flat();
    const covered = (literal: string) =>
      known.some((path) => path === literal || (path.includes("{id}") && path.startsWith(literal)));
    for (const [name, text] of Object.entries(sources)) {
      for (const match of text.matchAll(/["'`](\/(?:api\/[^"'`?$]*|ws))["'`?$]/g)) {
        expect(covered(match[1]), `${name}: ${match[1]}`).toBe(true);
      }
    }
  });
});

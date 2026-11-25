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
        // 31차 1번 (다) · (라): 설정 화면은 뜻 검색 설정 · 진단 경로를 더 부른다.
        "/api/setup/semantic",
        "/api/setup/doctor",
        // 31차 고침(결정 22): 알림음 설정을 서버에서 읽고 쓴다.
        "/api/setup/sound",
        // 39차 2 · 7번: 메신저 상태 · 토큰 등록 · 지우기 · 페어링 승인 · 빼기.
        "/api/messenger",
        "/api/messenger/telegram",
        "/api/messenger/pairing/approve",
        "/api/messenger/pairing/revoke",
        // 40차 4 · 6번: 알림 전달처 · 조용한 시간 읽기 · 바꾸기.
        "/api/notify/settings",
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
        // 32차 1번: 내 캐릭터 읽기 · 바꾸기 · 자리 남기기.
        "/api/office/me",
        "/api/office/me/position",
        // 33차 3번 · 4번: 전체 공지 · 지난 공지 · 담당 부르기 · 도착 알림 · 보는 방 알림.
        "/api/office/notice",
        "/api/office/notices",
        "/api/office/summon",
        "/api/office/summon/arrived",
        "/api/office/summon/view",
        "/ws",
        ...APPROVALS,
        // 50차: 근거 패널 — 매매 목록(종목의 최근 매매 찾기) · 근거 카드 읽기.
        "/api/trades",
        "/api/trades/{id}/basis",
      ]),
    );
    expect(sorted(SCREEN_PATHS.phone)).toEqual(
      sorted(["/api/push/key", "/api/push/subscribe", "/api/notices", "/api/daily-summary", ...APPROVALS]),
    );
    // 30차 2번 · 3번: 계좌 화면은 계좌 읽기 · 팀 상태 · 다시 시작 · 실시간 연결 경로를 부른다. 40차 1 · 2번: 전체 정지.
    expect(sorted(SCREEN_PATHS.account)).toEqual(
      sorted(["/api/session", "/api/account", "/api/team", "/api/team/restart", "/api/team/stop", "/ws"]),
    );
    // 29차 2번: 히스토리 화면은 매매 목록 · 매매 하나(근거가 함께 온다) · 하루 정리 · 승인 요청 기록 경로를 부른다.
    expect(sorted(SCREEN_PATHS.history)).toEqual(
      sorted(["/api/session", "/api/trades", "/api/trades/{id}", "/api/daily-summary", "/api/approvals"]),
    );
    // 28차 2번: 규칙 화면은 규칙 읽기 · 개선안 목록 · 개선안 답하기 경로를 부른다.
    // 35차 2번 · 3번: 금지 종목 · 업종표 읽기 · 고치기 경로를 더 부른다.
    expect(sorted(SCREEN_PATHS.rules)).toEqual(
      sorted(["/api/session", "/api/rules", "/api/proposals", "/api/proposals/answer", "/api/rules/config"]),
    );
    // 27차 4번: 템플릿 화면은 읽기 · 고르기 · 값 맞추기 · 멈추기 · 승인 카드(읽기 · 승인) 경로를 부른다.
    expect(sorted(SCREEN_PATHS.templates)).toEqual(
      sorted([
        "/api/session",
        "/api/templates/current",
        "/api/templates",
        "/api/templates/adjust",
        "/api/templates/{id}/stop",
        "/api/approvals/{id}",
        "/api/approvals/{id}/approve",
      ]),
    );
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

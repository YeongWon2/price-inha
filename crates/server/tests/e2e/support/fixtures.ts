// 브라우저 e2e 공통(결정 23): 시연 서버 주소와, 모든 시험에 붙는 지킴이(화면 오류 · 바깥 요청 없음).
import { test as base, expect, type Page } from "@playwright/test";

// 시험 파일은 Node 형 정의 없이 형 검사한다(화면 tsconfig 하나를 함께 쓴다). 이 모듈 안에서만 쓰는 선언.
declare const process: { env: Record<string, string | undefined> };

/** 모든 화면 맨 위의 공통 메뉴(server 26차 3번). */
export const MENU = ["사무실", "계좌", "템플릿", "규칙", "히스토리", "설정"];

/** 담당 여섯(명단 순서). */
export const AGENTS = ["뉴스 공시 시세", "전략", "리스크", "복기", "매매", "팀장"];

/** 시험이 여는 여덟 화면. */
export const SCREENS = ["/", "/keys", "/office/", "/phone/", "/account/", "/templates/", "/rules/", "/history/"];

function serverUrl(): string {
  const url = process.env.PI_E2E_BASE_URL;
  if (!url) throw new Error("PI_E2E_BASE_URL 이 없다 — npm run e2e 로 돌린다(전역 준비가 시연 서버를 띄운다)");
  return url;
}

/** 같은 출처(시연 서버) · blob: · data: 만 허락한다. */
function local(url: string, origin: string): boolean {
  const u = new URL(url);
  if (u.protocol === "blob:" || u.protocol === "data:") return true;
  if (u.protocol === "ws:" || u.protocol === "wss:") return u.host === new URL(origin).host;
  return u.origin === origin;
}

export const test = base.extend<{ guard: undefined }>({
  // Playwright 는 첫 인자의 구조 분해로 기대는 fixture 를 읽는다(빈 구조 분해 = 기대는 것 없음).
  baseURL: async ({}, use) => {
    await use(serverUrl());
  },
  guard: [
    async ({ page, baseURL }, use) => {
      const origin = new URL(baseURL ?? serverUrl()).origin;
      const problems: string[] = [];
      page.on("pageerror", (e) => problems.push(`화면 오류(pageerror): ${e.message}`));
      page.on("request", (r) => {
        if (!local(r.url(), origin)) problems.push(`바깥 요청: ${r.url()}`);
      });
      page.on("websocket", (ws) => {
        if (!local(ws.url(), origin)) problems.push(`바깥 웹소켓: ${ws.url()}`);
      });
      await use(undefined);
      expect(problems, "화면 오류와 바깥 요청이 없어야 한다").toEqual([]);
    },
    { auto: true },
  ],
});

export { expect };

/** 내 캐릭터의 저장된 자리(`GET /api/office/me`). 화면과 같은 세션(쿠키)으로 읽는다. */
export async function myPlace(page: Page): Promise<{ x: number; y: number }> {
  const res = await page.request.get("/api/office/me");
  expect(res.ok(), `GET /api/office/me → ${res.status()}`).toBe(true);
  const body = (await res.json()) as { x: number; y: number };
  return { x: body.x, y: body.y };
}

/** 템플릿이 없으면 "단타"를 고른다(사무실에 내 캐릭터가 서려면 고른 템플릿이 있어야 한다). */
export async function ensureTemplate(page: Page): Promise<void> {
  await page.goto("/templates/");
  // 목록을 다 읽을 때까지(빈 상태 글이나 지금 쓰는 템플릿 가운데 하나가 보일 때까지) 기다린 뒤 고른다.
  await expect(page.getByText(/아직 고른 템플릿이 없습니다|지금 쓰는 템플릿/).first()).toBeVisible();
  if ((await page.getByText("지금 쓰는 템플릿").count()) === 0) {
    await page.getByRole("button", { name: "단타" }).click();
    await expect(page.getByText("지금 쓰는 템플릿")).toBeVisible();
  }
}

/** 사무실을 열고 3D 장면에 내 이름표가 설 때까지 기다린다. */
export async function openOffice(page: Page): Promise<void> {
  await page.goto("/office/");
  await expect(page.locator("#labels .tag.me")).toBeVisible();
}

/**
 * 방향키 하나를 누르고 있는 동안 저장된 자리가 바뀌기를 기다린다. 벽에 막혀 그 쪽으로 못 가면 다음 방향을 해 본다.
 * 어느 쪽으로든 걸었으면 `true`.
 */
export async function walkUntilMoved(page: Page, before: { x: number; y: number }): Promise<boolean> {
  for (const key of ["ArrowUp", "ArrowLeft", "ArrowDown", "ArrowRight"]) {
    await page.keyboard.down(key);
    const moved = await expect
      .poll(async () => myPlace(page), { timeout: 5_000 })
      .not.toEqual(before)
      .then(() => true, () => false);
    await page.keyboard.up(key);
    if (moved) return true;
  }
  return false;
}

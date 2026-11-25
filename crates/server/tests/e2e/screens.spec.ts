// 화면 여덟 · 공통 메뉴 · 좁은 폭(결정 23 특성 시험: 지금 동작을 지킨다).
// 모든 시험에 지킴이(support/fixtures.ts)가 붙어 화면 오류(pageerror)와 바깥 요청이 없음을 함께 본다.
import { MENU, SCREENS, expect, test } from "./support/fixtures";

for (const path of SCREENS) {
  test(`화면 ${path} 이 열리고 공통 메뉴가 있다`, async ({ page }) => {
    // 키 화면은 설정 화면에서 넘어간다: 설정 화면이 세션을 연 뒤에 열린다(바로 열면 "세션이 없습니다").
    if (path === "/keys") await page.goto("/");
    await page.goto(path);
    await expect(page.getByRole("navigation", { name: "주 메뉴" }).getByRole("link")).toHaveText(MENU);
  });
}

test("폭 390px에서 관리 화면 넷이 가로로 넘치지 않는다", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  for (const path of ["/account/", "/templates/", "/rules/", "/history/"]) {
    await page.goto(path);
    await expect(page.getByRole("navigation", { name: "주 메뉴" })).toBeVisible();
    // 서버에서 읽은 표 · 카드까지 그려진 뒤에 잰다.
    await page.waitForLoadState("networkidle");
    const wide = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(wide, `${path} 가 가로로 넘친다`).toBeLessThanOrEqual(0);
  }
});

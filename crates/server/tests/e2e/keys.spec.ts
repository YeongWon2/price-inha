// 키 화면(`/keys`, 기능 H 2번 · 4번): 같은 컴퓨터에서는 설정 화면을 거치지 않고 주소로 바로 열어도 화면이 세션을
// 얻고 등록 여부를 그린다. (사용자 화면에는 `/keys`로 가는 링크가 없다 — 주소로 연다.)
import { expect, test } from "./support/fixtures";

test("키 화면을 세션 없이 바로 열면 화면이 세션을 얻고 등록 여부를 그린다", async ({ page }) => {
  const session = page.waitForResponse((r) => r.url().endsWith("/api/session") && r.request().method() === "POST");
  const opened = await page.goto("/keys");
  expect(opened?.status(), "세션 없이 키 화면이 열린다").toBe(200);
  await expect(page.getByRole("heading", { name: "키 등록" })).toBeVisible();
  expect((await session).ok(), "화면이 세션을 얻는다(세션 경로는 204로 답한다)").toBe(true);
  await expect(page.locator("#k-slot-dart-reg")).toHaveText(/^\(등록(됨| 안 됨)\)$/);
});

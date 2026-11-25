// 근거 카드 · 승인 카드 보강(server 50차 · C23 · C24): 시연 서버에서 되는 것만 본다 — 사무실에 근거 패널이 숨은 채
// 붙어 있고, 없는 매매의 근거는 404, 폰 승인 카드에 보강 자리가 있다.
import { expect, test } from "./support/fixtures";

test("사무실에 근거 패널이 숨은 채 붙어 있고 없는 매매의 근거는 404다", async ({ page }) => {
  await page.goto("/office/");
  const panel = page.locator('[data-role="basis-panel"]');
  await expect(panel).toHaveCount(1);
  await expect(panel).toBeHidden();
  const res = await page.request.get("/api/trades/987654/basis");
  expect(res.status()).toBe(404);
  expect(await res.json()).toEqual({ error: "not_found" });
});

test("폰 승인 카드에 투표 수 · 한도 전 → 후 자리가 있다", async ({ page }) => {
  await page.goto("/phone/");
  await expect(page.locator('[data-card="approval"] [data-part="extras"]')).toHaveCount(1);
});

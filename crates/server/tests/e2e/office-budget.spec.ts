// 사무실 위 줄의 하루 AI 잔량 바(server 51차 · 차수 표 21번, C22 · V64). 시연 서버는 하루 상한이 없다.
// 고정 대기 대신 화면이 바뀌기를 기다린다.
import { AGENTS, ensureTemplate, expect, openOffice, test } from "./support/fixtures";

test("상한이 없으면 위 줄에 상한 없음과 쓴 수가 보이고, 누르면 담당 여섯의 사용 표가 열린다", async ({ page }) => {
  // 검증: V64 C22
  await ensureTemplate(page);
  await openOffice(page);
  const button = page.locator("#bar button.budget");
  await expect(button).toHaveText(/AI 상한 없음 · 오늘 \d+번/);
  await expect(button).toHaveAttribute("data-unlimited", "true");
  const table = page.locator("#bar .budget-table");
  await expect(table).toBeHidden();
  await button.click();
  await expect(table).toBeVisible();
  await expect(table.locator("tbody tr")).toHaveCount(6);
  for (const name of AGENTS) {
    await expect(table.locator("tbody th", { hasText: name })).toHaveCount(1);
  }
  await button.click();
  await expect(table).toBeHidden();
});

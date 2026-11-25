// 계좌 화면의 전체 정지(server 40차 1 · 2번, 결정 25): 시연 서버에서 "전체 정지"를 누르고 확인하면 상태가 전체 정지로
// 보이고 정지 단추 대신 "다시 시작"이 나타난다. 지킴이(support/fixtures.ts)가 화면 오류와 바깥 요청이 없음을 함께 본다.
//
// 시연 서버는 하나를 여러 시험이 함께 쓰고 차례대로(이름 차례) 돈다. 이 파일은 팀을 전체 정지로 두므로 이름이 가장
// 뒤에 오게 둔다. 시연 모드에는 증권사 · AI 키가 없어 "다시 시작"은 지금 규칙대로 거절된다(키를 먼저 등록하라는
// 안내, E9) — 그래서 다시 시작을 눌러 돌아오는 흐름은 이 시험에 넣지 않는다.
import { expect, test } from "./support/fixtures";

test("계좌 화면에서 전체 정지를 누르고 확인하면 전체 정지로 보이고 '다시 시작'이 나타난다", async ({ page }) => {
  await page.goto("/account/");
  const card = page.locator('[data-role="team"]');
  await expect(card).toBeVisible();
  const stop = card.locator('button[data-action="stop"]');
  await expect(stop).toBeVisible();
  await expect(page.locator('button[data-action="restart"]')).toHaveCount(0);

  const asked: string[] = [];
  page.once("dialog", (dialog) => {
    asked.push(dialog.message());
    void dialog.accept();
  });
  const sent = page.waitForResponse((r) => r.url().endsWith("/api/team/stop") && r.request().method() === "POST");
  await stop.click();
  expect((await sent).status()).toBe(200);
  expect(asked.length).toBe(1);
  expect(asked[0]).toContain("새 주문이 모두 막힙니다");

  await expect(page.locator('[data-role="team-state"]')).toContainText("전체 정지");
  await expect(page.locator('[data-role="stop-result"]')).toContainText("전체 정지했습니다");
  await expect(page.locator('button[data-action="restart"]')).toBeVisible();
  await expect(page.locator('button[data-action="stop"]')).toHaveCount(0);

  await page.reload();
  await expect(page.locator('[data-role="team-state"]')).toContainText("전체 정지");
  await expect(page.locator('button[data-action="restart"]')).toBeVisible();
});

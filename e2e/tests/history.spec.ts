import { expect, test } from "../support/primum";

test("the History tab lists an item's changes with their diffs", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/BUG-1#history`);
  await expect(page.locator("#doctabs button.on")).toHaveText(/history/);
  const effort = page.locator("ol.history details", { hasText: "BUG:1: effort" });
  await effort.locator("summary").click();
  await expect(effort.locator("pre.diff")).toContainText("+effort: 4");
});

test("a conflict is resolved from the item page", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await expect(page.locator("tr.item[data-id='KILLER:1'] .conflict-mark")).toBeVisible();
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/KILLER-1`);
  const urgency = page.locator(".conflict[data-what=urgency]");
  await expect(urgency.locator("button[data-keep='1']")).toContainText("8");
  await expect(urgency.locator("button[data-keep='2']")).toContainText("2");
  await urgency.locator("button[data-keep='2']").click();
  await expect(page.locator("#conflicts")).toBeHidden();
  await expect(page.locator("#controls [data-f=urgency]")).toHaveValue("2");
  await expect.poll(() => board.item("KILLER:1")).toContain("urgency: 2");
});

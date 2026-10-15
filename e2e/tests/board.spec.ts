import { existsSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "../support/primum";

const ids = (page: import("@playwright/test").Page) =>
  page.locator("#rows tr.item td.id").allTextContents();

test("lists items in priority order and filters by lane", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await expect(page).toHaveURL(/\/p\/i%C5%9Fler\/$/);
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
  // added BUG #1, TODO #2, KILLER #3, DEBT #4; listed by lane
  expect(await ids(page)).toEqual(["#1", "#4", "#3", "#2"]);
  await expect(page.locator("#next")).toContainText("next up");
  await expect(page.locator("#next")).toContainText("#1");
  await page.locator("#filters [data-k=KILLER]").click();
  expect(await ids(page)).toEqual(["#3"]);
  await page.locator("#filters [data-k=ALL]").click();
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
});

test("editing an effort changes the score and the file", async ({ page, board }) => {
  await page.goto(board.baseURL);
  const row = page.locator("tr.item[data-id='4']");
  await expect(row.locator("td.score")).toHaveText("22");
  await row.locator("[data-f=effort]").fill("2");
  await row.locator("[data-f=effort]").press("Tab");
  await expect(page.locator("tr.item[data-id='4'] td.score")).toHaveText("55");
  await expect.poll(() => board.item("4")).toContain("effort: 2");
});

test("a docs badge opens the item with that document", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await page.locator("tr.item[data-id='1'] .docbadge", { hasText: "plan" }).click();
  await expect(page).toHaveURL(/\/item\/1#items%2F1\.plan\.md$/);
  await expect(page.locator("#doctabs button.on")).toHaveText("plan");
  await expect(page.locator("#doc h1")).toHaveText("Galeri planı");
  await expect(page.locator("#view h2")).toHaveText("Neden");
  await page.locator("#doctabs button", { hasText: "spec" }).click();
  await expect(page.locator("#doc table td").first()).toHaveText("ad");
});

test("the item's text sits beside its documents, stacked when narrow", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/1`);
  await expect(page.locator("#doc h1")).toBeVisible();
  let left = await page.locator("section.left").boundingBox();
  let docs = await page.locator("#docpane").boundingBox();
  expect(docs!.x).toBeGreaterThan(left!.x + left!.width - 1);
  await page.setViewportSize({ width: 800, height: 900 });
  left = await page.locator("section.left").boundingBox();
  docs = await page.locator("#docpane").boundingBox();
  expect(docs!.y).toBeGreaterThanOrEqual(left!.y + left!.height - 1);
});

test("adding an item shows it and writes its file", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await page.locator("#nnew").click();
  await page.locator("#nkind").selectOption("BUG");
  await page.locator("#ntitle").fill("Işıl'ın yeni hatası");
  await page.locator("#nadd").click();
  await expect(page.locator("tr.item[data-id='5']")).toContainText("Işıl'ın yeni hatası");
  expect(existsSync(join(board.root, "items/5.md"))).toBe(true);
});

test("changing kind keeps the number; delete returns to the board", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/4`);
  await page.locator("#kind").selectOption("URGENT");
  await page.locator("#kindsave").click();
  await expect.poll(() => board.item("4")).toContain("kind: URGENT");
  await expect(page).toHaveURL(/\/item\/4$/);
  await expect(page.locator("#item-id")).toHaveText("#4");
  await expect(page.locator("#item-id")).toHaveClass(/kind-urgent/);
  page.once("dialog", (d) => d.accept());
  await page.locator("#del").click();
  await expect(page).toHaveURL(/\/p\/i%C5%9Fler\/$/);
  await expect(page.locator("#rows tr.item")).toHaveCount(3);
  expect(existsSync(join(board.root, "items/4.md"))).toBe(false);
});

test("the second project's tab shows its own items", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await page.locator("nav.secnav a", { hasText: "ödev" }).click();
  await expect(page).toHaveURL(/\/p\/%C3%B6dev\/$/);
  expect(await ids(page)).toEqual(["#1"]);
  await expect(page.locator(".header h1")).toHaveText("ödev");
});

test("#3 in an item's text links to item 3", async ({ page, board }) => {
  const { appendFile } = await import("node:fs/promises");
  await appendFile(join(board.root, "items/2.md"), "\nAfter #3, and not #99.\n");
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/2`);
  const link = page.locator("#view a.ref", { hasText: "#3" });
  await expect(link).toHaveAttribute("href", "3");
  await expect(link).toHaveAttribute("title", /^KILLER: /);
  await expect(page.locator("#view")).toContainText("not #99");
  await expect(page.locator("#view a", { hasText: "#99" })).toHaveCount(0);
  await link.click();
  await expect(page).toHaveURL(/\/item\/3$/);
});

test("a ticket offers idle, doing, done and wontfix", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await expect(page.locator("tr.item[data-id='4'] select[data-f=status]")).toBeVisible();
  const options = await page.locator("tr.item[data-id='4'] select[data-f=status] option").allTextContents();
  expect(options).toEqual(["idle", "doing", "done", "wontfix"]);
});

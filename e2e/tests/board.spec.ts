import { existsSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "../support/primum";

const ids = (page: import("@playwright/test").Page) =>
  page.locator("#rows tr.item td.id").allTextContents();

test("lists items in priority order and filters by lane", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await expect(page).toHaveURL(/\/p\/i%C5%9Fler\/$/);
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
  expect(await ids(page)).toEqual(["BUG:1", "DEBT:1", "KILLER:1", "TODO:1"]);
  await expect(page.locator("#next")).toContainText("next up");
  await expect(page.locator("#next")).toContainText("BUG:1");
  await page.locator("#filters [data-k=KILLER]").click();
  expect(await ids(page)).toEqual(["KILLER:1"]);
  await page.locator("#filters [data-k=ALL]").click();
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
});

test("editing an effort changes the score and the file", async ({ page, board }) => {
  await page.goto(board.baseURL);
  const row = page.locator("tr.item[data-id='DEBT:1']");
  await expect(row.locator("td.score")).toHaveText("22");
  await row.locator("[data-f=effort]").fill("2");
  await row.locator("[data-f=effort]").press("Tab");
  await expect(page.locator("tr.item[data-id='DEBT:1'] td.score")).toHaveText("55");
  await expect.poll(() => board.item("DEBT:1")).toContain("effort: 2");
});

test("a docs badge opens the item with that document", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await page.locator("tr.item[data-id='BUG:1'] .docbadge", { hasText: "plan" }).click();
  await expect(page).toHaveURL(/\/item\/BUG-1#items%2FBUG-1\.plan\.md$/);
  await expect(page.locator("#doctabs button.on")).toHaveText("plan");
  await expect(page.locator("#doc h1")).toHaveText("Galeri planı");
  await expect(page.locator("#view h2")).toHaveText("Neden");
  await page.locator("#doctabs button", { hasText: "spec" }).click();
  await expect(page.locator("#doc table td").first()).toHaveText("ad");
});

test("the item's text sits beside its documents, stacked when narrow", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/BUG-1`);
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
  await expect(page.locator("tr.item[data-id='BUG:2']")).toContainText("Işıl'ın yeni hatası");
  expect(existsSync(join(board.root, "items/BUG-2.md"))).toBe(true);
});

test("changing kind moves to the new id; delete returns to the board", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/DEBT-1`);
  await page.locator("#kind").selectOption("URGENT");
  await page.locator("#kindsave").click();
  await expect(page).toHaveURL(/\/item\/URGENT-1$/);
  await expect(page.locator("#item-id")).toHaveText("URGENT:1");
  expect(existsSync(join(board.root, "items/DEBT-1.md"))).toBe(false);
  page.once("dialog", (d) => d.accept());
  await page.locator("#del").click();
  await expect(page).toHaveURL(/\/p\/i%C5%9Fler\/$/);
  await expect(page.locator("#rows tr.item")).toHaveCount(3);
  expect(existsSync(join(board.root, "items/URGENT-1.md"))).toBe(false);
});

test("the second project's tab shows its own items", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await page.locator("nav.secnav a", { hasText: "ödev" }).click();
  await expect(page).toHaveURL(/\/p\/%C3%B6dev\/$/);
  expect(await ids(page)).toEqual(["URGENT:1"]);
  await expect(page.locator(".header h1")).toHaveText("ödev");
});

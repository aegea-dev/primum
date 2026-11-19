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

test("the id column sorts by number across lanes", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await page.locator("th[data-s=id]").click();
  await expect.poll(() => ids(page)).toEqual(["#1", "#2", "#3", "#4"]);
  await page.locator("th[data-s=id]").click();
  await expect.poll(() => ids(page)).toEqual(["#4", "#3", "#2", "#1"]);
});

test("the lane chips count what the table can show", async ({ page, board }) => {
  await page.goto(board.baseURL);
  const chip = (k: string) => page.locator(`#filters [data-k=${k}]`);
  await expect(chip("ALL")).toHaveText("ALL 4");
  await expect(chip("DEBT")).toHaveText("DEBT 1");
  await page.locator("tr.item[data-id='4'] [data-f=status]").selectOption("done");
  // Hide done is on: #4 leaves the table, and so does its count and its lane
  await expect(page.locator("#rows tr.item")).toHaveCount(3);
  await expect(chip("ALL")).toHaveText("ALL 3");
  await expect(chip("DEBT")).toHaveCount(0);
  await page.locator("#hide-done").uncheck();
  await expect(chip("ALL")).toHaveText("ALL 4");
  await expect(chip("DEBT")).toHaveText("DEBT 1");
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
  await page.locator("#edit").click();
  await page.locator("#kind").selectOption("URGENT");
  await page.locator("#save").click();
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
  await expect(page.locator("nav.secnav a.on")).toHaveText("ödev");
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

const width = async (page: import("@playwright/test").Page, col: string) =>
  (await page.locator(`table.items th[data-col=${col}]`).boundingBox())!.width;

test("columns fit their content; title, needs and docs share the rest", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
  const table = (await page.locator("table.items").boundingBox())!.width;
  const [title, needs, docs] = [await width(page, "title"), await width(page, "needs"), await width(page, "docs")];
  // 40%, 10% and 10% of the row, and what is left grows them 3:1:1
  expect(title - 0.4 * table).toBeGreaterThan(0);
  expect((title - 0.4 * table) / (needs - 0.1 * table)).toBeCloseTo(3, 1);
  expect(docs).toBeCloseTo(needs, 0);
  expect(await width(page, "id")).toBeLessThan(80);
  expect(await width(page, "impact")).toBeLessThan(90);
  // lines beside and under the headers, the colour of the lines between
  // rows; drawn by the cells themselves, as a collapsed border does not
  // move with a sticky cell
  const [th, row] = await page.evaluate(() => {
    const th = getComputedStyle(document.querySelector("table.items th[data-col=title]")!);
    return [[th.borderRightWidth, th.borderBottomWidth, th.boxShadow],
      getComputedStyle(document.querySelector("#rows td")!).borderBottomColor];
  });
  expect(th[0]).toBe("0px");
  expect(th[1]).toBe("0px");
  expect(th[2]).toBe(`${row} -1px 0px 0px 0px inset, ${row} 0px -1px 0px 0px inset`);
});

test("dragging a header line resizes its column, remembered after a reload", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
  const before = await width(page, "title");
  const grip = (await page.locator("table.items th[data-col=title] .grip").boundingBox())!;
  const y = grip.y + grip.height / 2;
  await page.mouse.move(grip.x + grip.width / 2, y);
  await page.mouse.down();
  await page.mouse.move(grip.x + grip.width / 2 - 300, y, { steps: 5 });
  await page.mouse.up();
  expect(await width(page, "title")).toBeCloseTo(before - 300, -1);
  // the drag did not sort by title
  await expect(page.locator("table.items th[data-col=title]")).not.toHaveClass(/sorted/);
  // a title that no longer fits goes on to the next line
  const cell = page.locator("tr.item[data-id='1'] td.title");
  const line = await cell.evaluate((td) => parseFloat(getComputedStyle(td).lineHeight) || 20);
  await page.reload();
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
  expect(await width(page, "title")).toBeCloseTo(before - 300, -1);
  const shrunk = (await page.locator("table.items th[data-col=title] .grip").boundingBox())!;
  await page.mouse.move(shrunk.x + shrunk.width / 2, y);
  await page.mouse.down();
  await page.mouse.move(80, y, { steps: 5 });
  await page.mouse.up();
  expect((await cell.boundingBox())!.height).toBeGreaterThan(line * 1.5);
});

test("double-clicking a header line fits the column to its content", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
  const before = await width(page, "needs");
  await page.locator("table.items th[data-col=needs] .grip").dblclick();
  const fitted = await width(page, "needs");
  expect(fitted).toBeLessThan(before - 20);
  // "needs first" stays on one line
  const header = page.locator("table.items th[data-col=needs]");
  const line = await header.evaluate((th) => parseFloat(getComputedStyle(th).lineHeight) || 16);
  expect((await header.boundingBox())!.height).toBeLessThan(line * 1.5 + 14);
  await expect(header).not.toHaveClass(/sorted/);
  await page.reload();
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
  expect(await width(page, "needs")).toBeCloseTo(fitted, 0);
});

test("dragging the line between text and documents resizes them, remembered", async ({ page, board }) => {
  const url = `${board.baseURL}/p/i%C5%9Fler/item/1`;
  await page.goto(url);
  await expect(page.locator("#doc h1")).toBeVisible();
  const left = page.locator("section.left");
  const docs = page.locator("#docpane");
  const start = (await left.boundingBox())!.width;
  // by default the two sides are as wide as each other
  expect(start).toBeCloseTo((await docs.boundingBox())!.width, 0);
  const line = (await page.locator("#divider").boundingBox())!;
  const y = line.y + 40;
  await page.mouse.move(line.x + line.width / 2, y);
  await page.mouse.down();
  await page.mouse.move(line.x + line.width / 2 + 200, y, { steps: 5 });
  await page.mouse.up();
  expect((await left.boundingBox())!.width).toBeCloseTo(start + 200, -1);
  expect((await docs.boundingBox())!.x).toBeGreaterThan(line.x + 200);
  await page.reload();
  await expect(page.locator("#doc h1")).toBeVisible();
  expect((await left.boundingBox())!.width).toBeCloseTo(start + 200, -1);
  // a double-click puts it back
  await page.locator("#divider").dblclick();
  expect((await left.boundingBox())!.width).toBeCloseTo(start, 0);
  await page.reload();
  await expect(page.locator("#doc h1")).toBeVisible();
  expect((await left.boundingBox())!.width).toBeCloseTo(start, 0);
});

test("neither side of the item page can be dragged away", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/1`);
  await expect(page.locator("#doc h1")).toBeVisible();
  const line = (await page.locator("#divider").boundingBox())!;
  const y = line.y + 40;
  await page.mouse.move(line.x + line.width / 2, y);
  await page.mouse.down();
  await page.mouse.move(1430, y, { steps: 5 });
  await page.mouse.up();
  expect((await page.locator("#docpane").boundingBox())!.width).toBeGreaterThanOrEqual(239);
  const now = (await page.locator("#divider").boundingBox())!;
  await page.mouse.move(now.x + now.width / 2, y);
  await page.mouse.down();
  await page.mouse.move(5, y, { steps: 5 });
  await page.mouse.up();
  expect((await page.locator("section.left").boundingBox())!.width).toBeGreaterThanOrEqual(239);
  // stacked when narrow: no line to drag
  await page.setViewportSize({ width: 800, height: 900 });
  await expect(page.locator("#divider")).toBeHidden();
});

test("the item's text and documents fill their side, wide content included", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/1`);
  await expect(page.locator("#doc h1")).toBeVisible();
  const long = "çalışma_günlüğü_".repeat(20);
  const body = [
    "## Neden", "", "Gökçe galeriyi açınca boş sayfa görüyor.", "",
    "```", `ayar=${long}`, "```", "",
    `| alan | açıklama |`, `|---|---|`, `| ${long} | ${long} |`, "",
  ].join("\n");
  await page.locator("#edit").click();
  await page.locator("#editor").fill(body);
  await page.locator("#save").click();
  await expect(page.locator("#view pre")).toBeVisible();
  const drag = async (to: number) => {
    const line = (await page.locator("#divider").boundingBox())!;
    const y = line.y + 40;
    await page.mouse.move(line.x + line.width / 2, y);
    await page.mouse.down();
    await page.mouse.move(to, y, { steps: 5 });
    await page.mouse.up();
  };
  const fits = (sel: string) => page.locator(sel).evaluate((el) => el.scrollWidth <= el.clientWidth + 1);
  const fill = async (inner: string, outer: string) =>
    (await page.locator(inner).boundingBox())!.width / (await page.locator(outer).boundingBox())!.width;
  // left side wide: the text fills it
  await drag(1200);
  expect(await fill("#view", "section.left")).toBeGreaterThan(0.98);
  // left side narrow: nothing inside it is wider than it
  await drag(5);
  for (const sel of ["#view", "#view pre", "#view table"]) expect(await fits(sel)).toBe(true);
  expect((await page.locator("#view table").boundingBox())!.width)
    .toBeLessThanOrEqual((await page.locator("#view").boundingBox())!.width + 1);
  // the documents fill their side too
  expect(await fill("#doc", "#docpane")).toBeGreaterThan(0.95);
  await drag(1430);
  expect(await fits("#doc")).toBe(true);
});

test("each side of the item page scrolls on its own, its head staying on screen", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/1`);
  await expect(page.locator("#doc h1")).toBeVisible();
  const body = ["## Neden", ""].concat(Array.from({ length: 60 }, (_, n) => `Gökçe ${n}. kez galeriyi açıyor ve boş sayfa görüyor.\n`)).join("\n");
  await page.locator("#edit").click();
  await page.locator("#editor").fill(body);
  await page.locator("#save").click();
  await expect(page.locator("#view p").nth(50)).toBeAttached();
  // the fixture's spec is short: lengthen the shown document
  await page.locator("#doc").evaluate((el) => { el.innerHTML += "<p>ayrıntı</p>".repeat(80); });
  const top = async (sel: string) => (await page.locator(sel).boundingBox())!.y;
  // side by side the page itself does not scroll: one scrollbar per side
  expect(await page.evaluate(() => document.documentElement.scrollHeight <= innerHeight)).toBe(true);
  const left = (await page.locator("section.left").boundingBox())!;
  await page.mouse.move(left.x + 100, left.y + 300);
  await page.mouse.wheel(0, 1500);
  await expect.poll(() => page.locator("section.left").evaluate((el) => el.scrollTop)).toBeGreaterThan(1000);
  expect(await top(".item-title")).toBeCloseTo(left.y, 0);
  await expect(page.locator("#controls")).toBeInViewport();
  const pane = (await page.locator("#docpane").boundingBox())!;
  await page.mouse.move(pane.x + 100, pane.y + 300);
  await page.mouse.wheel(0, 1500);
  await expect.poll(() => page.locator("#docpane").evaluate((el) => el.scrollTop)).toBeGreaterThan(1000);
  expect(await top("#doctabs")).toBeCloseTo(pane.y, 0);
  await expect(page.locator("#docpath")).toBeInViewport();
  // the left side did not move with the documents
  expect(await page.locator("section.left").evaluate((el) => el.scrollTop)).toBeGreaterThan(1000);
  // stacked on a narrow page, the page scrolls and the head stays under the bar
  await page.setViewportSize({ width: 800, height: 600 });
  await page.evaluate(() => window.scrollTo(0, 1500));
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(1000);
  const bar = (await page.locator(".gallery-toolbar").boundingBox())!;
  expect(await top("section.left .sidehead")).toBeCloseTo(bar.y + bar.height, 1);
});

test("the page's top stays in one piece, and what sticks below meets it", async ({ page, board }) => {
  const edges = (sels: string[]) => page.evaluate((s) =>
    s.map((sel) => { const b = document.querySelector(sel)!.getBoundingClientRect(); return [b.top, b.bottom]; }), sels);
  const top = ["header.pagetop", "nav.secnav", ".header", ".gallery-toolbar"];
  const together = (t: number[][]) => {
    expect(t[0][0]).toBe(0);
    for (let n = 2; n < t.length; n++) expect(t[n][0]).toBeCloseTo(t[n - 1][1], 2);
    expect(t[0][1]).toBeCloseTo(t[t.length - 1][1], 2);
  };
  await page.setViewportSize({ width: 1440, height: 300 });
  await page.goto(board.baseURL);
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(50);
  let [pagetop, nav, header, bar, below] = await edges([...top, "table.items th"]);
  together([pagetop, nav, header, bar]);
  expect(below[0]).toBeCloseTo(pagetop[1], 2);
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/1`);
  await expect(page.locator("#doc h1")).toBeVisible();
  await page.locator("#edit").click();
  await page.locator("#editor").fill(Array.from({ length: 60 }, (_, n) => `Satır ${n}: ğüşıöç`).join("\n\n"));
  await page.locator("#save").click();
  // stacked, where the page scrolls under its top
  await page.setViewportSize({ width: 800, height: 600 });
  await page.evaluate(() => window.scrollTo(0, 800));
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(50);
  [pagetop, nav, header, bar, below] = await edges([...top, "section.left .sidehead"]);
  together([pagetop, nav, header, bar]);
  expect(below[0]).toBeCloseTo(pagetop[1], 2);
});

test("a sticky head fades what scrolls under it, only once something does", async ({ page, board }) => {
  const fade = (sel: string) => page.locator(sel).evaluate((el) => getComputedStyle(el, "::after").opacity);
  await page.setViewportSize({ width: 1440, height: 260 });
  await page.goto(board.baseURL);
  await expect(page.locator("#rows tr.item")).toHaveCount(4);
  await expect.poll(() => fade("table.items th[data-col=title]")).toBe("0");
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect.poll(() => fade("table.items th[data-col=title]")).toBe("1");
  await page.evaluate(() => window.scrollTo(0, 0));
  await expect.poll(() => fade("table.items th[data-col=title]")).toBe("0");
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/1`);
  await expect(page.locator("#doc h1")).toBeVisible();
  await page.locator("#edit").click();
  await page.locator("#editor").fill(Array.from({ length: 60 }, (_, n) => `Satır ${n}: ğüşıöç`).join("\n\n"));
  await page.locator("#save").click();
  await expect(page.locator("#view p").nth(50)).toBeAttached();
  const head = "section.left .sidehead";
  await expect.poll(() => fade(head)).toBe("0");
  await page.locator("section.left").evaluate((el) => { el.scrollTop = 600; });
  await expect.poll(() => fade(head)).toBe("1");
  // the documents' head does not fade while they are not scrolled
  expect(await fade("#docpane .sidehead")).toBe("0");
});

test("the item page shows its fields until Edit makes them all editable", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/1`);
  await expect(page.locator("#doc h1")).toBeVisible();
  // at rest: values, no fields
  await expect(page.locator("#controls [data-v=status]")).toHaveText("idle");
  await expect(page.locator("#controls [data-v=urgency]")).toHaveText("7");
  await expect(page.locator(".sidehead input:visible, .sidehead select:visible, #editor:visible")).toHaveCount(0);
  // Edit: the title, status, scores, kind and text become fields
  await page.locator("#edit").click();
  await expect(page.locator("#title-field")).toHaveValue("Gökçe'nin galerisi boş açılıyor");
  for (const f of ["impact", "urgency", "risk", "effort"]) await expect(page.locator(`#controls input[data-f=${f}]`)).toBeEditable();
  await expect(page.locator("#controls select[data-f=status]")).toBeEnabled();
  await expect(page.locator("#kind")).toBeVisible();
  await expect(page.locator("#editor")).toBeVisible();
  // Cancel drops every change
  await page.locator("#controls input[data-f=impact]").fill("2");
  await page.locator("#title-field").fill("Başka başlık");
  await page.locator("#cancel").click();
  await expect(page.locator("#controls [data-v=impact]")).toHaveText("8");
  await expect(page.locator("#item-title")).toHaveText("Gökçe'nin galerisi boş açılıyor");
  expect(await board.item("1")).toContain("impact: 8");
  // Save keeps them all
  await page.locator("#edit").click();
  await page.locator("#title-field").fill("Galeri boş açılıyor, ışık yok");
  await page.locator("#controls select[data-f=status]").selectOption("doing");
  await page.locator("#controls input[data-f=urgency]").fill("3");
  await page.locator("#editor").fill("## Neden\n\nGüncel metin: şimdi düzeldi mi?\n");
  await page.locator("#save").click();
  await expect(page.locator("#controls [data-v=status]")).toHaveText("doing");
  await expect(page.locator("#controls [data-v=urgency]")).toHaveText("3");
  await expect(page.locator("#item-title")).toHaveText("Galeri boş açılıyor, ışık yok");
  await expect(page.locator("#view")).toContainText("Güncel metin");
  await expect(page.locator(".sidehead input:visible, .sidehead select:visible")).toHaveCount(0);
  const file = await board.item("1");
  for (const s of ["status: doing", "urgency: 3", "Galeri boş açılıyor, ışık yok", "Güncel metin"]) expect(file).toContain(s);
});

test("a closed item's Edit changes only its status", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/4`);
  await page.locator("#edit").click();
  await page.locator("#controls select[data-f=status]").selectOption("done");
  await page.locator("#save").click();
  await expect(page.locator("#controls [data-v=status]")).toHaveText("done");
  await page.locator("#edit").click();
  await expect(page.locator("#controls select[data-f=status]")).toBeEnabled();
  await expect(page.locator("#controls input[data-f=impact]")).toBeDisabled();
  await expect(page.locator("#title-field, #editor:visible, #kind:visible")).toHaveCount(0);
  await page.locator("#controls select[data-f=status]").selectOption("idle");
  await page.locator("#save").click();
  await expect(page.locator("#controls [data-v=status]")).toHaveText("idle");
  await expect.poll(() => board.item("4")).toContain("status: idle");
});

test("the item's buttons sit under its fields, and the score follows the scores", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/1`);
  await expect(page.locator("#doc h1")).toBeVisible();
  // in the sticky head, under the fields, above the text
  await expect(page.locator(".sidehead #edit")).toBeVisible();
  const controls = (await page.locator("#controls").boundingBox())!;
  const edit = (await page.locator("#edit").boundingBox())!;
  expect(edit.y).toBeGreaterThanOrEqual(controls.y + controls.height - 1);
  expect(edit.y).toBeLessThan((await page.locator("#view").boundingBox())!.y);
  await page.locator("#edit").click();
  const field = (f: string) => page.locator(`#controls input[data-f=${f}]`);
  const risk = Number(await field("risk").inputValue());
  await field("impact").fill("9");
  await field("effort").fill("3");
  // (impact + urgency + risk) x 10 / effort, to one decimal, before saving
  const want = Math.round(((9 + 7 + risk) * 10 / 3) * 10) / 10;
  await expect(page.locator("#controls .score")).toHaveText(String(want));
  await page.locator("#cancel").click();
  await expect(page.locator("#controls .score")).not.toHaveText(String(want));
});

test("a document is edited in its own pane", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/1`);
  await expect(page.locator("#doc h1")).toHaveText("Galeri spec");
  await expect(page.locator("#doceditor")).toBeHidden();
  await page.locator("#docedit").click();
  await expect(page.locator("#doceditor")).toHaveValue(/^# Galeri spec/);
  expect(await page.locator("#doceditor").evaluate((el: HTMLTextAreaElement) => [el.selectionStart, el.scrollTop])).toEqual([0, 0]);
  await expect(page.locator("#doc")).toBeHidden();
  // cancel keeps the document as it was
  await page.locator("#doceditor").fill("# Silinecek\n");
  await page.locator("#doccancel").click();
  await expect(page.locator("#doc h1")).toHaveText("Galeri spec");
  // save writes it
  await page.locator("#docedit").click();
  await page.locator("#doceditor").fill("# Galeri spec, ikinci sürüm\n\nÜç alan: ad, tür, açıklama.\n");
  await page.locator("#docsave").click();
  await expect(page.locator("#doc h1")).toHaveText("Galeri spec, ikinci sürüm");
  await expect(page.locator("#doceditor")).toBeHidden();
  const { readFile } = await import("node:fs/promises");
  await expect.poll(() => readFile(join(board.root, "items/1.spec.md"), "utf8")).toContain("Üç alan");
  // another tab shows its own text; the history has nothing to edit
  await page.locator("#doctabs button", { hasText: "plan" }).click();
  await expect(page.locator("#doc h1")).toHaveText("Galeri planı");
  await page.locator("#doctabs button", { hasText: "history" }).click();
  await expect(page.locator("#docedit")).toBeHidden();
});

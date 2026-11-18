// Run against Storybook's production route with isolated IPC fixtures only:
// PLAYWRIGHT_MODULE_PATH=/path/to/playwright node tools/qa/selective-download.cjs
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || "playwright");
const url = `${process.env.STORYBOOK_URL || "http://localhost:6006"}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(), "dlsite-selection-qa-"));
const key = "dldr-selected-works-v1";

async function hook(page) {
  await page.evaluate(() => {
    const qa = (window.__selectionQa = { previews: [], starts: [] });
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async function(command, args, ...rest) {
      if (command === "preview_bulk_work_download") {
        // Native IPC serializes reactive arrays; structuredClone rejects Svelte proxies.
        qa.previews.push(JSON.parse(JSON.stringify(args.request)));
        if (qa.failPreview) throw new Error("Preview unavailable (simulated)");
        if (qa.holdPreview) {
          qa.holdPreview = false;
          await new Promise((resolve) => { qa.releasePreview = resolve; });
        }
      }
      if (command === "start_bulk_work_download") {
        qa.starts.push(JSON.parse(JSON.stringify(args.request)));
        if (qa.failStart) throw new Error("Queue unavailable (simulated)");
      }
      const result = await invoke.call(this, command, args, ...rest);
      if (command === "preview_bulk_work_download" && qa.unknownSize) {
        return { ...result, totalExpectedBytes: null, knownExpectedBytes: 0, unknownSizeCount: result.requestedCount };
      }
      return result;
    };
  });
}
const selected = (page) => page.evaluate((key) => JSON.parse(localStorage.getItem(key) || "[]"), key);
const enabled = (page) => page.locator(".product-card input[type=checkbox]:enabled");
const download = (page) => page.getByRole("button", { name: /^Download selected \(/ });
async function search(page, text) {
  await page.getByRole("combobox", { name: "Search library" }).fill(text);
  await page.getByRole("button", { name: "Search", exact: true }).click();
}

(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE_PATH });
  let page;
  try {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    page = await context.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.setDefaultTimeout(10000);
    await page.goto(url);
    await page.getByText("Page 1 of 8", { exact: true }).first().waitFor();
    await hook(page);
    await page.waitForFunction(() => document.querySelectorAll('.product-card').length === 30);
    const bottomPages = page.getByRole('navigation', {name:'Library pages at bottom',exact:true});
    assert.equal(await bottomPages.getByRole('button',{name:'Previous',exact:true}).isDisabled(),true);
    for (const invalid of ['0','9','1.5']) {
      await bottomPages.getByRole('spinbutton',{name:'Page number',exact:true}).fill(invalid);
      assert.equal(await bottomPages.getByRole('button',{name:'Go',exact:true}).isDisabled(),true);
    }
    await bottomPages.getByRole('spinbutton',{name:'Page number',exact:true}).fill('8');
    await bottomPages.getByRole('button',{name:'Go',exact:true}).click();
    await page.getByText('Page 8 of 8',{exact:true}).first().waitFor();
    await page.waitForFunction(() => document.querySelectorAll('.product-card').length === 6);
    assert.equal(await bottomPages.getByRole('button',{name:'Next',exact:true}).isDisabled(),true);
    await page.waitForFunction(() => document.activeElement?.classList.contains('list-header'));
    await bottomPages.getByRole('button',{name:'Go to page 1',exact:true}).click();
    await page.getByText('Page 1 of 8',{exact:true}).first().waitFor();
    await page.waitForFunction(() => document.querySelectorAll('.product-card').length === 30);
    assert.equal(await download(page).count(), 0);

    // Range selection, normal cover/details action, and page selection are independent.
    await enabled(page).nth(0).check();
    await enabled(page).nth(2).click({ modifiers: ["Shift"] });
    assert.equal((await selected(page)).length, 3);
    assert.equal(await page.getByRole("dialog").count(), 0);
    const firstPageIds = (await selected(page)).map((p) => p.workId);
    await page.locator(".list-header").getByRole("button", { name: "Next", exact: true }).click();
    await page.getByText("Page 2 of 8", { exact: true }).first().waitFor();
    await enabled(page).nth(0).check();
    const acrossPages = await selected(page);
    assert.equal(acrossPages.length, 4);
    await page.getByRole("button", { name: "Select page", exact: true }).click();
    assert.ok((await selected(page)).length > 4);
    await page.getByRole("button", { name: "Deselect page", exact: true }).click();
    assert.deepEqual((await selected(page)).map((p) => p.workId), firstPageIds);
    await enabled(page).nth(0).check();

    // Saved selection survives navigation, restart, and an empty search result.
    await page.locator("aside nav").getByRole("button", { name: "Settings", exact: true }).click();
    await page.locator(".settings-layout > .page-header .tabs").getByRole("button", { name: "Accounts", exact: true }).click();
    await page.getByRole("button", { name: "Library", exact: true }).click();
    assert.equal((await selected(page)).length, 4);
    await page.reload();
    await page.getByText("Page 1 of 8", { exact: true }).first().waitFor();
    await hook(page);
    assert.equal((await selected(page)).length, 4);
    await search(page, "no-such-work-xyz");
    await page.getByText("No products", { exact: true }).waitFor();
    assert.ok(await download(page).isEnabled());
    await page.getByRole("button", { name: "Review selected", exact: true }).click();
    const review = page.getByRole("dialog", { name: "Selected works (4)", exact: true });
    await review.waitFor();
    assert.equal(await review.getByRole("listitem").count(), 4);
    await review.getByRole("button", { name: /^Remove .* from selection$/ }).first().click();
    await page.getByRole("dialog", { name: "Selected works (3)", exact: true }).waitFor();
    await page.getByRole("dialog").getByRole("button", { name: "Close", exact: true }).click();

    // The same explicit IDs reach preview and start, with no current search/filter restriction.
    const expected = (await selected(page)).map((p) => p.workId);
    await download(page).click();
    const confirmation = page.getByRole("dialog", { name: "Download selected works?", exact: true });
    await confirmation.waitFor();
    assert.equal(await confirmation.getByRole("listitem").count(), expected.length);
    const concurrency = confirmation.getByRole("group", { name: "Simultaneous works per batch", exact: true });
    assert.equal(await concurrency.getByRole("button", { name: "2", exact: true }).getAttribute("aria-pressed"), "true");
    await concurrency.getByRole("button", { name: "4", exact: true }).click();
    for (const viewport of [{ width: 390, height: 844 }, { width: 1280, height: 900 }]) {
      await page.setViewportSize(viewport);
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      const start = await confirmation.getByRole("button", { name: "Start Download", exact: true }).boundingBox();
      const modal = await confirmation.boundingBox();
      assert.ok(start && modal && start.y >= modal.y && start.y + start.height <= modal.y + modal.height, "confirmation action must stay visible");
    }
    await page.screenshot({ path: path.join(captures, "confirmation.png") });
    await confirmation.getByRole("button", { name: "Start Download", exact: true }).click();
    await page.waitForFunction(() => window.__selectionQa.starts.length === 1);
    const requests = await page.evaluate(() => window.__selectionQa);
    assert.deepEqual(requests.starts[0], { ...requests.previews[0], maxConcurrentWorks: 4 });
    assert.equal(await page.evaluate(() => localStorage.getItem("dldr-download-concurrency-v1")), "4");
    assert.deepEqual(requests.starts[0].workIds, expected);
    assert.equal(requests.starts[0].search, null);
    assert.deepEqual(requests.starts[0].makerKeys, []);
    assert.equal((await selected(page)).length, 3);

    // Failed planning never queues a download; Cancel also leaves selection intact.
    await page.evaluate(() => { window.__selectionQa.failPreview = true; });
    await download(page).click();
    await page.getByText(/Preview unavailable/).first().waitFor();
    assert.equal(await page.evaluate(() => window.__selectionQa.starts.length), 1);
    await page.evaluate(() => { window.__selectionQa.failPreview = false; window.__selectionQa.unknownSize = true; });
    await download(page).click();
    await confirmation.waitFor();
    await confirmation.getByText(/unknown file\(s\)/).waitFor();
    await confirmation.getByRole("button", { name: "Cancel", exact: true }).click();
    assert.equal(await page.evaluate(() => window.__selectionQa.starts.length), 1);

    // Changing selection while preview is pending cannot change the confirmed IDs.
    await search(page, "");
    await page.getByText("Page 1 of 8", { exact: true }).first().waitFor();
    await page.evaluate(() => { window.__selectionQa.holdPreview = true; window.__selectionQa.unknownSize = false; });
    await download(page).click();
    await page.waitForFunction(() => !!window.__selectionQa.releasePreview);
    const planned = await selected(page);
    await page.getByRole("button", { name: "Clear selection", exact: true }).click();
    await enabled(page).nth(5).check();
    const revised = await selected(page);
    await page.evaluate(() => window.__selectionQa.releasePreview());
    await confirmation.waitFor();
    assert.equal(await confirmation.getByRole("listitem").count(), planned.length);
    await confirmation.getByRole("button", { name: "Start Download", exact: true }).click();
    await page.waitForFunction(() => window.__selectionQa.starts.length === 2);
    assert.deepEqual(await page.evaluate(() => window.__selectionQa.starts[1].workIds), planned.map((p) => p.workId));
    assert.deepEqual(await selected(page), revised);

    // Review remains usable at narrow widths, and the sticky bar does not overflow.
    await page.locator('button[aria-label="Dismiss notification"]').evaluateAll(buttons => buttons.forEach(button => button.click()));
    await page.screenshot({ path: path.join(captures, "library.png") });
    const bar = await page.getByRole("region", { name: "Download selection" }).boundingBox();
    assert.ok(bar && bar.y < 900 && bar.y + bar.height <= 900, "selection bar must stay in view");
    await page.setViewportSize({ width: 390, height: 844 });
    await page.getByRole("button", { name: "Review selected", exact: true }).click();
    await page.getByRole("dialog", { name: "Selected works (1)", exact: true }).waitFor();
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.screenshot({ path: path.join(captures, "narrow-review.png") });
    await page.getByRole("dialog").getByRole("button", { name: "Close", exact: true }).click();
    await page.getByRole("button", { name: "Clear selection", exact: true }).click();
    assert.equal((await selected(page)).length, 0);
    assert.equal(await download(page).count(), 0);
    await page.setViewportSize({ width: 1280, height: 900 });
    await search(page, revised[0].workId);
    await page.getByText("Page 1 of 1", { exact: true }).first().waitFor();
    const filteredButton = page.getByRole("button", { name: "Download filtered results", exact: true });
    await filteredButton.click();
    const filteredDialog = page.getByRole("dialog", { name: "Start bulk download?", exact: true });
    await filteredDialog.waitFor();
    await filteredDialog.getByRole("button", { name: "Start Download", exact: true }).click();
    await page.waitForFunction(() => window.__selectionQa.starts.length === 3);
    const filteredRequest = await page.evaluate(() => window.__selectionQa.starts[2]);
    assert.equal(filteredRequest.search, revised[0].workId);
    assert.equal(filteredRequest.workIds, undefined);
    assert.equal(filteredRequest.maxConcurrentWorks, 4);
    await page.reload();
    await page.getByText("Page 1 of 8", { exact: true }).first().waitFor();
    await hook(page);
    await search(page, revised[0].workId);
    await filteredButton.click();
    await filteredDialog.waitFor();
    const restoredConcurrency = filteredDialog.getByRole("group", { name: "Simultaneous works per batch", exact: true });
    assert.equal(await restoredConcurrency.getByRole("button", { name: "4", exact: true }).getAttribute("aria-pressed"), "true");
    await restoredConcurrency.getByRole("button", { name: "1", exact: true }).click();
    await filteredDialog.getByRole("button", { name: "Cancel", exact: true }).click();
    assert.equal(await page.evaluate(() => window.__selectionQa.starts.length), 0);
    assert.deepEqual(errors, []);
    console.log(JSON.stringify({ passed: true, captures, scenarios: ["shift range", "cross-page selection", "page select/deselect", "restart persistence", "empty search", "cover review/removal", "explicit IDs", "cancel/error", "immutable preview", "unknown capacity", "narrow layout", "filtered-download compatibility"] }));
    await context.close();
  } catch (error) {
    if (page) {
      await page.screenshot({ path: path.join(captures, "failure.png") });
      fs.writeFileSync(path.join(captures, "failure.txt"), await page.locator("body").innerText());
      fs.writeFileSync(path.join(captures, "requests.json"), JSON.stringify((await page.evaluate(() => window.__selectionQa)) ?? null, null, 2));
    }
    console.error(`Failure captures: ${captures}`);
    throw error;
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });

// Uses Storybook's production route and mocked IPC; never signs in or downloads content.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || "playwright");
const url = `${process.env.STORYBOOK_URL || "http://localhost:6006"}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(), "dlsite-progress-qa-"));

async function emit(page, overrides = {}) {
  await page.evaluate(async (overrides) => {
    const qa = window.__progressQa;
    const previous = qa.jobs.find((job) => job.id === (overrides.id ?? "qa-bulk"));
    const snapshot = { ...qa.base, ...previous, ...overrides };
    qa.jobs = [...qa.jobs.filter((job) => job.id !== snapshot.id), snapshot];
    await window.__TAURI_INTERNALS__.invoke("plugin:event|emit", { event: "dldr-job-event", payload: {
      sequence: ++qa.sequence, eventKind: "updated", jobId: snapshot.id, kind: snapshot.kind,
      status: snapshot.status, phase: snapshot.phase, progress: snapshot.progress,
      message: null, log: null, snapshot,
    } });
  }, overrides);
}

(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE_PATH });
  let page;
  try {
    page = await browser.newPage({ viewport: { width: 1280, height: 960 } });
    page.setDefaultTimeout(10000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(url);
    await page.getByText("Page 1 of 8", { exact: true }).first().waitFor();
    const card = page.locator(".product-card").filter({ has: page.locator("input:enabled") }).first();
    const checkbox = card.getByRole("checkbox");
    const style = await card.locator(".selection-mark").evaluate((element) => {
      const style = getComputedStyle(element), box = element.getBoundingClientRect();
      return { width: box.width, height: box.height, border: style.borderTopWidth, background: style.backgroundColor };
    });
    assert.ok(style.width >= 32 && style.height >= 32 && parseFloat(style.border) >= 2);
    assert.equal(style.background, "rgb(255, 255, 255)");
    await checkbox.check();
    assert.equal(await card.locator(".selection-mark").textContent(), "✓");
    await checkbox.focus();
    await checkbox.press("Space");
    assert.equal(await checkbox.isChecked(), false);
    assert.equal(await page.getByRole("dialog").count(), 0);
    assert.ok(await page.locator(".file-size").filter({ hasText: /MiB|GiB/ }).count());
    await page.getByRole("combobox", { name: "Search library" }).fill("RJ15000011");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await page.locator(".file-size").filter({ hasText: "Size unknown" }).first().waitFor();
    await page.getByRole("combobox", { name: "Search library" }).fill("");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await page.getByText("Page 1 of 8", { exact: true }).first().waitFor();
    await checkbox.check();
    await page.screenshot({ path: path.join(captures, "library.png") });

    await page.evaluate(() => {
      const qa = window.__progressQa = { jobs: [], sequence: 0, calls: [] };
      qa.base = {
        id: "qa-bulk", kind: "bulkWorkDownload", title: "Selected works", status: "running",
        phase: "bulkDownloading", progress: { current: 1, total: 4, unit: "items" },
        metadata: { requestedCount: 4, maxConcurrentWorks: 4 }, output: null, error: null, cancellable: true,
        createdAt: new Date().toISOString(), startedAt: new Date().toISOString(), finishedAt: null,
      };
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = function(command, args, ...rest) {
        qa.calls.push(command);
        if (command === "list_jobs") return Promise.resolve(qa.jobs);
        if (command === "list_products") return Promise.resolve(invoke.call(this, command, args, ...rest)).then(async result => {
          if (qa.holdReads) await new Promise(resolve => qa.pendingReads.push(resolve));
          return {...result, products: result.products.map(product => product.workId === qa.completedWorkId
            ? {...product, download: {...product.download, status: "downloaded", localPath: `/qa/downloads/${product.workId}`}}
            : product)};
        });
        return invoke.call(this, command, args, ...rest);
      };
    });
    await page.locator(".download-status").click();
    await page.locator(".download-panel footer").getByRole("button", { name: "Reload", exact: true }).click();
    await page.locator(".download-panel .entries").waitFor({ state: "attached" });
    const MiB = 1024 * 1024;
    const active = [
      { workId: "RJ-QA-1", title: "A quiet night", phase: "downloading", bytesReceived: 128 * MiB,
        bytesTotal: 512 * MiB, bytesPerSecond: 8 * MiB, rateUpdatedAt: Date.now() },
      { workId: "RJ-QA-2", title: "Unknown size recording", phase: "downloading", bytesReceived: 16 * MiB,
        bytesTotal: null, bytesPerSecond: 2 * MiB, rateUpdatedAt: Date.now() },
    ];
    const metrics = { totalWorks: 4, completedWorks: 1, succeededWorks: 1, failedWorks: 0,
      bytesReceived: 656 * MiB, activeWorks: active };
    await emit(page, { progressDetails: { download: metrics } });
    const overall = page.getByRole("region", { name: "Overall download progress", exact: true });
    await overall.getByText("1 / 4 works processed", { exact: true }).waitFor();
    await page.getByText("Up to 4 simultaneous works · 2 active", { exact: true }).waitFor();
    await overall.getByText("10.0 MiB/s", { exact: true }).waitFor();
    assert.equal(await overall.getByRole("progressbar", { name: "Overall work progress", exact: true }).getAttribute("value"), "25");
    assert.equal(await page.getByRole("progressbar", { name: "Batch work progress", exact: true }).getAttribute("value"), "25");
    assert.equal(await page.getByRole("progressbar", { name: "RJ-QA-1 file progress", exact: true }).getAttribute("value"), "25");
    assert.equal(await page.getByRole("progressbar", { name: "RJ-QA-2 file progress", exact: true }).getAttribute("value"), null);
    await page.getByText("16.0 MiB downloaded · total size unknown", { exact: true }).waitFor();
    await page.screenshot({ path: path.join(captures, "downloads.png") });
    await page.locator(".download-panel .download-queue-row button").first().click();
    await page.getByRole("dialog", { name: "Download details", exact: true }).waitFor();
    await page.keyboard.press("Escape");
    await page.getByRole("dialog", { name: "Download details", exact: true }).waitFor({ state: "hidden" });
    assert.equal(await page.locator(".download-panel").isVisible(), true);
    // Stalls expire without another IPC event or clicking Reload.
    await page.getByLabel("RJ-QA-1 download speed", { exact: true }).getByText("0 B/s", { exact: true }).waitFor();
    active[0] = { ...active[0], bytesReceived: 256 * MiB, bytesPerSecond: 4 * MiB, rateUpdatedAt: Date.now() };
    active[1] = { ...active[1], phase: "unpacking", bytesPerSecond: 0, rateUpdatedAt: Date.now() };
    await page.locator(".download-panel header button").click();
    assert.equal(await page.locator(".download-panel").count(), 0);
    await emit(page, { progressDetails: { download: { ...metrics, bytesReceived: 784 * MiB, activeWorks: active } } });
    await page.locator(".download-status").getByText("4.00 MiB/s", { exact: true }).waitFor();
    await page.locator(".download-status").click();
    await overall.getByText("4.00 MiB/s", { exact: true }).waitFor();
    assert.equal(await page.getByRole("progressbar", { name: "RJ-QA-1 file progress", exact: true }).getAttribute("value"), "50");
    await page.getByText("Extracting archive", { exact: true }).waitFor();
    await page.setViewportSize({ width: 390, height: 844 });
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "narrow view must not overflow");
    await page.screenshot({ path: path.join(captures, "downloads-narrow.png"), fullPage: true });
    await page.locator(".download-panel header button").click();
    await page.setViewportSize({ width: 1280, height: 960 });
    await page.locator('.list-header').getByRole('button', {name:'Next', exact:true}).click();
    await page.getByText('Page 2 of 8', {exact:true}).first().waitFor();
    // A slow completion refresh must never unmount the gallery or clamp its scroll.
    await page.evaluate(() => {
      const qa = window.__progressQa;
      const main = document.querySelector('main');
      qa.grid = document.querySelector('.product-table');
      qa.cards = [...qa.grid.querySelectorAll('.product-card')];
      qa.images = [...qa.grid.querySelectorAll('img')];
      qa.completedWorkId = qa.cards.find(card => !card.querySelector('.work-folder')).querySelector('.work-id').textContent;
      qa.focus = qa.cards[20].querySelector('input');
      qa.focus.focus({preventScroll:true});
      main.scrollTop = 900;
      qa.scrollTop = main.scrollTop;
      qa.minScrollTop = main.scrollTop;
      main.addEventListener('scroll', () => {qa.minScrollTop = Math.min(qa.minScrollTop, main.scrollTop);});
      qa.removedGrids = 0;
      qa.observer = new MutationObserver(records => {
        for (const record of records) for (const node of record.removedNodes)
          if (node === qa.grid || node.contains?.(qa.grid)) qa.removedGrids++;
      });
      qa.observer.observe(main, {childList:true, subtree:true});
      qa.pendingReads = [];
      qa.holdReads = true;
    });
    await emit(page, { status: "succeeded", phase: "completed", cancellable: false,
      progress: { current: 4, total: 4, unit: "items" },
      progressDetails: { download: { ...metrics, completedWorks: 4, succeededWorks: 4, activeWorks: [] } },
      finishedAt: new Date().toISOString() });
    await page.waitForFunction(() => window.__progressQa.pendingReads.length > 0);
    const stable = () => page.evaluate(() => {
      const qa = window.__progressQa;
      return {cards: document.querySelectorAll('.product-card').length,
        sameGrid: document.querySelector('.product-table') === qa.grid,
        sameCards: qa.cards.every((card, i) => card === qa.grid.querySelectorAll('.product-card')[i]),
        sameImages: qa.images.every((image, i) => image === qa.grid.querySelectorAll('img')[i]),
        scrollDifference: Math.abs(document.querySelector('main').scrollTop - qa.scrollTop),
        minScrollDifference: Math.abs(qa.minScrollTop - qa.scrollTop),
        sameFocus: document.activeElement === qa.focus, removedGrids: qa.removedGrids};
    });
    const expected = {cards:30, sameGrid:true, sameCards:true, sameImages:true,
      scrollDifference:0, minScrollDifference:0, sameFocus:true, removedGrids:0};
    assert.deepEqual(await stable(), expected, 'gallery stays mounted during the delayed refresh');
    await page.evaluate(() => {
      const qa = window.__progressQa; qa.holdReads = false;
      qa.pendingReads.splice(0).forEach(resolve => resolve());
    });
    await page.waitForFunction(() => [...document.querySelectorAll('.product-card')].some(card =>
      card.querySelector('.work-id')?.textContent === window.__progressQa.completedWorkId && card.querySelector('.work-folder')));
    assert.deepEqual(await stable(), expected, 'completion updates the folder action in place');
    await page.evaluate(() => window.__progressQa.observer.disconnect());
    await page.screenshot({path:path.join(captures, 'completed-without-scroll-reset.png')});
    await page.locator(".download-status").click();
    await overall.getByText("4 / 4 works processed", { exact: true }).waitFor();
    assert.equal(await overall.getByRole("progressbar", { name: "Overall work progress", exact: true }).getAttribute("value"), "100");
    await emit(page, { id: "qa-next", kind: "workDownload", status: "queued", progressDetails: undefined,
      progress: null, metadata: { workId: "RJ-QA-NEXT" } });
    await overall.getByText("0 / 1 works processed", { exact: true }).waitFor();
    assert.deepEqual(errors, []);
    const calls = await page.evaluate(() => window.__progressQa.calls);
    assert.ok(!calls.some((command) => /start_.*download|sync_account|save_account/.test(command)));
    console.log(`Download progress QA passed; screenshots: ${captures}`);
  } catch (error) {
    if (page) {
      await page.screenshot({ path: path.join(captures, "failure.png"), fullPage: true }).catch(() => {});
      console.error(await page.locator("body").innerText().catch(() => ""));
    }
    console.error(`Screenshots: ${captures}`);
    throw error;
  } finally { await browser.close(); }
})().catch((error) => { console.error(error); process.exitCode = 1; });

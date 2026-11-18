// Start `pnpm storybook --ci`, install Playwright in a disposable directory, then:
// PLAYWRIGHT_MODULE_PATH=/path/to/playwright node tools/qa/name-language-settings.cjs
// Uses only the production Storybook route's isolated IPC fixtures.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { chromium } = require(
  process.env.PLAYWRIGHT_MODULE_PATH || "playwright",
);
const base = process.env.STORYBOOK_URL || "http://localhost:6006";
const appUrl = `${base}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const settingsKey = "dldr-production-story-settings";
const captures = fs.mkdtempSync(
  path.join(os.tmpdir(), "dlsite-name-settings-"),
);

async function newApp(browser, settings) {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
  });
  const page = await context.newPage();
  if (settings)
    await page.addInitScript(
      ({ key, value }) => {
        if (!sessionStorage.getItem(key))
          sessionStorage.setItem(key, JSON.stringify(value));
      },
      { key: settingsKey, value: settings },
    );
  await page.goto(appUrl);
  await page.getByText("Page 1 of 8", { exact: true }).first().waitFor();
  await page.evaluate(() => {
    const qa = (window.__nameQa = {
      lists: [],
      saves: [],
    });
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async function (
      command,
      args,
      ...rest
    ) {
      if (command === "list_products")
        qa.lists.push(JSON.parse(JSON.stringify(args.request)));
      if (command === "save_settings") {
        qa.saves.push(JSON.parse(JSON.stringify(args.settings)));
        if (qa.pauseSave) {
          qa.pauseSave = false;
          await new Promise((resolve) => {
            qa.releaseSave = resolve;
          });
        }
      }
      if (command === "list_name_languages")
        throw new Error("Fixed settings must not request language discovery");
      const holdList = command === "list_products" && qa.holdNextList;
      const holdDetail = command === "get_product_detail" && qa.holdNextDetail;
      if (holdList) qa.holdNextList = false;
      if (holdDetail) qa.holdNextDetail = false;
      const result = await invoke.call(this, command, args, ...rest);
      if (holdList)
        return new Promise((resolve) => {
          qa.heldList = true;
          qa.releaseList = () => {
            qa.releasedList = true;
            resolve(result);
          };
        });
      if (holdDetail)
        return new Promise((resolve) => {
          qa.heldDetail = true;
          qa.releaseDetail = () => {
            qa.releasedDetail = true;
            resolve(result);
          };
        });
      return result;
    };
  });
  return { context, page };
}

const row = (page, code) =>
  page
    .getByRole("list", { name: "Name language priority" })
    .locator("li")
    .filter({ hasText: code });
const up = (page, code) =>
  row(page, code).getByRole("button", { name: / up$/ });
const down = (page, code) =>
  row(page, code).getByRole("button", { name: / down$/ });
async function names(page) {
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "General", exact: true }).click();
}
async function saved(page, count) {
  await page.waitForFunction(
    (count) => window.__nameQa.saves.length === count,
    count,
  );
  await page
    .getByRole("button", { name: "Save changes", exact: true })
    .waitFor();
  await page.waitForFunction(
    () =>
      ![...document.querySelectorAll("button")].find(
        (b) => b.textContent.trim() === "Save changes",
      )?.disabled,
  );
}

async function saveRefreshAndReload(browser) {
  const { context, page } = await newApp(browser);
  try {
    await page.locator(".list-header").getByRole("button", { name: "Next", exact: true }).click();
    await page.getByText("Page 2 of 8", { exact: true }).first().waitFor();
    await names(page);
    assert.equal(await page.locator(".language-priority-row").count(), 5);
    assert.equal(await page.locator(".language-priority-row code").count(), 4);
    assert.equal(await page.locator(".language-priority-row").filter({hasText:"pt_BR"}).count(),0);
    await up(page, "ja_JP").focus();
    await page.keyboard.press("Enter");
    assert.match(
      await page
        .getByRole("list", { name: "Name language priority" })
        .locator("li")
        .first()
        .innerText(),
      /ja_JP/,
    );
    await page.getByRole("button", { name: "Download settings", exact: true }).click();
    await page
      .getByLabel("Library folder", { exact: true })
      .fill("/tmp/changed-library");
    await page.getByRole("button", { name: "General", exact: true }).click();
    await page.evaluate(() => {
      window.__nameQa.pauseSave = true;
    });
    await page
      .getByRole("button", { name: "Save changes", exact: true })
      .click();
    await page.getByRole("button", { name: "Saving…", exact: true }).waitFor();
    assert(await up(page, "en_US").isDisabled());
    assert(
      await page
        .getByRole("button", { name: "Saving…", exact: true })
        .isDisabled(),
    );
    await page.evaluate(() => window.__nameQa.releaseSave());
    await saved(page, 1);
    const patch = await page.evaluate(() => window.__nameQa.saves[0]);
    assert.equal(patch.languagePriority[0], "ja_JP");
    assert.equal(patch.libraryRoot, "/tmp/changed-library");
    assert.equal(
      await page.evaluate(() => window.__nameQa.lists.at(-1).offset),
      0,
    );
    await page.getByRole("button", { name: "Library", exact: true }).click();
    await page.getByText("Page 1 of 8", { exact: true }).first().waitFor();
    assert.match(
      await page
        .getByRole("button", { name: /^Details: / })
        .first()
        .getAttribute("aria-label"),
      /静かな夜/,
    );
    await page.getByRole("button", { name: "Filters", exact: true }).click();
    await page
      .locator(".maker-filter")
      .getByRole("button", { name: /北窓スタジオ/ })
      .click();
    await page.getByRole("button", { name: "Title", exact: true }).click();
    for (const search of ["Observatory", "조용한", "静夜"]) {
      await page.getByLabel("Search library", { exact: true }).fill(search);
      await page.getByRole("button", { name: "Search", exact: true }).click();
      await page.waitForFunction(
        (search) => window.__nameQa.lists.at(-1).search === search,
        search,
      );
      await page
        .getByRole("button", { name: "Reload", exact: true })
        .waitFor({ state: "visible" });
      await page.waitForFunction(
        () =>
          ![...document.querySelectorAll("button")].find(
            (b) => b.textContent.trim() === "Reload",
          )?.disabled,
      );
      assert(
        (await page.getByRole("button", { name: /^Details: / }).count()) > 0,
      );
    }
    const selected = await page.evaluate(() => window.__nameQa.lists.at(-1));
    await names(page);
    await up(page, "zh_CN").click();
    await page
      .getByRole("button", { name: "Save changes", exact: true })
      .click();
    await saved(page, 2);
    const refreshed = await page.evaluate(() => window.__nameQa.lists.at(-1));
    assert.deepEqual(refreshed.makerKeys, selected.makerKeys);
    assert.equal(refreshed.search, selected.search);
    assert.equal(refreshed.sort, "titleAsc");
    await page.reload();
    await page.getByRole("heading", { name: "Library", exact: true }).waitFor();
    await names(page);
    assert.match(
      await page
        .getByRole("list", { name: "Name language priority" })
        .locator("li")
        .first()
        .innerText(),
      /ja_JP/,
    );
    await page.getByRole("button", { name: "Library", exact: true }).click();
    await page.getByRole("button", { name: "Filters", exact: true }).click();
    await page.getByRole("button", { name: "Local Only", exact: true }).click();
    await page.locator(".card-credits dd").getByText("Not provided", { exact: true }).first().waitFor();
    assert.doesNotMatch(
      await page.getByRole("button", { name: /^Details: / }).first().getAttribute("aria-label"),
      /静かな夜/,
    );
    await names(page);
    await page
      .getByRole("button", { name: "About & updates", exact: true })
      .click();
    await page.getByText("Language name data", { exact: true }).click();
    assert.match(
      await page.locator("main pre").innerText(),
      /UNICODE LICENSE V3/,
    );
    console.log(
      "PASS: keyboard reorder, shared tab draft, pending-save guard, page reset, all-language search, retained filters, reload and license.",
    );
  } finally {
    await context.close();
  }
}

async function legacyUndoFailure(browser) {
  const {context,page}=await newApp(browser, {libraryRoot:"/tmp/original",downloadRoot:"/tmp/downloads",languagePriority:["ko_KR","ja_JP","pt_BR"]});
  try {
    await names(page);
    assert.equal(await page.locator('.language-priority-row').count(),5);
    assert.deepEqual(await page.locator('.language-priority-row code').allTextContents(),['ja_JP','zh_TW','zh_CN','en_US']);
    assert.equal(await page.locator('.language-priority-row').last().getByRole('button').count(),0);
    await up(page,'en_US').click();await down(page,'en_US').click();
    await page.getByRole('button',{name:'Download settings',exact:true}).click();
    await page.getByLabel('Library folder',{exact:true}).fill('/tmp/path-only');
    await page.getByRole('button',{name:'General',exact:true}).click();
    await page.getByRole('button',{name:'Save changes',exact:true}).click();await saved(page,1);
    assert.equal(await page.evaluate(()=>Object.hasOwn(window.__nameQa.saves[0],'languagePriority')),false);
    await page.getByRole('button',{name:'Download settings',exact:true}).click();
    await page.getByLabel('Library folder',{exact:true}).fill('/fail-save');
    await page.getByRole('button',{name:'General',exact:true}).click();
    await up(page,'en_US').click();
    const failedDraft=await page.locator('.language-priority-row code').allTextContents();
    await page.getByRole('button',{name:'Save changes',exact:true}).click();
    await page.getByText(/Could not save settings \(simulated\)/).waitFor();
    assert.deepEqual(await page.locator('.language-priority-row code').allTextContents(),failedDraft);
    await page.getByRole('button',{name:'Revert changes',exact:true}).click();
    assert.match(await page.locator('.language-priority-row').first().innerText(),/ja_JP/);
    console.log('PASS: legacy extra languages collapse to four choices plus Other, undo omits unchanged priority, failed saves retain draft, revert restores saved values.');
  } finally { await context.close(); }
}

async function lateResponses(browser) {
  const { context, page } = await newApp(browser);
  try {
    await page.evaluate(() => {
      window.__nameQa.holdNextList = true;
      window.__nameQa.holdNextDetail = true;
    });
    await page
      .getByRole("button", { name: /^Details: / })
      .first()
      .click();
    await page.waitForFunction(() => window.__nameQa.heldDetail);
    await page.getByRole("button", { name: "Reload", exact: true }).click();
    await page.waitForFunction(() => window.__nameQa.heldList);
    await names(page);
    await up(page, "ja_JP").click();
    await page
      .getByRole("button", { name: "Save changes", exact: true })
      .click();
    const detail = page.getByRole("dialog", {
      name: "Work details",
      exact: true,
    });
    await detail.waitFor();
    const title = await detail.locator(".copy-title").innerText();
    assert.match(title, /静かな夜/);
    await page.evaluate(() => {
      window.__nameQa.releaseList();
      window.__nameQa.releaseDetail();
    });
    await page.evaluate(() => new Promise(requestAnimationFrame));
    assert.equal(await detail.locator(".copy-title").innerText(), title);
    await detail
      .getByRole("button", { name: "Close Work details", exact: true })
      .click();
    await page.getByRole("button", { name: "Library", exact: true }).click();
    assert.match(
      await page
        .getByRole("button", { name: /^Details: / })
        .first()
        .getAttribute("aria-label"),
      /静かな夜/,
    );
    console.log(
      "PASS: old list and detail responses delivered after successful priority save cannot overwrite the new names.",
    );
  } finally {
    await context.close();
  }
}

async function narrowAndBusy(browser) {
  const context = await browser.newContext({
    viewport: { width: 320, height: 850 },
  });
  try {
    const page = await context.newPage();
    await page.goto(
      `${base}/iframe.html?id=settings-view--name-languages&viewMode=story`,
    );
    await page.getByRole("list", { name: "Name language priority" }).waitFor();
    assert(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    );
    await up(page, "en_US").focus();
    await page.keyboard.press("Enter");
    await page.screenshot({
      path: path.join(captures, "names-narrow.png"),
      fullPage: true,
    });
    for (const story of ["saving-languages", "loading-languages"]) {
      await page.goto(
        `${base}/iframe.html?id=settings-view--${story}&viewMode=story`,
      );
      await page
        .getByRole("list", { name: "Name language priority" })
        .waitFor();
      for (const button of await page
        .getByRole("list", { name: "Name language priority" })
        .getByRole("button")
        .all())
        assert(await button.isDisabled());
    }
    console.log(
      `PASS: 320px fixed-language layout, keyboard controls, and disabled loading/saving states. Screenshot: ${captures}`,
    );
  } finally {
    await context.close();
  }
}

(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE_PATH });
  try {
    await saveRefreshAndReload(browser);
    await legacyUndoFailure(browser);
    await lateResponses(browser);
    await narrowAndBusy(browser);
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

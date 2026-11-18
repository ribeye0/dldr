// Production UI against isolated native fixtures; no account access or downloads.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || "playwright");
const url = `${process.env.STORYBOOK_URL || "http://localhost:6006"}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(), "dlsite-rate-qa-"));

async function hook(page) {
  await page.evaluate(() => {
    const qa = window.__rateQa = { rates: [], calls: [], failGet: false, failSet: false };
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = function(command, args, ...rest) {
      qa.calls.push(command);
      if (command === "get_download_rate_limit" && qa.failGet) return Promise.reject(new Error("Rate setting unavailable (simulated)"));
      if (command === "set_download_rate_limit") {
        qa.rates.push(args.request.bytesPerSecond);
        if (qa.failSet) return Promise.reject(new Error("Rate setting unavailable (simulated)"));
      }
      return invoke.call(this, command, args, ...rest);
    };
  });
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
    await hook(page);
    assert.ok(await page.getByLabel("Primary", { exact: true }).evaluate((element) => element.scrollWidth <= element.clientWidth), "sidebar controls must stay within the column");
    const downloads = async () => {
      await page.locator("aside nav").getByRole("button", { name: "Settings", exact: true }).click();
      await page.locator(".settings-layout > .page-header .tabs").getByRole("button", { name: "Download settings", exact: true }).click();
    };
    await downloads();
    const form = page.getByRole("form", { name: "Download speed limit", exact: true });
    const input = form.getByRole("spinbutton", { name: "Total download limit (MiB/s)", exact: true });
    const apply = form.getByRole("button", { name: "Apply limit", exact: true });
    await form.getByText("Current limit: Unlimited", { exact: true }).waitFor();
    await input.fill("0.5"); await apply.click();
    await form.getByText("Current limit: 512 KiB/s", { exact: true }).waitFor();
    assert.deepEqual(await page.evaluate(() => window.__rateQa.rates), [524288]);
    await input.fill("5"); await apply.click();
    await form.getByText("Current limit: 5.00 MiB/s", { exact: true }).waitFor();
    await page.reload();
    await page.getByText("Page 1 of 8", { exact: true }).first().waitFor();
    await hook(page); await downloads();
    await form.getByText("Current limit: 5.00 MiB/s", { exact: true }).waitFor();
    assert.equal(await input.inputValue(), "5");
    await page.evaluate(() => window.__rateQa.failSet = true);
    await input.fill("2"); await apply.click();
    await form.getByRole("alert").getByText("Rate setting unavailable (simulated)", { exact: true }).waitFor();
    assert.ok(await form.getByText("Current limit: 5.00 MiB/s", { exact: true }).isVisible());
    await page.evaluate(() => window.__rateQa.failSet = false);
    await input.fill("0.0001"); await apply.click();
    await form.getByRole("alert").getByText(/minimum limit is 1 KiB\/s/).waitFor();
    assert.deepEqual(await page.evaluate(() => window.__rateQa.rates), [2097152]);
    await input.fill("0"); await apply.click();
    await form.getByText("Current limit: Unlimited", { exact: true }).waitFor();
    assert.deepEqual(await page.evaluate(() => window.__rateQa.rates), [2097152, null]);
    await page.screenshot({ path: path.join(captures, "desktop.png") });
    await page.getByLabel("Primary", { exact: true }).getByRole("button", { name: "Library", exact: true }).click();
    await page.evaluate(() => window.__rateQa.failGet = true);
    await downloads();
    await form.getByText("Current limit unavailable", { exact: true }).waitFor();
    assert.ok(await apply.isDisabled());
    await page.evaluate(() => window.__rateQa.failGet = false);
    await form.getByRole("button", { name: "Retry rate limit", exact: true }).click();
    await form.getByText("Current limit: Unlimited", { exact: true }).waitFor();
    await page.setViewportSize({ width: 390, height: 844 });
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    assert.ok(await input.isVisible());
    await page.screenshot({ path: path.join(captures, "narrow.png"), fullPage: true });
    assert.deepEqual(errors, []);
    const calls = await page.evaluate(() => window.__rateQa.calls);
    assert.ok(!calls.some((command) => /start_.*download|sync_account|save_account/.test(command)));
    console.log(`Download rate UI QA passed; screenshots: ${captures}`);
  } catch (error) {
    if (page) await page.screenshot({ path: path.join(captures, "failure.png"), fullPage: true }).catch(() => {});
    console.error(`Screenshots: ${captures}`);
    throw error;
  } finally { await browser.close(); }
})().catch((error) => { console.error(error); process.exitCode = 1; });

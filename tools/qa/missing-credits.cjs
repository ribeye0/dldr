// Production route, synthetic public-credit response; no account or downloads.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const url = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story&missingCredits=1`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(), 'dlsite-missing-credits-qa-'));
(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE_PATH });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    page.setDefaultTimeout(15000);
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(url);
    const card = page.locator('.product-card').filter({ has: page.locator('[aria-label="作品資訊：RJ01729125"]') });
    await card.waitFor();
    assert.match(await card.locator('.card-credits').innerText(), /讀取中/);
    await page.waitForFunction(() => document.querySelector('[aria-label="作品資訊：RJ01729125"]')?.textContent.includes('浅木式'));
    assert.deepEqual(await card.locator('.card-credits dt').allTextContents(), ['社團', '聲優', '販賣日']);
    assert.match(await card.locator('.card-credits dd').nth(1).innerText(), /伊倉える[\s\S]*浅木式/);
    assert.doesNotMatch(await card.locator('.product-tags').innerText(), /伊倉える|浅木式/);
    const checkbox = card.locator('input[type=checkbox]');
    if (await checkbox.isEnabled()) await checkbox.check();
    const selection = await page.evaluate(() => localStorage.getItem('dldr-selected-works-v1'));
    await card.getByRole('button', { name: /^詳細資訊：/ }).click();
    await page.getByRole('dialog').waitFor();
    assert.match(await page.locator('.identity-credits dd').nth(1).innerText(), /伊倉える[\s\S]*浅木式/);
    await page.keyboard.press('Escape');
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    await page.locator('aside nav button[data-view=downloaded]').click();
    await page.getByRole('heading', { name: '已下載', exact: true }).waitFor();
    await card.waitFor();
    assert.match(await card.locator('.card-credits dd').nth(1).innerText(), /伊倉える[\s\S]*浅木式/);
    await page.getByRole('button', { name: '重新載入', exact: true }).click();
    await card.waitFor();
    assert.match(await card.locator('.card-credits dd').nth(1).innerText(), /伊倉える[\s\S]*浅木式/);
    assert.equal(await page.evaluate(() => localStorage.getItem('dldr-selected-works-v1')), selection);
    await page.screenshot({ path: path.join(captures, 'downloaded.png') });
    await page.setViewportSize({ width: 390, height: 844 });
    await card.getByRole('button', { name: /^詳細資訊：/ }).click();
    await page.getByRole('dialog').waitFor();
    assert.match(await page.locator('.identity-credits dd').nth(1).innerText(), /伊倉える[\s\S]*浅木式/);
    await page.screenshot({ path: path.join(captures, 'narrow-detail.png') });
    assert.deepEqual(errors, []);
    console.log(`Missing public credits QA passed: ${captures}`);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exit(1); });

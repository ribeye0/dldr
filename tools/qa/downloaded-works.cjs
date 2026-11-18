// Production route + isolated native IPC fixtures; no account or real files touched.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const url = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(), 'dlsite-downloaded-qa-'));
const emptyQuery = { search: null, accountIds: [], typeGroups: [], ageCategories: [], sourceGroups: [], makerKeys: [], sort: 'latestPurchaseDesc', limit: 500, offset: 0 };
const cards = (page) => page.locator('.product-card');
const nav = (page, name) => page.getByRole('navigation', { name: 'Main', exact: true }).getByRole('button', { name, exact: true });
async function search(page, text) {
  await page.getByRole('combobox', { name: 'Search downloaded works' }).fill(text);
  await page.getByRole('button', { name: 'Search', exact: true }).click();
}
async function settled(page) {
  await page.waitForFunction(() => !document.querySelector('.empty-state')?.textContent.includes('Loading'));
}
(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE_PATH });
  let page;
  try {
    page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    page.setDefaultTimeout(10000);
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.goto(url);
    await page.getByText('Page 1 of 8', { exact: true }).first().waitFor();
    const master = await page.evaluate(async (request) => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const qa = window.__downloadedQa = { requests: [], fail: false, empty: false };
      window.__TAURI_INTERNALS__.invoke = async function(command, args, ...rest) {
        if (command === 'list_products' || command === 'list_product_filter_facets') {
          qa.requests.push({ command, request: JSON.parse(JSON.stringify(args.request)) });
          if (qa.fail && args.request.downloadedOnly) throw new Error('Downloaded query failed (simulated)');
          if (qa.empty && args.request.downloadedOnly) return command === 'list_products' ? { products: [], totalCount: 0 } : { makers: [] };
        }
        const result = await invoke.call(this, command, args, ...rest);
        if (qa.longCredits && command === 'list_products' && args.request.downloadedOnly) {
          result.products[0] = { ...result.products[0], makerName: '長いサークル名'.repeat(30), creditGroups: [{ kind: 'voice', label: 'CV', names: ['とても長い声優名'.repeat(30)] }] };
        }
        return result;
      };
      return invoke('list_products', { request });
    }, emptyQuery);
    const expected = master.products.filter((p) => p.download.status === 'downloaded' && p.download.localPath?.trim());
    assert.ok(expected.length > 1 && expected.length < master.totalCount);
    await page.locator('.product-card input[type=checkbox]:enabled').first().check();
    const selected = await page.evaluate(() => localStorage.getItem('dldr-selected-works-v1'));
    await page.locator(".list-header").getByRole('button', { name: 'Next', exact: true }).click();
    await page.getByText('Page 2 of 8', { exact: true }).first().waitFor();
    await nav(page, 'Downloaded').click();
    await page.getByRole('heading', { name: 'Downloaded', exact: true }).waitFor();
    await settled(page);
    assert.equal(await cards(page).count(), expected.length);
    assert.equal(await page.locator('.product-card input[type=checkbox]').count(), 0);
    assert.equal(await page.getByRole('button', { name: 'Download Results', exact: true }).count(), 0);
    assert.equal(await page.getByRole('button', { name: 'Sync', exact: true }).count(), 0);
    const titles = await cards(page).evaluateAll((items) => items.map((card) => card.querySelector('button[aria-label^="Details:"]').getAttribute('aria-label').replace('Details: ', '')).sort());
    assert.deepEqual(titles, expected.map((p) => p.title).sort());
    // Creator information is separate from genre chips on both galleries.
    const firstTitle = await cards(page).first().getByRole('button', { name: /^Details:/ }).getAttribute('aria-label');
    const firstWork = expected.find((p) => `Details: ${p.title}` === firstTitle);
    assert.deepEqual(await cards(page).first().locator('.card-credits dt').allTextContents(), ['Circle', 'Voice actor', 'Released']);
    assert.equal(await cards(page).first().locator('.card-credits dd').first().textContent(), firstWork.makerName);
    assert.deepEqual(await cards(page).first().locator('.voice-chip').allTextContents(), firstWork.creditGroups.filter((group) => group.kind === 'voice').flatMap((group) => group.names));
    assert.match(firstWork.makerName, /[\u3040-\u30ff\u4e00-\u9fff]/);
    assert.match(await cards(page).first().locator('.card-credits dd').nth(1).innerText(), /あかり/);
    assert.ok(await page.locator('.card-credits dd').filter({ hasText: /^Not provided$/ }).count() > 0);
    assert.equal(await cards(page).first().locator('.product-tags').getByText(firstWork.makerName, { exact: true }).count(), 0);
    assert.ok(await page.getByRole('button', { name: /^Open downloaded folder for / }).count() > 0);
    assert.ok(await page.getByRole('button', { name: 'Search tag: Ear Cleaning', exact: true }).count() > 0);
    assert.equal(await page.getByRole('button', { name: 'Search tag: Long-form listening', exact: true }).count(), 0);
    assert.equal(await page.getByRole('button', { name: 'Search tag: Voice Person', exact: true }).count(), 0);
    assert.ok(await page.getByRole('button', { name: 'Search tag: Whispering', exact: true }).count() > 0);
    assert.ok(await page.getByRole('button', { name: 'Search tag: Ear Licking', exact: true }).count() > 0);
    const tagPosition = await cards(page).first().evaluate((card) => ({ tag: card.querySelector('.product-tags').getBoundingClientRect().top, cover: card.querySelector('.cover-frame').getBoundingClientRect().bottom }));
    assert.ok(tagPosition.tag > tagPosition.cover);
    await page.screenshot({ path: path.join(captures, 'desktop.png') });

    // Clicking the cover still opens details, including the existing local-folder action.
    await cards(page).first().getByRole('button', { name: /^Details:/ }).click();
    await page.getByRole('dialog').waitFor();
    assert.deepEqual(await page.locator('.identity-credits dt').allTextContents(), ['Circle', 'Voice actor']);
    assert.equal(await page.locator('.identity-credits dd').first().innerText(), firstWork.makerName);
    assert.deepEqual(await page.locator('.identity-credits .voice-chip').allTextContents(), firstWork.creditGroups.filter((group) => group.kind === 'voice').flatMap((group) => group.names));
    const creditRows = await page.locator('.identity-credits > div').evaluateAll((rows) => rows.map((row) => ({ top: row.getBoundingClientRect().top, bottom: row.getBoundingClientRect().bottom })));
    assert.ok(creditRows[1].top >= creditRows[0].bottom, 'circle and voice actor occupy distinct rows');
    assert.equal(await page.locator('.credits-grid dt').getByText('Circle', { exact: true }).count(), 0);
    await page.screenshot({ path: path.join(captures, 'details.png') });
    await page.keyboard.press('Escape');
    await page.getByRole('dialog').waitFor({ state: 'hidden' });

    await page.getByRole('button', { name: 'Search tag: Ear Cleaning', exact: true }).first().click();
    await settled(page);
    assert.equal(await page.getByRole('combobox', { name: 'Search downloaded works' }).inputValue(), '$tag:Ear Cleaning$');
    assert.equal(await cards(page).count(), expected.filter((p) => p.tags?.some((tag) => tag.name === '掏耳朵')).length);
    const queries = await page.evaluate(() => window.__downloadedQa.requests);
    assert.ok(queries.some(({ command, request }) => command === 'list_products' && request.downloadedOnly && request.search === '$tag:Ear Cleaning$' && request.offset === 0));
    assert.ok(queries.some(({ command, request }) => command === 'list_product_filter_facets' && request.downloadedOnly && request.search === '$tag:Ear Cleaning$'));

    // Queries/pages belong to each gallery; the saved download selection is unaffected.
    await nav(page, 'Library').click();
    await page.getByText('Page 2 of 8', { exact: true }).first().waitFor();
    assert.deepEqual(await cards(page).first().locator('.card-credits dt').allTextContents(), ['Circle', 'Voice actor', 'Released']);
    assert.equal(await page.getByRole('combobox', { name: 'Search library' }).inputValue(), '');
    assert.equal(await page.evaluate(() => localStorage.getItem('dldr-selected-works-v1')), selected);
    await page.getByRole('button', { name: 'Search tag: Ear Cleaning', exact: true }).first().click();
    await settled(page);
    assert.equal(await page.getByRole('combobox', { name: 'Search library' }).inputValue(), '$tag:Ear Cleaning$');
    assert.ok(await cards(page).count() > expected.length);
    await nav(page, 'Downloaded').click();
    await settled(page);
    assert.equal(await page.getByRole('combobox', { name: 'Search downloaded works' }).inputValue(), '$tag:Ear Cleaning$');
    await search(page, '舔耳');
    await settled(page);
    assert.equal(await cards(page).count(), expected.filter((p) => p.tags?.some((tag) => tag.class === 'genre' && tag.name === '舔耳')).length);
    await search(page, 'nonexistent-tag-qa');
    await page.getByText(/^No downloaded works match/).waitFor();
    assert.equal(await cards(page).count(), 0);
    await page.getByRole('button', { name: 'Reset filters', exact: true }).click();
    await settled(page);
    assert.equal(await cards(page).count(), expected.length);

    // Empty library and failed refresh remain scoped; retry restores the cover list.
    await page.evaluate(() => { window.__downloadedQa.empty = true; });
    await page.getByRole('button', { name: 'Reload', exact: true }).click();
    await page.getByText(/^No downloaded works match/).waitFor();
    await page.evaluate(() => { window.__downloadedQa.empty = false; window.__downloadedQa.fail = true; });
    await page.getByRole('button', { name: 'Reload', exact: true }).click();
    await page.getByText('Downloaded query failed (simulated)', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Dismiss notification', exact: true }).click();
    await page.evaluate(() => { window.__downloadedQa.fail = false; });
    await page.getByRole('button', { name: 'Reload', exact: true }).click();
    await settled(page);
    assert.equal(await cards(page).count(), expected.length);

    // Re-query after returning from the queue; narrow navigation keeps all six views usable.
    await nav(page, 'Settings').click();
    await page.locator('.settings-layout > .page-header .tabs').getByRole('button', { name: 'Accounts', exact: true }).click();
    await nav(page, 'Downloaded').click();
    await settled(page);
    const before = (await page.evaluate(() => window.__downloadedQa.requests)).length;
    await page.evaluate(async () => {
      const jobs = await window.__TAURI_INTERNALS__.invoke('list_jobs');
      const snapshot = { ...jobs[0], status: 'succeeded', kind: 'workDownload' };
      await window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'dldr-job-event', payload: { sequence: 10000, eventKind: 'updated', jobId: snapshot.id, kind: snapshot.kind, status: snapshot.status, phase: snapshot.phase, progress: snapshot.progress, message: null, log: null, snapshot } });
    });
    await page.waitForFunction((before) => window.__downloadedQa.requests.length > before, before);
    assert.ok((await page.evaluate(() => window.__downloadedQa.requests)).slice(before).every(({ request }) => request.downloadedOnly));
    await page.setViewportSize({ width: 390, height: 844 });
    await settled(page);
    await page.screenshot({ path: path.join(captures, 'narrow.png') });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    assert.equal(await page.locator('main').evaluate((el) => el.scrollWidth <= el.clientWidth), true);
    await cards(page).first().getByRole('button', { name: /^Details:/ }).click();
    await page.getByRole('dialog').waitFor();
    assert.equal(await page.getByRole('dialog').evaluate((el) => el.scrollWidth <= el.clientWidth), true);
    await page.screenshot({ path: path.join(captures, 'narrow-details.png') });
    await page.keyboard.press('Escape');
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    await nav(page, 'Library').click();
    await settled(page);
    await nav(page, 'Downloaded').click();
    await settled(page);
    await page.evaluate(() => { window.__downloadedQa.longCredits = true; });
    await page.getByRole('button', { name: 'Reload', exact: true }).click();
    await settled(page);
    assert.equal(await page.locator('main').evaluate((el) => el.scrollWidth <= el.clientWidth), true, 'long circle/voice names must wrap');
    assert.deepEqual(errors, []);
    console.log(JSON.stringify({ ok: true, downloaded: expected.length, captures }));
  } catch (error) {
    if (page) await page.screenshot({ path: path.join(captures, 'failure.png') });
    console.error(captures);
    throw error;
  } finally { await browser.close(); }
})().catch((error) => { console.error(error); process.exitCode = 1; });

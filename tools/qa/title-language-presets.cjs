// Production UI with isolated IPC fixtures. No real profile, login or purchased media.
const assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const url = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(), 'dldr-title-language-'));
const presets = {
  'zh-TW': ['zh_TW','zh_CN','ja_JP','en_US'], 'zh-CN': ['zh_CN','zh_TW','ja_JP','en_US'],
  ja: ['ja_JP','zh_TW','zh_CN','en_US'], en: ['en_US','ja_JP','zh_TW','zh_CN'],
};
const codes = p => p.locator('.title-language [data-title-language]').evaluateAll(rows => rows.map(row => row.dataset.titleLanguage));
const custom = p => p.locator('input[name="title-language-mode"]').nth(1);
const follow = p => p.locator('input[name="title-language-mode"]').nth(0);
const row = (p, code) => p.locator(`.language-priority-row[data-title-language="${code}"]`);
async function settings(p) {
  await p.locator('aside nav button[data-view="settings"]').click();
  await p.locator('.settings-layout > .page-header .tabs button').first().click();
  await p.locator('.title-language').waitFor();
}
async function save(p, expected) {
  const form = p.locator('form').filter({has: p.locator('#ui-locale')});
  await form.locator('button[type=submit]').click();
  await p.waitForFunction(async expected => {
    const value = await window.__TAURI_INTERNALS__.invoke('get_settings');
    return Object.entries(expected).every(([key, item]) => JSON.stringify(value[key]) === JSON.stringify(item));
  }, expected);
  await p.waitForFunction(() => !document.querySelector('#ui-locale').closest('form').querySelector('button[type=submit]').disabled);
}
(async () => {
  const browser = await chromium.launch({headless:true, executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
  try {
    const page = await browser.newPage({viewport:{width:1280,height:960}});
    page.setDefaultTimeout(12000);
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(() => {
      const key = 'dldr-production-story-settings';
      if (!sessionStorage.getItem(key)) sessionStorage.setItem(key, JSON.stringify({uiLocale:'zh-TW',languagePriority:['zh_TW','zh_CN','ja_JP','en_US']}));
    });
    await page.goto(url); await page.locator('.product-card').first().waitFor();
    await settings(page);
    for (const locale of ['zh-TW','zh-CN','ja','en']) {
      await page.locator('#ui-locale').selectOption(locale);
      assert(await follow(page).isChecked());
      assert.deepEqual(await codes(page), presets[locale]);
      assert.equal(await page.locator('.language-priority-row').count(), 0);
      assert.doesNotMatch(await page.locator('.title-language').innerText(), /zh_TW|ja_JP|ko_KR/);
      await save(page, {uiLocale:locale,titleLanguageFollowsUi:true,languagePriority:presets[locale]});
    }
    // Custom means custom even if the order currently equals the locale preset.
    await custom(page).check(); await save(page,{titleLanguageFollowsUi:false,languagePriority:presets.en});
    await page.locator('#ui-locale').selectOption('zh-CN');
    assert.deepEqual(await codes(page),presets.en);
    await save(page,{uiLocale:'zh-CN',titleLanguageFollowsUi:false,languagePriority:presets.en});
    await page.reload(); await page.locator('.product-card').first().waitFor(); await settings(page);
    assert(await custom(page).isChecked()); assert.deepEqual(await codes(page),presets.en);
    // Arrow controls stay focused on the language being moved.
    await row(page,'ja_JP').locator('[data-direction="-1"]').focus(); await page.keyboard.press('Enter');
    assert.equal((await codes(page))[0],'ja_JP');
    assert.equal(await page.evaluate(() => document.activeElement.closest('[data-title-language]').dataset.titleLanguage),'ja_JP');
    // Drag a language across multiple rows.
    const source = await row(page,'zh_CN').locator('.drag-handle').boundingBox();
    const target = await row(page,'ja_JP').boundingBox();
    await page.mouse.move(source.x+source.width/2,source.y+source.height/2); await page.mouse.down();
    await page.mouse.move(target.x+target.width/2,target.y+target.height/2,{steps:14}); await page.mouse.up();
    const moved = await codes(page); assert.equal(moved[0],'zh_CN');
    await save(page,{titleLanguageFollowsUi:false,languagePriority:moved});
    await page.reload(); await page.locator('.product-card').first().waitFor(); await settings(page);
    assert.deepEqual(await codes(page),moved);
    // A canceled drag restores the order before the gesture.
    const from = await row(page,moved[0]).locator('.drag-handle').boundingBox();
    const to = await row(page,moved[3]).boundingBox();
    await page.mouse.move(from.x+10,from.y+10); await page.mouse.down();
    await page.mouse.move(to.x+to.width/2,to.y+to.height/2,{steps:10});
    await page.keyboard.press('Escape'); await page.mouse.up(); assert.deepEqual(await codes(page),moved);
    for (const theme of ['light','dark']) {
      await page.locator('#ui-theme').selectOption(theme);
      await page.screenshot({path:path.join(captures,`custom-${theme}.png`),fullPage:true});
    }
    await page.setViewportSize({width:390,height:844});
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth>innerWidth),false);
    await page.screenshot({path:path.join(captures,'custom-narrow.png'),fullPage:true});
    await page.locator('.custom-heading button').click(); assert(await follow(page).isChecked());
    assert.deepEqual(await codes(page),presets['zh-CN']);
    await save(page,{titleLanguageFollowsUi:true,languagePriority:presets['zh-CN']});
    await page.reload(); await page.locator('.product-card').first().waitFor(); await settings(page);
    assert(await follow(page).isChecked()); assert.equal(await page.locator('.language-priority-row').count(),0);
    await page.setViewportSize({width:1280,height:960}); await page.locator('#ui-theme').selectOption('light');
    await page.screenshot({path:path.join(captures,'follow-interface.png'),fullPage:true});
    assert.deepEqual(errors,[]);
    console.log(JSON.stringify({passed:true,captures}));
  } finally { await browser.close(); }
})().catch(error => {console.error(error);process.exitCode=1;});

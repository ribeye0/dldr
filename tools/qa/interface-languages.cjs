// Production Svelte route + isolated IPC fixtures. No credentials, network downloads or real files.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const url = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story&reliability=1`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(), 'dld-languages-qa-'));
const languages = [
  {code:'zh-TW',unlimited:'不限速',library:'作品庫',downloaded:'已下載',settings:'設定',genre:'低語',filter:'篩選',limit:'下載總速率上限（MiB/s）'},
  {code:'zh-CN',unlimited:'不限速',library:'作品库',downloaded:'已下载',settings:'设置',genre:'低语',filter:'筛选',limit:'总下载速度上限（MiB/s）'},
  {code:'ja',unlimited:'無制限',library:'ライブラリ',downloaded:'ダウンロード済み',settings:'設定',genre:'ささやき',filter:'絞り込み',limit:'ダウンロードの合計速度制限（MiB/s）'},
  {code:'en',unlimited:'Unlimited',library:'Library',downloaded:'Downloaded',settings:'Settings',genre:'Whispering',filter:'Filters',limit:'Total download limit (MiB/s)'},
];
const nav = (page, index) => page.locator(`aside nav button[data-view=${['library','downloaded','settings'][index]}]`);
const settled = page => page.waitForFunction(() => document.querySelectorAll('.product-card').length > 0);
(async () => {
 const browser = await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try {
  const page = await browser.newPage({viewport:{width:1360,height:1000}}); page.setDefaultTimeout(15000);
  const errors=[]; page.on('pageerror',error=>errors.push(error.message));
  await page.goto(url); await settled(page);
  const original = await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('get_settings'));
  const input = page.locator('.library-controls input');
  await input.fill('$tag:ささやき$'); await page.getByRole('button',{name:'Search',exact:true}).click();
  await page.waitForFunction(()=>document.querySelectorAll('.product-card').length>0 && [...document.querySelectorAll('.product-card')].every(c=>c.querySelector('.product-tags')?.textContent.includes('Whispering')));
  await page.locator('.product-card input[type=checkbox]:not(:disabled)').first().check();
  const selectedId = await page.locator('.product-card input:checked').getAttribute('aria-label');
  await page.getByRole('button',{name:'Filters',exact:true}).click();
  await page.locator('.metadata-filters .group').first().getByRole('button').filter({hasText:'Whispering'}).click();
  for (const language of languages) {
   await nav(page,2).click(); await page.locator('.settings-layout .tabs button').nth(0).click();
   await page.locator('#ui-locale').selectOption(language.code);
   assert.equal(await nav(page,0).innerText(),language.library);
   assert.equal(await page.locator('html').getAttribute('lang'),language.code);
   const form = page.locator('form').filter({has:page.locator('#ui-locale')});
   await form.locator('button[type=submit]').click();
   await page.waitForFunction(async code=>(await window.__TAURI_INTERNALS__.invoke('get_settings')).uiLocale===code,language.code);
   const settings = await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('get_settings'));
   const defaults = { 'zh-TW':['zh_TW','zh_CN','ja_JP','en_US'], 'zh-CN':['zh_CN','zh_TW','ja_JP','en_US'], ja:['ja_JP','zh_TW','zh_CN','en_US'], en:['en_US','ja_JP','zh_TW','zh_CN'] };
   assert.deepEqual(settings.languagePriority.slice(0,defaults[language.code].length), defaults[language.code]);
   assert.equal(settings.libraryRoot, original.libraryRoot); assert.equal(settings.downloadRoot, original.downloadRoot);
   await nav(page,0).click(); await settled(page);
   assert.equal(await input.inputValue(),'$tag:ささやき$');
   assert.equal(await page.locator('.product-card input:checked').count(),1);
   const genre = page.locator('.metadata-filters .group').first().getByRole('button').filter({hasText:language.genre});
   assert.equal(await genre.getAttribute('class').then(v=>v.includes('included')),true);
   assert.ok((await page.locator('.product-card .product-tags').allTextContents()).every(text=>text.includes(language.genre)));
   const credits = await page.locator('.product-card .card-credits dd').first().innerText();
   await page.locator('.product-card').first().locator('.card-title').click();
   await page.locator('dialog #detail-tags').waitFor();
   assert.ok((await page.locator('dialog .tag-overview').innerText()).includes(language.genre));
   assert.ok((await page.locator('dialog .identity-credits').innerText()).includes(credits));
   await page.keyboard.press('Escape');
   await page.screenshot({path:path.join(captures,`${language.code}-library.png`)});
   await nav(page,1).click(); await settled(page);
   assert.equal(await page.locator('.page-header h1').innerText(),language.downloaded);
   assert.equal(await page.locator('.library-controls input').inputValue(),'');
   await nav(page,2).click(); await page.locator('.settings-layout > .page-header .tabs button').nth(1).click(); await page.getByText(language.limit,{exact:true}).waitFor();
   await page.locator('.current-limit').filter({hasText:language.unlimited}).waitFor();
   await page.locator('.download-status').click();
   assert.equal(await page.locator('.queue .entry').count(),3);
   await page.locator('.download-panel header button').click();
   await page.locator('.settings-layout > .page-header .tabs button').nth(2).click(); assert.ok(await page.locator('.accounts-panel').count());
   await nav(page,0).click(); await settled(page);
   await page.setViewportSize({width:390,height:844});
   await page.screenshot({path:path.join(captures,`${language.code}-narrow.png`)});
   assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
   await page.setViewportSize({width:1360,height:1000});
  }
  await page.reload(); await settled(page);
  assert.equal(await page.locator('html').getAttribute('lang'),'en');
  assert.equal(await page.locator('.product-card input:checked').count(),1);
  assert.ok(selectedId);
  // Different language names resolve to the same stored classification in plain and precise searches.
  const search = page.locator('.library-controls input');
  await page.getByRole('button',{name:'Reset filters',exact:true}).click();
  const sets=[];
  for (const name of ['低語','低语','ささやき','Whispering']) {
   await search.fill(`$tag:${name}$`); await page.getByRole('button',{name:'Search',exact:true}).click();
   await settled(page); sets.push(await page.locator('.product-card input').evaluateAll(items=>items.map(el=>el.getAttribute('aria-label'))));
  }
  sets.forEach(values=>assert.deepEqual(values,sets[0]));
  await search.fill('$tag:NTR'); await page.getByRole('alert').filter({hasText:'Missing closing $'}).waitFor();
  assert.equal(await page.getByRole('button',{name:'Search',exact:true}).isDisabled(),true);
  assert.deepEqual(errors,[]);
  console.log(`Four-language GUI QA passed: ${captures}`);
 } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});

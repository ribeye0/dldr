// Production route and isolated IPC; no real profile or purchased downloads.
const assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE_PATH||'playwright');
const url=`${process.env.STORYBOOK_URL||'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures=fs.mkdtempSync(path.join(os.tmpdir(),'dld-title-presets-'));
const presets={'zh-TW':['zh_TW','zh_CN','ja_JP','en_US'],'zh-CN':['zh_CN','zh_TW','ja_JP','en_US']};
const nav=(p,index)=>p.locator(`aside nav button[data-view=${['library','downloaded','settings'][index]}]`);
const orderOf=list=>list.locator('code').allTextContents();
const rows=p=>p.getByRole('list').filter({has:p.locator('code')}).first();
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try{
 const page=await browser.newPage({viewport:{width:1280,height:960}});page.setDefaultTimeout(12000);
 await page.addInitScript(()=>{ const key='dldr-production-story-settings'; if(!sessionStorage.getItem(key)) sessionStorage.setItem(key,JSON.stringify({uiLocale:'zh-TW',languagePriority:['zh_TW','zh_CN','ja_JP','en_US']})); });
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(url);await page.locator('.product-card').first().waitFor();
 await page.locator('.product-card input:enabled').first().check();
 const card=page.locator('.product-card').filter({has:page.locator('input:checked')});
 const maker=await card.locator('.card-credits dd').first().innerText();
 const actors=await card.locator('.voice-chip').allTextContents();
 for(const code of ['zh-TW','zh-CN','zh-TW']){
   await nav(page,2).click();await page.locator('.settings-layout > .page-header .tabs button').nth(0).click();
   const list=rows(page);
   await page.locator('#ui-locale').selectOption(code);
   const order=await orderOf(list);
   assert.deepEqual(order.slice(0,presets[code].length),presets[code]);
   assert.equal(await list.locator('li').count(),5);
   assert.equal(order.includes('ko_KR'),false);
   await page.locator('form').filter({has:page.locator('#ui-locale')}).locator('button[type=submit]').click();
   await page.waitForFunction(async language=>(await window.__TAURI_INTERNALS__.invoke('get_settings')).uiLocale===language,code);
   await page.screenshot({path:path.join(captures,`${code}-settings.png`)});
   await nav(page,0).click();await card.waitFor();
   assert.match(await card.locator('.card-title').innerText(),code==='zh-TW'?/靜夜故事/:/静夜故事/);
   assert.equal(await card.locator('.card-credits dd').first().innerText(),maker);
   assert.deepEqual(await card.locator('.voice-chip').allTextContents(),actors);
 }
 await nav(page,2).click();
 const list=rows(page);
 // An explicit manual reorder survives changing the interface locale and a restart.
 await list.locator('li').filter({hasText:'ja_JP'}).getByRole('button').first().click();
 const custom=await orderOf(list);
 await page.locator('#ui-locale').selectOption('zh-CN');assert.deepEqual(await orderOf(list),custom);
 await page.locator('form').filter({has:page.locator('#ui-locale')}).locator('button[type=submit]').click();
 await page.waitForFunction(async expected=>JSON.stringify((await window.__TAURI_INTERNALS__.invoke('get_settings')).languagePriority)===JSON.stringify(expected),custom);
 await page.reload();await page.locator('.product-card').first().waitFor();await nav(page,2).click();
 assert.deepEqual(await orderOf(list),custom);
 await page.setViewportSize({width:390,height:844});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
 await page.screenshot({path:path.join(captures,'custom-narrow.png')});
 assert.deepEqual(errors,[]);console.log(JSON.stringify({passed:true,captures}));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});

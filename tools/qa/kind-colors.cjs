// Isolated production-route fixtures only; checks rendered colors without real downloads.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const url = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(),'dld-kind-colors-'));
const rgb = hex => `rgb(${hex.match(/../g).map(v=>parseInt(v,16)).join(', ')})`;
const colors = {audio:'feecdf',comic:'dff4e6',game:'f1eafd',illustration:'e9f5ff',video:'f5eaff',other:'eeeeee'};
const background = locator => locator.evaluate(el=>getComputedStyle(el).backgroundColor);
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try {
  const page=await browser.newPage({viewport:{width:1360,height:1000}});page.setDefaultTimeout(12000);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>sessionStorage.setItem('dldr-production-story-settings',JSON.stringify({uiLocale:'zh-TW'})));
  await page.goto(url);await page.locator('.product-card').first().waitFor();
  await page.evaluate(()=>{
   const original=window.__TAURI_INTERNALS__.invoke;const works=new Map();
   window.__TAURI_INTERNALS__.invoke=async(command,args,...rest)=>{
    const result=await original(command,args,...rest);
    if(command==='list_products')result.products=result.products.map((p,index)=>{
     if(index>=6)return p;
     const value={...p,workType:['SOU','MNG','GAM','ICG','MOV','SOF'][index],ageCategory:index%2?'r18':'all'};
     works.set(p.workId,value);return value;
    });
    if(command==='get_product_detail'&&works.has(result.workId)){
     const p=works.get(result.workId);return {...result,workType:p.workType,ageCategory:p.ageCategory};
    }
    return result;
   };
  });
  await page.locator('.library-controls').getByRole('button',{name:'重新載入',exact:true}).click();
  await page.locator('.product-card .kind[data-tone=illustration]').first().waitFor();
  for(const [tone,color]of Object.entries(colors)){
   const badge=page.locator(`.product-card .kind[data-tone="${tone}"]`).first();
   assert.equal(await background(badge),rgb(color),tone);
   assert.equal(await badge.evaluate(el=>getComputedStyle(el).color),rgb('092a83'));
  }
  assert.equal(await background(page.locator('.product-card .age[data-age=all]').first()),rgb('6ac921'));
  assert.equal(await background(page.locator('.product-card .age[data-age=r18]').first()),rgb('ff91b7'));
  assert.match(await page.locator('.download-status .status-title').innerText(),/^下載中/);
  await page.locator('.product-card').nth(1).locator('.card-title').click();
  assert.equal(await background(page.locator('dialog .kind')),rgb('dff4e6'));
  assert.equal(await background(page.locator('dialog .age')),rgb('ff91b7'));
  await page.screenshot({path:path.join(captures,'comic-details.png')});await page.keyboard.press('Escape');
  await page.getByRole('button',{name:'篩選',exact:true}).click();
  for(const kind of ['audio','game','video','other'])assert.equal(await background(page.locator(`[data-type-filter=${kind}]`)),rgb(colors[kind]));
  assert.equal(await background(page.locator('[data-age-filter=all]')),rgb('6ac921'));
  assert.equal(await background(page.locator('[data-age-filter=r18]')),rgb('ff91b7'));
  await page.locator('[data-type-filter=audio]').click();
  await page.waitForFunction(expected=>getComputedStyle(document.querySelector('[data-type-filter=audio]')).backgroundColor===expected,rgb(colors.audio));
  await page.screenshot({path:path.join(captures,'desktop.png')});
  await page.setViewportSize({width:390,height:844});await page.screenshot({path:path.join(captures,'narrow.png')});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  assert.deepEqual(errors,[]);console.log(`Work-kind and age colors QA passed: ${captures}`);
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});

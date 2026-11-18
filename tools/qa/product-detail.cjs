// Production route with isolated IPC fixtures; never uses real credentials or purchased media.
const assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const story = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(), 'dldr-product-detail-'));
(async () => {
 const browser = await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try {
  for (const width of [1280,390]) {
   const context = await browser.newContext({viewport:{width,height:900},permissions:['clipboard-read','clipboard-write']});
   const page = await context.newPage(); page.setDefaultTimeout(10000);
   const errors = []; page.on('pageerror', error => errors.push(error.message));
   await page.addInitScript(theme=>localStorage.setItem("dldr.color-theme",theme),width===390?"dark":"light");
   await page.goto(story);
   await page.locator('.product-card').first().waitFor();
   await page.evaluate(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    const qa = window.__detailQa = {loads:0,fail:false,sparse:false,updated:false,external:[]};
    window.__TAURI_INTERNALS__.invoke = async function(command,args,...rest) {
     if(command === 'get_product_page_content') {
      qa.loads++; if(qa.fail) throw new Error('Public page unavailable (simulated)');
     }
     if(command === 'open_external_url') {qa.external.push(args.url);return;}
     const result = await original.call(this,command,args,...rest);
     if(command === 'get_product_detail') return {...result,
      registeredAt:'2026-01-01T00:00:00Z',publishedAt:'2026-02-02T00:00:00Z',updatedAt:qa.updated===true?'2026-03-03T00:00:00Z':qa.updated==='upload'?'2026-02-01T00:00:00Z':null,
      earliestPurchasedAt:'2026-04-04T00:00:00Z', latestPurchasedAt:'2026-04-04T00:00:00Z',
      creditGroups:qa.sparse?[]:[{kind:'voice',label:'Voice',names:['声優']},{kind:'illust',label:'Illust',names:['絵師']},{kind:'scenario',label:'Scenario',names:['脚本家']},{kind:'series',label:'Series',names:['シリーズ']}],
     };
     return result;
    };
   });
   const open = async () => {await page.locator('.product-card').first().getByRole('button',{name:/^Details:/}).click();await page.getByRole('dialog',{name:'Work details',exact:true}).waitFor();};
   await open();
   const detail = page.getByRole('dialog',{name:'Work details',exact:true});
   assert.deepEqual(await detail.locator('.credits-grid dt').allTextContents(),['Illust','Scenario','Series']);
   for(const label of ['Custom Tags','Download details','Ownership & purchases','Work information & other names']) assert.equal(await detail.getByText(label,{exact:true}).count(),0);
   assert.equal(await page.evaluate(()=>window.__detailQa.loads),0);
   await detail.locator('summary').filter({hasText:/^Dates$/}).click();
   assert.deepEqual(await detail.locator('.metadata-body dt').allTextContents(),['Registered','Published','Purchased']);
   assert.deepEqual(await detail.locator('.metadata-body dd').allTextContents(),['2026-01-01','2026-02-02','2026-04-04']);
   const content = detail.locator('.product-page-section');
   assert.equal(await content.getAttribute('open'),null);
   await content.locator('summary').click();
   await content.locator('.product-page-content').waitFor();
   assert.equal(await page.evaluate(()=>window.__detailQa.loads),1);
   await content.locator('a').click();
   assert.deepEqual(await page.evaluate(()=>window.__detailQa.external),['https://www.dlsite.com/']);
   assert.equal(page.url(),story);
   await content.locator('summary').click(); await content.locator('summary').click();
   await content.locator('.product-page-content').waitFor();
   assert.equal(await page.evaluate(()=>window.__detailQa.loads),1);
   // Copy feedback remains above the scrolling dialog, fully visible and dismissible.
   await detail.locator('.copy-title').click();
   await page.locator('.toast-stack:popover-open .toast').first().waitFor();
   await detail.evaluate(node=>node.scrollTop=node.scrollHeight);
   const visibility = await page.locator('.toast-stack').evaluate(node=>{
    const rect=node.getBoundingClientRect(),close=node.querySelector('.toast-close'),r=close.getBoundingClientRect();
    return {inViewport:rect.left>=0&&rect.right<=innerWidth&&rect.top>=0&&rect.bottom<=innerHeight,
     hit:document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)?.closest('.toast-close')===close};
   });
   assert.equal(visibility.inViewport,true);assert.equal(visibility.hit,true);
   assert.ok(await page.locator('.toast-stack').evaluate(node=>node.getBoundingClientRect().top > node.closest('dialog').querySelector('header').getBoundingClientRect().bottom));
   await page.screenshot({path:path.join(captures,`details-${width}.png`)});
   // An image-preview dialog becomes the active notification parent without clipping it.
   await detail.getByRole('button',{name:'Preview cover image',exact:true}).evaluate(button=>button.click());
   await page.waitForFunction(()=>document.querySelectorAll('dialog[open]').length===2);
   assert.equal(await page.locator('dialog[open]').last().locator('.toast-stack:popover-open').count(),1);
   await page.locator('.toast-close').first().click();
   await page.keyboard.press('Escape');
   await page.waitForFunction(()=>document.querySelectorAll('dialog[open]').length===1);
   await page.keyboard.press('Escape');
   await detail.waitFor({state:'hidden'});
   // A new work/detail session starts collapsed; failures can be retried.
   await page.evaluate(()=>{window.__detailQa.sparse=true;window.__detailQa.fail=true;window.__detailQa.updated=true;});
   await open();assert.equal(await detail.locator('#detail-credits').count(),0);
   assert.equal(await content.getAttribute('open'),null);
   await content.locator('summary').click();
   await content.getByRole('alert').waitFor();
   await page.evaluate(()=>window.__detailQa.fail=false);
   await content.getByRole('button',{name:'Retry',exact:true}).click();
   await content.locator('.product-page-content').waitFor();
   await detail.locator('summary').filter({hasText:/^Dates$/}).click();
   assert.deepEqual(await detail.locator('.metadata-body dt').allTextContents(),['Registered','Published','Updated','Purchased']);
   await page.keyboard.press('Escape');
   await page.evaluate(()=>window.__detailQa.updated='upload');
   await open();
   await detail.locator('summary').filter({hasText:/^Dates$/}).click();
   assert.deepEqual(await detail.locator('.metadata-body dt').allTextContents(),['Registered','Published','Purchased']);
   assert.deepEqual(errors,[]);
   await context.close();
  }
  console.log(`Product detail, lazy content, dates, credits and notification QA passed: ${captures}`);
 } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exit(1);});

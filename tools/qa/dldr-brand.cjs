// Isolated production route; no real accounts or purchased transfers.
const assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE_PATH||'playwright');
const url=`${process.env.STORYBOOK_URL||'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures=fs.mkdtempSync(path.join(os.tmpdir(),'dldr-brand-'));
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try{
 const page=await browser.newPage({viewport:{width:1920,height:1080}});page.setDefaultTimeout(12000);
 const errors=[];page.on('pageerror',error=>errors.push(error.message));
 await page.goto(url);await page.locator('.product-card').first().waitFor();
 assert.equal(await page.title(),'DLDR — DLsite Downloader');
 const logo=page.locator('aside .brand-logo');
 assert.equal(await logo.getAttribute('alt'),'DLDR — DLsite Downloader');
 assert.equal(await logo.evaluate(img=>img.complete && img.naturalWidth>0),true);
 assert.equal(await page.locator('aside .brand-link svg').count(),0,'transparent image needs no SVG background mask');
 assert.match(await page.locator('aside .version').innerText(),/^DLDR\s/);
 for(const [width,height] of [[1920,1080],[960,1080],[390,844]]){
   await page.setViewportSize({width,height});
   assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
   const box=await logo.boundingBox();assert.ok(box.width>0 && box.x+box.width<=width);
   assert.ok(await page.locator('.download-status').isVisible());
   await page.screenshot({path:path.join(captures,`library-${width}.png`)});
 }
 await page.setViewportSize({width:1280,height:960});
 await page.locator('aside nav button[data-view=settings]').click();
 await page.locator('.settings-layout > .page-header .tabs button').last().click();
 await page.getByRole('heading',{name:'DLDR',exact:true}).waitFor();
 await page.getByText('DLsite Downloader',{exact:true}).waitFor();
 assert.equal(await page.locator('.app-mark img').evaluate(img=>img.complete && img.naturalWidth>0),true);
 await page.screenshot({path:path.join(captures,'about.png')});
 assert.deepEqual(errors,[]);console.log(JSON.stringify({passed:true,captures}));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});

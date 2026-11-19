// Isolated production route. Checks card sizing, creator pills and external-link dispatch.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const url = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(),'dld-card-layout-'));
const navy = 'rgb(9, 42, 131)';
const nav = (page,index) => page.locator(`aside nav button[data-view=${['library','downloaded','settings'][index]}]`);
const cardsReady = page => page.locator('.product-card').first().waitFor();
const checkLabels = async card => {
 const labels = await card.locator('.cover-labels').evaluate(node => [...node.querySelectorAll('.badge,.work-id')].map(el => {
  const box=el.getBoundingClientRect(),range=document.createRange();range.selectNodeContents(el);
  const text=range.getBoundingClientRect();
  return {size:parseFloat(getComputedStyle(el).fontSize),x:Math.abs((box.left+box.right-text.left-text.right)/2),y:Math.abs((box.top+box.bottom-text.top-text.bottom)/2),height:box.height};
 }));
 assert.equal(labels.length,3);
 for(const label of labels){assert.ok(label.size>=12);assert.equal(label.height,labels[0].height);assert.ok(label.x<1);assert.ok(label.y<2);}
};
const checkCredits = async page => {
 const credits=await page.locator('dialog .identity-credits > div').evaluateAll(rows=>rows.map(row=>{
  const label=row.querySelector('dt'),value=row.querySelector('dd'),name=value.querySelector('button');
  const labelRange=document.createRange(),nameRange=document.createRange();labelRange.selectNodeContents(label);nameRange.selectNodeContents(name);
  return {left:value.getBoundingClientRect().left,top:labelRange.getClientRects()[0].top-nameRange.getClientRects()[0].top};
 }));
 assert.equal(credits.length,2);assert.ok(Math.abs(credits[0].left-credits[1].left)<1);
 for(const row of credits)assert.ok(Math.abs(row.top)<1,JSON.stringify(credits));
};
const measure = page => page.evaluate(()=>({
 width:innerWidth,sidebar:document.querySelector('aside').getBoundingClientRect().width,
 card:document.querySelector('.product-card').getBoundingClientRect().width,
 columns:getComputedStyle(document.querySelector('.product-table')).gridTemplateColumns.split(' ').length,
 overflow:document.documentElement.scrollWidth>innerWidth,
}));
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try{
  const page=await browser.newPage({viewport:{width:1920,height:1080}});page.setDefaultTimeout(12000);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>sessionStorage.setItem('dldr-production-story-settings',JSON.stringify({uiLocale:'zh-TW'})));
  await page.goto(url);await cardsReady(page);
  await page.evaluate(() => document.fonts.ready);
  assert.deepEqual(await page.evaluate(async () =>
   (await document.fonts.load('600 16px "DLDR Noto Sans CJK"', '繁體中文简体中文日本語𠮷')).map(face => face.status)
  ), ['loaded'], 'the bundled CJK font must decode and load');
  await page.evaluate(()=>{
   const original=window.__TAURI_INTERNALS__.invoke;
   const probe=window.__cardLayoutQa={urls:[],details:0,downloads:0,playerOpens:0,clipboard:'',longNames:false};
   navigator.clipboard.writeText=async value=>{probe.clipboard=value;};
   window.__TAURI_INTERNALS__.invoke=async(command,args,...rest)=>{
    if(command==='open_external_url')probe.urls.push(args.url);
    if(command==='get_product_detail')probe.details++;
    if(command==='open_player')probe.playerOpens++;
    if(['start_work_download','start_bulk_work_download'].includes(command))probe.downloads++;
    const result=await original(command,args,...rest);
    if(command==='list_products'&&probe.longNames)result.products[0]={...result.products[0],creditGroups:[{kind:'voice',label:'CV',names:['とても長い声優名'.repeat(30),'第二位聲優']} ]};
    return result;
   };
  });
  const full=await measure(page);assert.equal(full.sidebar,196);assert.equal(full.columns,6);assert.ok(full.card>260 && full.card<280);assert.equal(full.overflow,false);
  for(const width of [1600,2560,3840]){
   await page.setViewportSize({width,height:1080});
   const wide=await measure(page);assert.equal(wide.columns,6,`six columns at ${width}px`);assert.equal(wide.overflow,false);
  }
  await page.setViewportSize({width:1920,height:1080});
  const first=page.locator('.product-card').first();
  await checkLabels(first);
  const actors=await first.locator('.voice-chip').allTextContents();assert.equal(actors.length,2);
  for(const selector of ['.kind','.age','.product-tag','.voice-chip']){
   const styles=await first.locator(selector).first().evaluate(el=>({border:getComputedStyle(el).borderTopColor,radius:getComputedStyle(el).borderTopLeftRadius}));
   assert.equal(styles.border,navy,selector);if(selector==='.voice-chip')assert.equal(styles.radius,'999px');
  }
  const link=first.locator('.product-page-link');
  const href=await link.getAttribute('href');assert.match(href,/^https:\/\/www\.dlsite\.com\/home\/work\/=\/product_id\/RJ\d+\.html$/);
  const positions=await first.locator('.file-meta').evaluate(el=>{const size=el.querySelector('.file-size').getBoundingClientRect(),link=el.querySelector('a').getBoundingClientRect();return {sizeRight:size.right,linkLeft:link.left,sizeTop:size.top,linkTop:link.top};});
  assert.ok(positions.linkLeft>positions.sizeRight);assert.ok(Math.abs(positions.linkTop-positions.sizeTop)<5);
  await first.locator('input[type=checkbox]').check();
  const id=await first.locator('.work-id').innerText();
  await first.locator('.work-id').click();
  assert.equal(await page.evaluate(()=>window.__cardLayoutQa.clipboard),id);
  assert.equal(await page.locator('.product-card input:checked').count(),1);
  assert.equal(await page.locator('.player-panel').count(),0);
  // Use the stable class for measurements; locale wording is independently tested.
  const selectionBox=await page.locator('.selection-bar').boundingBox();
  assert.equal(selectionBox.width,640);assert.ok(selectionBox.width<full.width-full.sidebar-100);
  const before=await page.evaluate(()=>window.__cardLayoutQa.details);
  await link.click();assert.deepEqual(await page.evaluate(()=>window.__cardLayoutQa.urls),[href]);
  assert.equal(await page.evaluate(()=>window.__cardLayoutQa.details),before);
  assert.equal(await page.locator('.product-card input:checked').count(),1);assert.equal(page.url(),url);
  await first.locator('.card-title').click();
  await page.locator('dialog .voice-chip').first().waitFor();assert.deepEqual(await page.locator('dialog .voice-chip').allTextContents(),actors);
  await checkCredits(page);
  assert.equal(await page.locator('dialog .voice-chip').first().evaluate(el=>getComputedStyle(el).borderTopColor),navy);
  assert.equal(await page.locator('dialog #detail-tags + .chips button').first().evaluate(el=>getComputedStyle(el).borderTopColor),navy);
  await page.locator('dialog .voice-chip').first().click();assert.equal(await page.evaluate(()=>window.__cardLayoutQa.clipboard),actors[0]);
  await page.screenshot({path:path.join(captures,'details.png')});await page.keyboard.press('Escape');
  await first.locator('.voice-chip').first().click();
  await page.waitForFunction(()=>document.querySelector('.library-controls input').value.startsWith('$va:'));
  assert.equal(await page.locator('.library-controls input').inputValue(),`$va:${actors[0]}$`);
  await page.getByRole('button',{name:'重設篩選',exact:true}).click();await cardsReady(page);
  await first.scrollIntoViewIfNeeded();await page.screenshot({path:path.join(captures,'full.png')});
  await page.setViewportSize({width:960,height:1080});
  const half=await measure(page);assert.equal(half.columns,3);assert.ok(half.card>215 && half.card<245);assert.equal(half.overflow,false);
  await page.screenshot({path:path.join(captures,'half.png')});
  await nav(page,1).click();await cardsReady(page);
  const downloaded=page.locator('.product-card').first();
  assert.equal(await page.locator('.listen-button').count(),0);
  const copied=await downloaded.locator('.work-id').innerText();
  await downloaded.locator('.work-id').click();
  assert.equal(await page.evaluate(()=>window.__cardLayoutQa.clipboard),copied);
  assert.equal(await page.locator('.player-panel').count(),0);
  await downloaded.locator('.card-text').filter({hasText:'社團'}).click();
  assert.equal(await page.locator('.player-panel').count(),0);
  // Blank space in both the outer padding and the nested metadata row opens
  // only the player. Controls above it and text remain independent hit targets.
  const beforePlayDetails=await page.evaluate(()=>window.__cardLayoutQa.details);
  await downloaded.click({position:{x:6,y:6}});await page.locator('.player-panel').waitFor();
  assert.equal(await page.evaluate(()=>window.__cardLayoutQa.details),beforePlayDetails);
  assert.equal(await page.locator('.player-panel .primary-control').getAttribute('aria-label'),'播放');
  await page.keyboard.press('Escape');
  const gap=await downloaded.locator('.file-meta').evaluate(el=>{
   const a=el.querySelector('.file-size').getBoundingClientRect(),b=el.querySelector('a').getBoundingClientRect();
   return {x:(a.right+b.left)/2,y:(a.top+a.bottom)/2};
  });
  await page.mouse.click(gap.x,gap.y);await page.locator('.player-panel').waitFor();
  await page.keyboard.press('Escape');
  await downloaded.locator('.card-play-target').focus();await page.keyboard.press('Enter');await page.locator('.player-panel').waitFor();
  assert.equal(await page.evaluate(()=>window.__cardLayoutQa.playerOpens),3);
  await page.keyboard.press('Escape');
  assert.ok(await downloaded.locator('.product-page-link').count());assert.ok(await downloaded.locator('.voice-chip').count());
  await downloaded.locator('.product-page-link').click();assert.equal((await page.evaluate(()=>window.__cardLayoutQa.urls)).length,2);
  await nav(page,0).click();await cardsReady(page);
  for(const code of ['en','ja','zh-CN','zh-TW']){
   await nav(page,2).click();await page.locator('.settings-layout .tabs button').nth(0).click();
   await page.locator('#ui-locale').selectOption(code);
   await page.locator('form').filter({has:page.locator('#ui-locale')}).locator('button[type=submit]').click();
   await page.waitForFunction(async locale=>(await window.__TAURI_INTERNALS__.invoke('get_settings')).uiLocale===locale,code);
   await nav(page,0).click();await cardsReady(page);
   await first.locator('.card-title').click();await page.locator('dialog .identity-credits').waitFor();await checkCredits(page);
   await page.keyboard.press('Escape');
   assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
   await page.setViewportSize({width:390,height:844});await page.screenshot({path:path.join(captures,`${code}-narrow.png`)});
   assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
   assert.equal(await page.locator('.selection-bar').evaluate(el=>el.scrollWidth>el.clientWidth),false);
   const coverLabels=await first.locator('.cover-frame').evaluate(el=>{
    const cover=el.getBoundingClientRect(),id=el.querySelector('.work-id').getBoundingClientRect(),badges=el.querySelector('.work-badges').getBoundingClientRect();
    return {contained:id.left>=cover.left&&id.right<=cover.right&&id.top>=cover.top&&id.bottom<=cover.bottom,separate:id.top>=badges.bottom||id.left>=badges.right};
   });
   assert.equal(coverLabels.contained,true);assert.equal(coverLabels.separate,true);
   await checkLabels(first);
   await page.setViewportSize({width:960,height:1080});
  }
  await page.evaluate(()=>window.__cardLayoutQa.longNames=true);
  await page.locator('.library-controls').getByRole('button',{name:'重新載入',exact:true}).click();
  await first.locator('.voice-chip').filter({hasText:'第二位聲優'}).waitFor();
  assert.equal(await first.locator('.voice-chip').first().innerText(),'とても長い声優名'.repeat(30));
  assert.equal(await first.locator('.voice-chip').first().evaluate(el=>el.scrollWidth>el.clientWidth),false);
  assert.equal(await page.evaluate(()=>window.__cardLayoutQa.downloads),0);
  await nav(page,2).click();await page.locator('.settings-layout .tabs button').last().click();
  assert.equal(await page.locator('.settings-layout details').count(),0);
  assert.equal(await page.locator('.settings-layout').getByText('Tauri',{exact:true}).count(),0);
  assert.equal(await page.locator('.settings-layout').getByRole('button',{name:'GitHub ↗',exact:true}).count(),1);
  assert.deepEqual(errors,[]);console.log(JSON.stringify({passed:true,full,half,captures}));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});

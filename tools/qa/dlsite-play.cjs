// Production UI plus official-origin browser fixtures. No real account/media requests.
const assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const source = fs.readFileSync(path.join(__dirname,'../../apps/desktop/src-tauri/src/dlsite-play.js'),'utf8');
const captures = fs.mkdtempSync(path.join(os.tmpdir(),'dldr-play-qa-'));
const messages = locale => JSON.parse(fs.readFileSync(path.join(__dirname,`../../apps/desktop/src/lib/i18n/${locale}.json`),'utf8'));
const root = `https://play.dlsite.com`;
const story = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
(async () => {
 const browser = await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try {
  const page = await browser.newPage({viewport:{width:1360,height:1000}});
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto(story);await page.locator('.product-card').first().waitFor();
  const sidebarPlay=page.locator('[data-action=dlsite-play]');
  const launcher=page.getByRole('dialog',{name:'DLsite Play',exact:true});
  await sidebarPlay.click();await launcher.waitFor();
  await launcher.getByRole('button',{name:'Close DLsite Play',exact:true}).click();
  await launcher.waitFor({state:'hidden'});
  const pointerStyle=await sidebarPlay.evaluate(el=>({shadow:getComputedStyle(el).boxShadow,outline:getComputedStyle(el).outlineStyle}));
  assert.deepEqual(pointerStyle,{shadow:'none',outline:'none'});
  // Keyboard navigation keeps a complete focus outline, rather than an underline.
  await page.locator('aside nav button[data-view=settings]').focus();await page.keyboard.press('Tab');
  assert.equal(await sidebarPlay.evaluate(el=>document.activeElement===el),true);
  const keyboardStyle=await sidebarPlay.evaluate(el=>({shadow:getComputedStyle(el).boxShadow,outline:getComputedStyle(el).outlineStyle,width:getComputedStyle(el).outlineWidth}));
  assert.deepEqual(keyboardStyle,{shadow:'none',outline:'solid',width:'2px'});
  await page.keyboard.press('Enter');await launcher.waitFor();await page.keyboard.press('Escape');await launcher.waitFor({state:'hidden'});
  const field=page.locator('.search-field');
  const border=await field.evaluate(el=>getComputedStyle(el).borderTopColor);
  await field.locator('input').focus();assert.equal(await field.evaluate(el=>getComputedStyle(el).borderTopColor),border);
  const currentPage=page.locator('.pagination button[aria-current=page]').first();
  assert.deepEqual(await currentPage.evaluate(el=>({background:getComputedStyle(el).backgroundColor,opacity:getComputedStyle(el).opacity,color:getComputedStyle(el).color})),
    {background:'rgb(34, 85, 170)',opacity:'1',color:'rgb(255, 255, 255)'});
  await page.evaluate(()=>{
   const original=window.__TAURI_INTERNALS__.invoke;window.__playQa={opens:[],fail:false,downloads:0};
   window.__TAURI_INTERNALS__.invoke=async(command,args,...rest)=>{
    if(command==='open_dlsite_play') {if(window.__playQa.fail)throw new Error('Synthetic browser failure');window.__playQa.opens.push(args);return;}
    if(command==='start_work_download')window.__playQa.downloads++;
    return original(command,args,...rest);
   };
  });
  await page.locator('[data-action=dlsite-play]').click();
  let dialog=page.getByRole('dialog',{name:'DLsite Play',exact:true});await dialog.waitFor();
  assert.match(await dialog.innerText(),/saved separately from DLDR/);
  await dialog.locator('select').selectOption({index:1});
  await dialog.getByRole('button',{name:'Open DLsite Play',exact:true}).click();
  await dialog.waitFor({state:'hidden'});
  assert.equal((await page.evaluate(()=>window.__playQa.opens))[0].workId,null);
  const card=page.locator('.product-card').filter({has:page.locator('.online-play')}).first();
  const workId=await card.locator('.work-id').innerText();
  await card.locator('.online-play').click();await dialog.waitFor();
  await dialog.locator('select').selectOption({index:1});
  await page.evaluate(()=>window.__playQa.fail=true);
  await dialog.getByRole('button',{name:'Open DLsite Play',exact:true}).click();
  await dialog.getByRole('alert').getByText('Synthetic browser failure').waitFor();
  await page.evaluate(()=>window.__playQa.fail=false);
  await dialog.getByRole('button',{name:'Open DLsite Play',exact:true}).click();
  await dialog.waitFor({state:'hidden'});
  assert.equal((await page.evaluate(()=>window.__playQa.opens)).at(-1).workId,workId);
  assert.equal(await page.evaluate(()=>window.__playQa.downloads),0);
  await page.screenshot({path:path.join(captures,'library-online.png')});
  // Paired card actions reuse the queue and remain separate from local playback.
  const pendingId=await page.locator('.product-card:has(.card-download:enabled)').first().locator('.work-id').innerText();
  const pending=page.locator('.product-card').filter({has:page.locator('.work-id',{hasText:pendingId})});
  assert.deepEqual(await pending.locator('.card-actions button').allTextContents(),['DLPLAY','Download']);
  await pending.locator('.card-download').click();
  await page.waitForFunction(()=>window.__playQa.downloads===1);
  await pending.locator('.card-download:disabled').waitFor();
  assert.equal(await page.evaluate(()=>window.__playQa.downloads),1);
  assert.equal(await pending.locator('.work-id').innerText(),pendingId);
  await pending.locator('.playlist-toggle').click();
  await page.locator('aside nav button[data-view=playlists]').click();
  const playlistCard=page.locator('.playlists-view .product-card');await playlistCard.waitFor();
  assert.deepEqual(await playlistCard.locator('.card-actions button').allTextContents(),['DLPLAY','Download']);
  assert.equal(await playlistCard.locator('.card-download').isDisabled(),true);
  await page.locator('aside nav button[data-view=downloaded]').click();
  const completed=page.locator('.product-card').first();await completed.waitFor();
  assert.deepEqual(await completed.locator('.card-actions button').allTextContents(),['DLPLAY','Open directory']);
  assert.equal(await completed.locator('.file-meta button').count(),0);
  await completed.locator('.online-play').click();await dialog.waitFor();
  assert.equal(await page.locator('.player-panel').count(),0);
  await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});
  await page.screenshot({path:path.join(captures,'downloaded-actions.png')});
  for (const locale of ['zh-TW','zh-CN','ja','en']) {
   await page.locator('aside nav button[data-view=settings]').click();await page.locator('#ui-locale').selectOption(locale);
   await page.locator('aside nav button[data-view=downloaded]').click();await completed.waitFor();
   await page.setViewportSize({width:390,height:844});
   assert.deepEqual(await completed.locator('.card-actions button').allTextContents(),['DLPLAY',messages(locale)['Open directory']]);
   const box=await completed.locator('.card-actions').evaluate(el=>({width:el.clientWidth,scroll:el.scrollWidth}));
   assert.ok(box.scroll<=box.width);
   assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
   await completed.screenshot({path:path.join(captures,`card-actions-${locale}.png`)});
   await page.setViewportSize({width:1360,height:1000});
  }
  await page.setViewportSize({width:390,height:844});await page.locator('[data-action=dlsite-play]').click();await dialog.waitFor();
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  await page.screenshot({path:path.join(captures,'launch-mobile.png')});await page.close();

  for (const locale of ['en','zh-TW','zh-CN','ja']) {
   const context=await browser.newContext({viewport:{width:1000,height:800}});
   const tab=await context.newPage(); const bridge=[];let state='ready';let downloads=0;
   await tab.addInitScript(() => {
    const original=Element.prototype.attachShadow;
    Element.prototype.attachShadow=function(options){const root=original.call(this,options);if(this.id==='dldr-play-panel')window.__panelRoot=root;return root;};
    const now=Date.now;window.__advance=0;Date.now=()=>now()+window.__advance;
   });
   await tab.addInitScript(`(${source.replace('location.assign(url.href);','window.__nativeNavigation(url.href);')})(${JSON.stringify({token:'fixture-token',accountLabel:'測試 · <not HTML>',messages:messages(locale)})});`);
   // Native cancels before navigation commits. Mock only this transport boundary;
   // network-level aborts in Chromium would replace the document with an error page.
   await tab.exposeFunction('__nativeNavigation', async href => {
    const url=new URL(href);bridge.push(Object.fromEntries(url.searchParams));
    const action=url.searchParams.get('action');
    assert.equal(url.origin,root);assert.equal(url.pathname,'/__dldr_bridge');
    assert.equal(url.searchParams.get('token'),'fixture-token');
    if(action==='download'){downloads++;state='queued';}
    const message={ready:'Download to DLDR',queued:'Download queued',downloading:'Open DLDR for verification or download progress.',downloaded:'Downloaded',unavailable:'Sync purchases in DLDR before downloading this work.',error:'Could not add the download. Check DLDR Logs.'}[state];
    await tab.evaluate(response=>window.dispatchEvent(new CustomEvent('dldr-play-status',{detail:response})),{workId:url.searchParams.get('work'),sequence:Number(url.searchParams.get('sequence')),status:state,message}).catch(()=>{});
   });
   await tab.route('**/*', route => route.fulfill({contentType:'text/html',body:'<!doctype html><html><body style="margin:0;background:#f4f7fc;font:16px system-ui;padding:24px"><h1>Official-player route fixture</h1><p>No purchased media.</p></body></html>'}));
   await tab.goto(root+'/library');assert.equal(await tab.locator('#dldr-play-panel').count(),0);
   await tab.evaluate(()=>history.pushState({},'', '/work/rj01760323'));
   await tab.waitForFunction(()=>window.__panelRoot?.querySelector('button')?.disabled===false);
   assert.equal(await tab.evaluate(()=>window.__panelRoot.querySelector('button').textContent),messages(locale)['Download to DLDR']);
   assert.equal(new URL(tab.url()).pathname,'/work/rj01760323');
   assert.equal(downloads,0);
   const rect=await tab.evaluate(()=>{const r=window.__panelRoot.querySelector('button').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};});
   await tab.mouse.click(rect.x,rect.y);
   await tab.waitForFunction(label=>window.__panelRoot.querySelector('button').textContent===label,messages(locale)['Download queued']);
   assert.equal(downloads,1);
   await tab.mouse.click(rect.x,rect.y);assert.equal(downloads,1);
   for(const next of ['downloading','downloaded','unavailable','error','ready']) {
    state=next;await tab.evaluate(()=>window.__advance+=6000);
    const expected=next==='downloading'?messages(locale)['Downloading']:next==='downloaded'?messages(locale)['Downloaded']:next==='error'?messages(locale)['Retry download']:messages(locale)['Download to DLDR'];
    await tab.waitForFunction(label=>window.__panelRoot.querySelector('button').textContent===label,expected);
    await tab.waitForFunction(enabled=>window.__panelRoot.querySelector('button').disabled!==enabled,['error','ready'].includes(next));
   }
   await tab.evaluate(()=>window.dispatchEvent(new CustomEvent('dldr-play-status',{detail:{workId:'RJ1',sequence:0,status:'downloaded',message:'Downloaded'}})));
   assert.equal(await tab.evaluate(()=>window.__panelRoot.querySelector('button').textContent),messages(locale)['Download to DLDR']);
   await tab.screenshot({path:path.join(captures,`panel-${locale}.png`)});
   await tab.evaluate(()=>history.pushState({},'', '/library?work=RJ01760323'));
   await tab.waitForFunction(()=>document.querySelector('#dldr-play-panel').hidden);
   assert.equal(bridge.filter(r=>r.action==='download').length,1);
   await tab.goto('https://evil.invalid/work/RJ01760323');assert.equal(await tab.locator('#dldr-play-panel').count(),0);
   await context.close();
  }
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({passed:true,captures,checks:['paired card actions and download dispatch','playlist queue state','downloaded DLPLAY without local playback','four-locale card layout','sidebar/card account binding','launch failure/retry','no implicit downloads','390px dialog','official origin + SPA routes','closed shadow panel','physical download click','duplicate prevention','status transitions','stale-response rejection','four locales']}));
 } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});

// Production route with isolated native fixtures; never accesses the real profile.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const url = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story&reliability=1`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(),'dld-download-center-'));
const nav = (page,name) => page.locator('aside nav').getByRole('button',{name,exact:true});
const tab = (page,name) => page.locator('.settings-layout > .page-header .tabs').getByRole('button',{name,exact:true});
async function emit(page) {
 await page.evaluate(async()=>{
  const qa=window.__centerQa;
  await window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'dldr-job-event',payload:{sequence:++qa.sequence,eventKind:'updated',jobId:qa.job.id,kind:qa.job.kind,status:qa.job.status,phase:qa.job.phase,progress:qa.job.progress,message:null,log:null,snapshot:qa.job}});
 });
}
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 let page;
 try{
  page=await browser.newPage({viewport:{width:1360,height:1000}});page.setDefaultTimeout(12000);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(url);await page.locator('.product-card').first().waitFor();
  assert.deepEqual(await page.locator('aside nav button').allTextContents(),['Library','Downloaded','Playlists','Settings','DLsite Play']);
  await page.locator('.product-card input:enabled').first().check();
  await nav(page,'Settings').click();
  assert.deepEqual(await page.locator('.settings-layout > .page-header .tabs button').allTextContents(),['General','Download settings','Accounts','Logs','About & updates']);
  await tab(page,'Accounts').click();await page.locator('.accounts-panel').waitFor();
  assert.equal(await page.locator('.accounts-panel h1').count(),0);
  await page.locator('.account-row').first().getByRole('button',{name:'Edit',exact:true}).click();await page.locator('dialog').waitFor();
  await page.keyboard.press('Escape');await page.locator('dialog').waitFor({state:'hidden'});
  await tab(page,'Logs').click();assert.equal(await page.locator('.logs-layout h2').first().innerText(),'Logs');
  await page.getByRole('textbox',{name:'Search logs',exact:true}).fill('no-match-probe');
  await page.getByText('No matching events',{exact:true}).waitFor();
  await tab(page,'Download settings').click();
  await page.getByRole('combobox',{name:'Global simultaneous works',exact:true}).selectOption('1');
  await page.getByRole('form',{name:'Parallel downloads',exact:true}).getByRole('button',{name:'Apply limit',exact:true}).click();
  await page.waitForFunction(async()=>await window.__TAURI_INTERNALS__.invoke('get_transfer_settings')===1);
  await nav(page,'Library').click();await page.locator('.product-card input:checked').waitFor();
  await nav(page,'Settings').click();await page.getByRole('form',{name:'Parallel downloads',exact:true}).waitFor();
  assert.equal(await page.getByRole('combobox',{name:'Global simultaneous works',exact:true}).inputValue(),'1');
  await nav(page,'Library').click();
  await page.evaluate(async()=>{
   const original=window.__TAURI_INTERNALS__.invoke;
   const entries=await original('list_download_queue');
   const MiB=1024**2;
   const qa=window.__centerQa={sequence:0,calls:[],failQueue:false,entries,job:{
    id:'qa-center',kind:'bulkWorkDownload',title:'Queue test',status:'running',phase:'bulkDownloading',progress:{current:1,total:4,unit:'items'},metadata:{requestedCount:4,maxConcurrentWorks:2},output:null,error:null,cancellable:true,createdAt:new Date().toISOString(),startedAt:new Date().toISOString(),finishedAt:null,
    progressDetails:{download:{totalWorks:4,completedWorks:1,succeededWorks:1,failedWorks:0,bytesReceived:160*MiB,activeWorks:entries.slice(0,2).map((entry,i)=>({workId:entry.workId,title:entry.title,phase:'downloading',bytesReceived:(i+1)*32*MiB,bytesTotal:i?null:128*MiB,bytesPerSecond:(i+1)*MiB,rateUpdatedAt:Date.now()}))}}
   }};
   qa.entries[0].status='running';qa.entries[1].status='running';
   window.__TAURI_INTERNALS__.invoke=async(command,args,...rest)=>{
    qa.calls.push({command,args});
    if(command==='list_jobs')return [qa.job];
    if(command==='list_download_queue'){if(qa.failQueue)throw new Error('Queue read failed (simulated)');return structuredClone(qa.entries);}
    if(command==='control_download_queue'){
     const targets=args.action==='resumeAll'?qa.entries.filter(e=>e.status==='paused'):qa.entries.filter(e=>e.workId===args.workId);
     for(const entry of targets)entry.status=args.action==='pause'?'paused':args.action==='cancel'?'cancelled':'queued';return;
    }
    return original(command,args,...rest);
   };
  });
  const status=page.locator('.download-status');
  await status.click();const panel=page.locator('.download-panel');
  await panel.locator('footer').getByRole('button',{name:'Reload',exact:true}).click();await emit(page);
  const first=panel.getByRole('article',{name:'Queue RJ15000000',exact:true});
  await first.getByRole('progressbar').waitFor();
  assert.equal(await first.getByRole('progressbar').getAttribute('value'),'25');
  assert.equal(await panel.getByRole('article',{name:'Queue RJ15000001',exact:true}).getByRole('progressbar').getAttribute('value'),null);
  await first.getByText('1.00 MiB/s',{exact:true}).waitFor();
  await page.screenshot({path:path.join(captures,'progress-desktop.png')});
  // Background content stays usable: no backdrop, focus trap or query reset.
  await page.getByRole('combobox',{name:'Search library',exact:true}).fill('Observatory');
  await page.getByRole('button',{name:'Search',exact:true}).click();
  await page.waitForFunction(()=>[...document.querySelectorAll('.product-card')].every(el=>el.textContent.includes('Observatory')));
  assert.equal(await panel.isVisible(),true);
  await panel.getByRole('button',{name:'Pause all',exact:true}).click();
  await status.getByText('2 paused',{exact:true}).waitFor();
  assert.deepEqual(await page.evaluate(()=>window.__centerQa.calls.filter(c=>c.command==='control_download_queue').map(c=>c.args)),[{workId:'RJ15000000',action:'pause'},{workId:'RJ15000001',action:'pause'}]);
  await panel.getByRole('button',{name:'Resume all paused',exact:true}).click();
  await first.getByText(/Queued/).waitFor();
  await first.getByRole('button',{name:'Cancel',exact:true}).click();
  await first.waitFor({state:'hidden'});
  await panel.locator('header button').click();
  assert.equal(await status.getAttribute('aria-expanded'),'false');
  // The queue observer keeps failures visible while the panel is closed.
  await page.evaluate(()=>window.__centerQa.failQueue=true);
  await status.getByText('Download queue needs attention',{exact:true}).waitFor();
  await status.click();await panel.getByRole('alert').getByText('Queue read failed (simulated)',{exact:true}).waitFor();
  await page.evaluate(()=>window.__centerQa.failQueue=false);
  await panel.getByRole('alert').getByRole('button',{name:'Reload',exact:true}).click();await panel.getByRole('alert').waitFor({state:'hidden'});
  await panel.locator('header button').focus();await page.keyboard.press('Escape');
  assert.equal(await panel.count(),0);assert.equal(await status.evaluate(el=>el===document.activeElement),true);
  await status.click();await panel.locator('footer').getByRole('button',{name:'Logs',exact:true}).click();
  assert.equal(await panel.count(),0);assert.equal(await tab(page,'Logs').getAttribute('aria-pressed'),'true');
  await status.click();await panel.locator('footer').getByRole('button',{name:'Download settings',exact:true}).click();
  assert.equal(await tab(page,'Download settings').getAttribute('aria-pressed'),'true');
  await page.setViewportSize({width:390,height:844});await status.click();
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  const box=await panel.boundingBox();assert.ok(box.x>=0 && box.x+box.width<=390);assert.ok(box.y>=0 && box.y+box.height<=844);
  await page.screenshot({path:path.join(captures,'progress-narrow.png')});
  assert.deepEqual(errors,[]);
  assert.equal(await page.evaluate(()=>window.__centerQa.calls.some(c=>/start_.*download|save_account|start_account_sync/.test(c.command))),false);
  console.log(JSON.stringify({passed:true,captures}));
 }catch(error){if(page)await page.screenshot({path:path.join(captures,'failure.png')}).catch(()=>{});console.error(captures);throw error;}
 finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});

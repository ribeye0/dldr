// Exercise the production Logs view with isolated, typed native fixtures.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const captures=fs.mkdtempSync(path.join(os.tmpdir(),'dldr-logs-'));
const url=`${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story&reliability=1`;
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 let page; const errors=[];
 try{
  page=await browser.newPage({viewport:{width:1360,height:1000}});
  page.setDefaultTimeout(12000);
  page.on('pageerror',e=>errors.push(e.message));
  await page.goto(url);await page.locator('.product-card').first().waitFor();
  await page.evaluate(()=>{
   const original=window.__TAURI_INTERNALS__.invoke;
   const runId='run-11111111-1111-4111-8111-111111111111',operationId='op-22222222-2222-4222-8222-222222222222';
   const base={at:'2026-11-18T09:15:32.135Z',runId,operationId,component:'app',outcome:'failed',level:'error',operation:'start_work_download',kind:'diagnostic',stage:'cause',errorCode:'storage',durationMs:32};
   const events=[
    {...base,sequence:4,message:'Queue download: failed',errorMessage:'SQLite rejected a value because it violates a CHECK constraint.',hint:"Export this operation's diagnostics with the steps needed to reproduce it.",details:{workId:'RJ01760323',databaseCode:'275',databaseKind:'CheckViolation'}},
    {...base,sequence:3,operation:'list_products',outcome:'succeeded',level:'info',kind:'finished',stage:'result',message:'Load library: completed',errorMessage:null,errorCode:null,details:{count:707}},
    {...base,sequence:2,operation:'job.phase',level:'info',outcome:'succeeded',message:'Task stage changed: completed',errorMessage:null,errorCode:null,details:{workId:'RJ01760323',phase:'extracting'}},
    {...base,sequence:1,operation:'work.download',level:'warn',outcome:'cancelled',message:'Download work: cancelled',errorMessage:null,errorCode:null,details:{workId:'RJ01647600'}},
   ];
   window.__logsQa={events,calls:0,runId,operationId};
   window.__TAURI_INTERNALS__.invoke=async(command,args,...rest)=>{
    if(command==='list_audit_events'){window.__logsQa.calls++;return args?.request?.runId?[{...events[0],runId:args.request.runId,details:{workId:'RJ00000001',httpStatus:503},errorMessage:'DLsite returned a server error.'}]:structuredClone(events);}
    if(command==='list_jobs')return [];
    if(command==='diagnostic_runs')return [{runId,current:true},{runId:'run-33333333-3333-4333-8333-333333333333',current:false,startedAt:'2026-11-17T08:00:00Z'}];
    if(command==='diagnostic_operation')return events.filter(e=>e.operationId===args.operationId);
    return original(command,args,...rest);
   };
  });
  await page.locator('aside nav button[data-view=settings]').click();
  await page.locator("#ui-locale").waitFor();
  await page.waitForFunction(()=>!!window.__logsQa);
  const tabs=page.locator('.settings-layout > .page-header .tabs');
  await tabs.getByRole('button',{name:'Logs',exact:true}).click();
  const timeline=page.getByRole('list',{name:'Logs timeline'});
  await page.waitForFunction(()=>document.querySelector('.logs-layout')?.textContent.includes('CHECK constraint'));
  assert.equal(await page.getByRole('button',{name:/Work history|Application logs/}).count(),0);
  assert.equal(await timeline.locator('.log-record').count(),3);
  await timeline.locator('.log-row').first().click();
  await page.getByText('Suggested action',{exact:true}).waitFor();
  assert.match(await timeline.innerText(),/275/);
  assert.match(await timeline.innerText(),/RJ01760323/);
  await page.getByRole('button',{name:'View complete operation',exact:true}).click();
  await page.getByRole('dialog').getByRole('button',{name:'Copy log entry'}).first().waitFor();
  await page.keyboard.press('Escape');
  const search=page.getByRole('textbox',{name:'Search logs',exact:true});
  await search.fill('275');assert.equal(await timeline.locator('.log-record').count(),1);
  await search.fill('rj01647600');assert.equal(await timeline.locator('.log-record').count(),1);
  await search.fill('nonexistent');await page.getByText('No matching events',{exact:true}).waitFor();
  await search.fill('');
  await page.getByRole('checkbox',{name:'Include routine queries and operation starts'}).check();
  assert.equal(await timeline.locator('.log-record').count(),4);
  await page.getByRole('group',{name:'Logs filter'}).getByRole('button',{name:'Errors',exact:true}).click();
  assert.equal(await timeline.locator('.log-record').count(),1);
  await page.getByRole('combobox',{name:'Run',exact:true}).selectOption('run-33333333-3333-4333-8333-333333333333');
  await page.getByText('DLsite returned a server error.',{exact:true}).waitFor();
  assert.match(await timeline.innerText(),/RJ00000001/);
  await page.getByRole('combobox',{name:'Run',exact:true}).selectOption('');
  await page.getByText('SQLite rejected a value because it violates a CHECK constraint.',{exact:true}).waitFor();
  const before=await page.evaluate(()=>window.__logsQa.calls);
  await page.waitForFunction(n=>window.__logsQa.calls>n,before,{timeout:8000});
  assert.equal(await timeline.locator('.log-record').count(),1);
  await page.screenshot({path:path.join(captures,'logs-light.png')});
  for(const language of ['zh-TW','zh-CN','ja']){
   await tabs.getByRole('button',{name:/General|一般|常规|全般/,exact:true}).click();
   await page.locator('#ui-locale').selectOption(language);
   await tabs.locator('button').nth(3).click();
   await page.locator('.logs-layout .log-record').first().waitFor();
   assert(!await page.locator('.logs-layout').innerText().then(s=>s.includes('One timeline for downloads')));
  }
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({passed:true,captures,checks:['unified timeline','nested error search','routine filter','complete operation','historical run','live refresh','four languages']}));
 } catch(error) { if(page){await page.screenshot({path:path.join(captures,"failure.png")});console.error("page errors:",errors);console.error(await page.locator("body").innerText());console.error(await page.evaluate(()=>window.__logsQa));} throw error; } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});

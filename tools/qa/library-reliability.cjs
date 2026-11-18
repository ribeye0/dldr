// Production route with isolated IPC and dialogs; no real downloads/files/accounts.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const base = process.env.STORYBOOK_URL || 'http://localhost:6006';
const url = `${base}/iframe.html?id=app-production-workspace--interactive&viewMode=story&reliability=1`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(),'dlsite-reliability-qa-'));
const nav = (p,name) => p.getByRole('navigation',{name:'Main',exact:true}).getByRole('button',{name:name==='Downloads'?/^Downloads(?: \d+)?$/:name,exact:name!=='Downloads'});
async function installProbe(page) {
  await page.evaluate(() => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__reliabilityQa={requests:[],lowSpace:false,failRestore:false};
    window.__TAURI_INTERNALS__.invoke=async function(command,args,...rest) {
      window.__reliabilityQa.requests.push({command,args:JSON.parse(JSON.stringify(args??{}))});
      if(command==='restore_library_backup' && window.__reliabilityQa.failRestore) throw new Error('Invalid backup (simulated)');
      const result=await invoke.call(this,command,args,...rest);
      if(command==='check_download_space' && window.__reliabilityQa.lowSpace) result.budgets[0]={...result.budgets[0],sufficient:false,availableBytes:1024};
      return result;
    };
  });
}
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try {
  const page=await browser.newPage({viewport:{width:1280,height:900}});page.setDefaultTimeout(12000);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(url);await page.getByText('Page 1 of 8',{exact:true}).first().waitFor();await installProbe(page);
  await page.getByRole('button',{name:'Filters',exact:true}).click();
  const genre=page.getByRole('button',{name:'Genre Whispering：Any',exact:true});await genre.click();
  await page.getByRole('button',{name:'Genre Whispering：Include',exact:true}).waitFor();
  await page.waitForFunction(()=>[...document.querySelectorAll('.product-card')].length>0 && [...document.querySelectorAll('.product-card')].every(c=>c.querySelector('.product-tags')?.textContent.includes('Whispering')));
  await page.getByRole('button',{name:'Genre Whispering：Include',exact:true}).click();
  await page.getByRole('button',{name:'Genre Whispering：Exclude',exact:true}).waitFor();
  await page.waitForFunction(()=>[...document.querySelectorAll('.product-card')].length>0 && [...document.querySelectorAll('.product-card')].every(c=>!c.querySelector('.product-tags')?.textContent.includes('Whispering')));
  await page.getByRole('button',{name:'Genre Whispering：Exclude',exact:true}).click();
  await page.getByRole('button',{name:'Voice actor あかり（サンプル）：Any',exact:true}).click();
  await page.getByRole('button',{name:'Voice actor あかり（サンプル）：Include',exact:true}).waitFor();
  await page.waitForFunction(()=>[...document.querySelectorAll('.product-card')].length>0 && [...document.querySelectorAll('.product-card')].every(c=>c.querySelector('.card-credits')?.textContent.includes('あかり')));
  const queries=await page.evaluate(()=>window.__reliabilityQa.requests.filter(r=>r.command==='list_products').map(r=>r.args.request));
  assert.ok(queries.some(q=>q.genreNames?.includes('低語')));assert.ok(queries.some(q=>q.excludedGenreNames?.includes('低語')));assert.ok(queries.some(q=>q.voiceNames?.includes('あかり（サンプル）')));
  await page.getByRole('button',{name:'Voice actor あかり（サンプル）：Include',exact:true}).click();
  await page.getByRole('button',{name:'Voice actor あかり（サンプル）：Exclude',exact:true}).click();
  await page.getByRole('button',{name:'Filters',exact:true}).click();
  await page.locator('.product-card input[type=checkbox]:enabled').first().check();
  await page.evaluate(()=>window.__reliabilityQa.lowSpace=true);
  await page.getByRole('button',{name:/^Download selected \(/}).click();
  await page.getByRole('dialog').waitFor();await page.getByRole('region',{name:'Disk space estimate'}).waitFor();
  assert.equal(await page.getByRole('button',{name:'Start Download',exact:true}).isDisabled(),true);
  await page.getByRole('button',{name:'Cancel',exact:true}).click();
  await page.evaluate(()=>window.__reliabilityQa.lowSpace=false);
  await page.getByRole('button',{name:/^Download selected \(/}).click();
  await page.getByRole('dialog').waitFor();assert.equal(await page.getByRole('button',{name:'Start Download',exact:true}).isEnabled(),true);
  await page.getByRole('button',{name:'Cancel',exact:true}).click();
  await page.locator('.download-status').click();const queue=page.getByRole('region',{name:'Saved download queue'});await queue.waitFor();await queue.getByRole('combobox',{name:'Saved queue status'}).selectOption('all');
  const first=queue.getByRole('article',{name:'Queue RJ15000000',exact:true});
  await first.getByRole('button',{name:'Pause',exact:true}).click();await first.getByText(/Paused/).waitFor();
  await first.getByRole('button',{name:'Resume',exact:true}).click();await first.getByText(/Queued/).waitFor();
  await first.getByRole('button',{name:'Cancel',exact:true}).click();await first.getByText(/Cancelled/).waitFor();
  await queue.getByRole('button',{name:'Retry all failed',exact:true}).click();await queue.getByRole('article',{name:'Queue RJ15000002',exact:true}).getByText(/Queued/).waitFor();
  await queue.getByRole('button',{name:'Clear completed / cancelled',exact:true}).click();
  assert.equal(await queue.getByRole('article').count(),2);
  await page.screenshot({path:path.join(captures,'queue-desktop.png')});
  await page.reload();await page.getByText('Page 1 of 8',{exact:true}).first().waitFor();await installProbe(page);await page.locator('.download-status').click();
  await queue.getByRole('article',{name:'Queue RJ15000002',exact:true}).getByText(/Paused/).waitFor();
  await queue.getByRole('button',{name:'Resume all paused',exact:true}).click();
  await page.waitForFunction(()=>[...document.querySelectorAll('[aria-label="Saved download queue"] article')].every(e=>e.textContent.includes('Queued')));
  await page.locator('.download-panel header button').click();
  await nav(page,'Settings').click();await page.getByRole('region',{name:'Library maintenance'}).waitFor();
  await page.getByRole('button',{name:'Verify downloaded files',exact:true}).click();await page.getByText(/Checked 12 works/).waitFor();
  await page.getByRole('button',{name:'Export backup',exact:true}).click();await page.getByText(/Backup saved/).waitFor();
  await page.getByRole('button',{name:'Import backup',exact:true}).click();await page.getByRole('dialog').waitFor();
  assert.equal((await page.evaluate(()=>window.__reliabilityQa.requests)).filter(r=>r.command==='restore_library_backup').length,0);
  await page.getByRole('button',{name:'Cancel',exact:true}).click();
  await page.getByRole('button',{name:'Import backup',exact:true}).click();await page.getByRole('button',{name:'Confirm import',exact:true}).click();await page.getByText(/Imported 42 records/).waitFor();
  await page.evaluate(()=>window.__reliabilityQa.failRestore=true);await page.getByRole('button',{name:'Import backup',exact:true}).click();await page.getByRole('button',{name:'Confirm import',exact:true}).click();await page.getByRole('alert').filter({hasText:'Invalid backup'}).waitFor();
  await page.screenshot({path:path.join(captures,'maintenance.png')});
  await page.getByRole('button',{name:'About & updates',exact:true}).click();
  assert.equal(await page.getByRole('button',{name:'Check for Updates',exact:true}).count(),0);await page.getByText(/Upstream binary updates are disabled/).waitFor();
  await page.setViewportSize({width:390,height:844});await page.locator('.download-status').click();await queue.waitFor();await queue.scrollIntoViewIfNeeded();await page.screenshot({path:path.join(captures,'queue-narrow.png')});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth),false);
  assert.deepEqual(errors,[]);console.log(`Library reliability QA passed: ${captures}`);
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exit(1);});

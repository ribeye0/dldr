// Isolated IPC: never downloads a release or replaces an installed application.
const assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE_PATH||'playwright');
const captures=fs.mkdtempSync(path.join(os.tmpdir(),'dldr-updater-'));
const url=`${process.env.STORYBOOK_URL||'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try{
  for(const [locale,title,check,install,retry] of [
   ['en','Update DLDR','Check for updates','Update and restart','Retry'],
   ['zh-TW','更新 DLDR','檢查更新','更新並重新啟動','重試'],
   ['zh-CN','更新 DLDR','检查更新','更新并重新启动','重试'],
   ['ja','DLDR を更新','更新を確認','更新して再起動','再試行'],
  ]){
   const page=await browser.newPage({viewport:{width:locale==='en'?1280:390,height:900}});
   const errors=[];page.on('pageerror',error=>errors.push(error.message));
   await page.addInitScript(locale=>{
    sessionStorage.setItem('dldr-production-story-settings',JSON.stringify({uiLocale:locale}));
    localStorage.setItem('dldr.color-theme',locale==='zh-CN'?'dark':'light');
   },locale);
   await page.goto(url);await page.locator('.product-card').first().waitFor();
   await page.evaluate(()=>{
    const invoke=window.__TAURI_INTERNALS__.invoke;
    const qa=window.__updateQa={checks:0,installs:0,kind:'windowsPortable',release:{currentVersion:'0.1.1',version:'0.2.0',notes:'A short change list.\n<script>not HTML</script>'},checkError:null,installError:null,urls:[]};
    window.__TAURI_INTERNALS__.invoke=async(command,args,...rest)=>{
     if(command==='check_app_update'){qa.checks++;if(qa.checkError)throw qa.checkError;return {kind:qa.kind,release:qa.release};}
     if(command==='install_app_update'){
      qa.installs++;if(qa.installError)throw qa.installError;
      qa.progress=args.onEvent.onmessage;
      return new Promise((resolve,reject)=>{qa.resolve=resolve;qa.reject=reject;});
     }
     if(command==='open_external_url'){qa.urls.push(args.url);return;}
     return invoke(command,args,...rest);
    };
   });
   await page.locator('aside nav button[data-view=settings]').click();
   await page.locator('.settings-layout .tabs button').last().click();
   const checkButton=page.getByRole('button',{name:check,exact:true});
   await checkButton.click();
   const dialog=page.getByRole('dialog',{name:title,exact:true});await dialog.waitFor();
   const installButton=dialog.getByRole('button',{name:install,exact:true});await installButton.waitFor();
   assert.equal(await page.evaluate(()=>window.__updateQa.installs),0);
   assert.equal(await dialog.locator('details').getAttribute('open'),null);
   await dialog.locator('summary').click();assert.ok((await dialog.innerText()).includes('<script>not HTML</script>'));
   assert.equal(await dialog.locator('script').count(),0);
   await dialog.screenshot({path:path.join(captures,`${locale}-available.png`)});
   assert.equal(await dialog.evaluate(node=>node.scrollWidth>node.clientWidth),false);
   await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});
   await checkButton.click();await installButton.click();
   await page.waitForFunction(()=>typeof window.__updateQa.progress==='function');
   await page.evaluate(()=>window.__updateQa.progress({event:'progress',downloaded:1024,total:null}));
   await dialog.locator('progress').waitFor();assert.equal(await dialog.locator('progress').getAttribute('value'),null);
   await page.evaluate(()=>window.__updateQa.progress({event:'progress',downloaded:5000,total:10000}));
   await page.waitForFunction(()=>document.querySelector('dialog progress')?.value===50);
   await page.keyboard.press('Escape');assert.equal(await dialog.isVisible(),true);
   await page.evaluate(()=>window.__updateQa.progress({event:'verifying'}));
   assert.equal(await dialog.locator('progress').count(),0);
   await page.evaluate(()=>window.__updateQa.reject('update_signature'));
   await dialog.getByRole('button',{name:retry,exact:true}).waitFor();
   await dialog.screenshot({path:path.join(captures,`${locale}-error.png`)});
   await dialog.getByRole('button',{name:retry,exact:true}).click();await installButton.waitFor();
   await page.evaluate(()=>window.__updateQa.installError='update_jobs_active');await installButton.click();
   await dialog.getByRole('button',{name:retry,exact:true}).waitFor();
   await page.evaluate(()=>{window.__updateQa.checkError='update_feed_unavailable';});
   await dialog.getByRole('button',{name:retry,exact:true}).click();
   await dialog.getByRole('button',{name:retry,exact:true}).waitFor();
   await page.evaluate(()=>{window.__updateQa.checkError=null;window.__updateQa.release=null;});
   await dialog.getByRole('button',{name:retry,exact:true}).click();
   await page.waitForFunction(()=>document.querySelector('dialog')?.textContent&&!document.querySelector('dialog details')&&!document.querySelector('dialog .error'));
   assert.equal(await installButton.count(),0);
   await page.keyboard.press('Escape');
   await page.evaluate(()=>window.__updateQa.kind='unsupported');await checkButton.click();
   await dialog.getByRole('button',{name:'GitHub Releases ↗',exact:true}).click();
   assert.deepEqual(await page.evaluate(()=>window.__updateQa.urls),['https://github.com/ribeye0/dldr/releases']);
   assert.deepEqual(errors,[]);await page.close();
  }
  console.log(JSON.stringify({passed:true,captures}));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});

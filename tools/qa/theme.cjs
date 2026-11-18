// Production UI / browser storage fixtures only. Never accesses a real profile or media.
const assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const story = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(), 'dldr-theme-qa-'));
const key = 'dldr.color-theme';
const nav = (page, view) => page.locator(`aside nav button[data-view=${view}]`);
const resolved = (page, value) => page.waitForFunction(value => document.documentElement.dataset.theme === value, value);
async function contrast(page, selectors) {
 const values = await page.evaluate(selectors => {
  const canvas = document.createElement('canvas');canvas.width=canvas.height=1;
  const ctx = canvas.getContext('2d');
  function rgba(color) {ctx.clearRect(0,0,1,1);ctx.fillStyle=color;ctx.fillRect(0,0,1,1);return Array.from(ctx.getImageData(0,0,1,1).data);}
  function background(element) {
   const color=rgba(getComputedStyle(element).backgroundColor);
   if(color[3]===255)return color;
   const base=element.parentElement?background(element.parentElement):[255,255,255,255],alpha=color[3]/255;
   return color.map((c,i)=>i===3?255:c*alpha+base[i]*(1-alpha));
  }
  function luminance(rgb) {return rgb.slice(0,3).map(c=>c/255).map(c=>c<=.04045?c/12.92:((c+.055)/1.055)**2.4).reduce((sum,c,i)=>sum+c*[.2126,.7152,.0722][i],0);}
  return selectors.map(selector=>{
   const element=document.querySelector(selector);if(!element)throw new Error(`Missing contrast target: ${selector}`);
   const a=luminance(rgba(getComputedStyle(element).color)),b=luminance(background(element));
   return {selector,ratio:(Math.max(a,b)+.05)/(Math.min(a,b)+.05)};
  });
 }, selectors);
 for(const result of values)assert.ok(result.ratio>=4.5,JSON.stringify(result));
 return values;
}
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try {
  const context=await browser.newContext({viewport:{width:1360,height:1000},colorScheme:'dark'});
  const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(story);await page.locator('.product-card').first().waitFor();
  await page.evaluate(()=>{
   const original=window.__TAURI_INTERNALS__.invoke;window.__headerThemes=[];
   window.__TAURI_INTERNALS__.invoke=(command,args,...rest)=>{if(command==='set_native_header_theme')window.__headerThemes.push(args.dark);return original(command,args,...rest)};
  });
  await resolved(page,'light'); // Existing installations retain the original light default.
  await nav(page,'settings').click();await page.locator('#ui-theme').selectOption('system');await resolved(page,'dark');
  await page.emulateMedia({colorScheme:'light'});await resolved(page,'light');
  await page.emulateMedia({colorScheme:'dark'});await resolved(page,'dark');
  await page.locator('#ui-theme').selectOption('light');await resolved(page,'light');
  await page.emulateMedia({colorScheme:'dark'});await resolved(page,'light');
  await page.locator('#ui-theme').selectOption('dark');await resolved(page,'dark');
  assert.deepEqual(await page.evaluate(()=>window.__headerThemes),[true,false,true,false,true]);
  await page.reload();await page.locator('.product-card').first().waitFor();await resolved(page,'dark');
  const other=await context.newPage();await other.goto(story);await other.locator('.product-card').first().waitFor();await resolved(other,'dark');
  await nav(page,'settings').click();assert.equal(await page.locator('#ui-theme').inputValue(),'dark');
  await page.locator('#ui-theme').selectOption('light');await resolved(other,'light');
  await page.locator('#ui-theme').selectOption('dark');await resolved(other,'dark');await other.close();
  const ratios=await contrast(page,['h1','#ui-theme','#ui-theme-hint','#ui-locale','.language-rank','.language-priority-row .ui-button:not(:disabled)']);
  await page.screenshot({path:path.join(captures,'settings-dark.png')});
  for(const section of ['Download settings','Accounts','Logs','About & updates']){
   await page.getByRole('group',{name:'Settings section',exact:true}).getByRole('button',{name:section,exact:true}).click();
   await page.screenshot({path:path.join(captures,`settings-${section.replaceAll(' ','-')}.png`)});
  }
  await nav(page,'library').click();
  ratios.push(...await contrast(page,['.product-card .card-title','.product-card .card-credits dt','.product-card .voice-chip','.product-card .product-tag','.product-card .product-page-link','.product-card .online-play','.badge.kind','.badge.age']));
  await page.screenshot({path:path.join(captures,'library-dark.png')});
  await page.getByRole('button',{name:'Filters',exact:true}).click();await page.locator('#library-filter-grid').waitFor();
  const filterSelectors=['[data-type-filter=audio]','[data-type-filter=video]','[data-type-filter=game]','[data-type-filter=image]','[data-age-filter=all]','[data-age-filter=r15]','[data-age-filter=r18]','[data-source-filter=owned]','[data-source-filter=localOnly]'];
  ratios.push(...await contrast(page,filterSelectors));
  for(const selector of ['[data-type-filter=audio]','[data-age-filter=r18]']){
   await page.locator(selector).click();await page.waitForTimeout(150);ratios.push(...await contrast(page,[selector]));
   await page.mouse.move(0,0);await page.waitForTimeout(150);ratios.push(...await contrast(page,[selector]));
  }
  await page.screenshot({path:path.join(captures,'filters-dark.png')});
  await page.getByRole('button',{name:'Reset filters',exact:true}).click();
  const filters=page.getByRole('button',{name:'Filters',exact:true});if(await filters.getAttribute('aria-expanded')==='true')await filters.click();
  await page.locator('.product-card').first().waitFor();
  const active=page.locator('.card-download:enabled').first();await active.hover();ratios.push(...await contrast(page,['.card-download:hover']));await page.mouse.move(0,0);
  await page.locator('.product-card .card-title').first().click();const dialog=page.locator('dialog[open]');await dialog.waitFor();
  ratios.push(...await contrast(page,['dialog[open]','dialog[open] .voice-chip']));await page.screenshot({path:path.join(captures,'details-dark.png')});await page.keyboard.press('Escape');
  await page.locator('.download-status').click();await page.locator('.download-panel').waitFor();
  ratios.push(...await contrast(page,['.download-panel','.download-panel header p']));await page.screenshot({path:path.join(captures,'downloads-dark.png')});await page.keyboard.press('Escape');
  if(await page.locator('.download-panel').count())await page.locator('.download-status').click();
  await nav(page,'downloaded').click();const downloaded=page.locator('.product-card').first();await downloaded.waitFor();
  ratios.push(...await contrast(page,['.work-folder']));await downloaded.screenshot({path:path.join(captures,'downloaded-card-dark.png')});
  await downloaded.locator('.playlist-toggle').click({button:'right'});await page.locator('.playlist-menu').waitFor();await page.screenshot({path:path.join(captures,'playlist-menu-dark.png')});await page.keyboard.press('Escape');
  await downloaded.locator('.card-play-target').click({position:{x:6,y:6}});await page.locator('.player-panel').waitFor();
  await page.locator('.player-panel .primary-control').click(); // Mock player only.
  await page.locator('.script-lines button').first().waitFor();await page.locator('.script-lines button').first().click();
  await page.locator('.script-lines button.active').first().waitFor();
  ratios.push(...await contrast(page,['.player-panel','.primary-control','.script-lines button.active']));
  await page.screenshot({path:path.join(captures,'player-dark.png')});await page.keyboard.press('Escape');
  await nav(page,'settings').click();await page.getByRole('group',{name:'Settings section',exact:true}).getByRole('button',{name:'General',exact:true}).click();
  for(const language of ['zh-TW','zh-CN','ja','en']){
   await page.locator('#ui-locale').selectOption(language);assert.equal(await page.locator('#ui-theme option').count(),3);
   await page.setViewportSize({width:390,height:844});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
   await page.locator('#ui-theme').screenshot({path:path.join(captures,`theme-${language}.png`)});await page.setViewportSize({width:1360,height:1000});
  }
  await page.locator('#ui-locale').selectOption('zh-TW');await nav(page,'library').click();await page.mouse.move(0,0);await page.screenshot({path:path.join(captures,'library-dark-zh-TW.png')});
  await nav(page,'settings').click();await page.locator('#ui-theme').selectOption('light');await nav(page,'library').click();await resolved(page,'light');
  await contrast(page,['h1','.card-title','.voice-chip','.product-tag','.online-play']);await page.screenshot({path:path.join(captures,'library-light.png')});
  await page.evaluate(key=>localStorage.setItem(key,'invalid-preference'),key);await page.reload();await page.locator('.product-card').first().waitFor();await resolved(page,'light');
  assert.deepEqual(errors,[]);await context.close();
  // Check the exact production head bootstrap without Svelte loading: no white flash.
  const html=fs.readFileSync(path.join(__dirname,'../../apps/desktop/src/app.html'),'utf8');
  const bootstrap=html.match(/<script>([\s\S]*?)<\/script>/)[1],critical=html.match(/<style>([\s\S]*?)<\/style>/)[1];
  for(const [preference,system,expected] of [['dark','light','dark'],['system','dark','dark'],['system','light','light'],['unknown','dark','light']]){
   const ctx=await browser.newContext({colorScheme:system});await ctx.addInitScript(({key,preference})=>localStorage.setItem(key,preference),{key,preference});
   const tab=await ctx.newPage();await tab.route('https://theme.fixture/',r=>r.fulfill({contentType:'text/html',body:`<!doctype html><html><head><script>${bootstrap}</script><style>${critical}</style></head><body></body></html>`}));
   await tab.goto('https://theme.fixture/');await resolved(tab,expected);assert.equal(await tab.evaluate(()=>getComputedStyle(document.documentElement).colorScheme),expected);if(expected==='dark')assert.equal(await tab.evaluate(()=>getComputedStyle(document.body).backgroundColor),'rgb(12, 20, 36)');await ctx.close();
  }
  const denied=await browser.newContext();await denied.addInitScript(()=>Object.defineProperty(window,'localStorage',{get(){throw new Error('Storage denied')}}));
  const tab=await denied.newPage();await tab.goto(story);await tab.locator('.product-card').first().waitFor();await nav(tab,'settings').click();await tab.locator('#ui-theme').selectOption('dark');await resolved(tab,'dark');await denied.close();
  console.log(JSON.stringify({passed:true,captures,minTextContrast:Math.min(...ratios.map(r=>r.ratio)),checks:['native-header theme notification sequence','immediate three-way switch','live system changes and explicit override','reload persistence','cross-window storage','invalid / denied storage','before-paint bootstrap','card/voice/tag/modal/download/player contrast','all settings sections','four locales and narrow window','light regression']}));
 }finally{await browser.close()}
})().catch(error=>{console.error(error);process.exitCode=1});

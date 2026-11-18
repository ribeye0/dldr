// Production route and isolated persistent IPC fixtures; no real library/media.
const assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE_PATH||'playwright');
const url=`${process.env.STORYBOOK_URL||'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures=fs.mkdtempSync(path.join(os.tmpdir(),'dldr-playlists-qa-'));
const nav=(page,view)=>page.locator(`aside nav button[data-view=${view}]`);
const snapshot=page=>page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('list_playlists'));
async function editor(page,title,name){
 const dialog=page.getByRole('dialog',{name:title,exact:true});await dialog.waitFor();
 await dialog.getByRole('textbox',{name:'Playlist name',exact:true}).fill(name);
 await dialog.getByRole('button',{name:'Save changes',exact:true}).click();await dialog.waitFor({state:'hidden'});
}
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}});page.setDefaultTimeout(12000);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(url);await page.locator('.product-card').first().waitFor();
  await page.evaluate(()=>{
   const original=window.__TAURI_INTERNALS__.invoke;
   const probe=window.__playlistQa={folders:[],opens:0,downloads:0,deleted:0,fail:false};
   window.__TAURI_INTERNALS__.invoke=async(command,args,...rest)=>{
    if(command==='open_work_download')probe.folders.push(args.request.workId);
    if(command==='open_player')probe.opens++;
    if(command==='delete_work_download')probe.deleted++;
    if(['start_work_download','start_bulk_work_download'].includes(command))probe.downloads++;
    if(command==='edit_playlist'&&probe.fail){probe.fail=false;throw new Error('Simulated playlist failure');}
    return original(command,args,...rest);
   };
  });
  assert.deepEqual(await page.locator('aside nav button').allTextContents(),['Library','Downloaded','Playlists','Settings','DLsite Play']);
  // Undownloaded works can be bookmarked, but gain no playback/folder action.
  const pending=page.locator('.product-card:not(:has(.card-play-target))').first();
  const pendingId=await pending.locator('.work-id').innerText();
  await pending.locator('.playlist-toggle').click();
  assert.deepEqual((await snapshot(page))[0].workIds,[pendingId]);
  assert.equal(await pending.locator('.work-folder').count(),0);
  assert.equal(await pending.locator('.card-play-target').count(),0);
  const status=pending.locator('.cover-frame [role=img]');
  if(await status.count()) {
   const indicator=await status.boundingBox(),bookmark=await pending.locator('.playlist-toggle').boundingBox();
   assert.ok(indicator.x+indicator.width<=bookmark.x);
  }
  assert.equal(await page.locator('.selection-checkbox:checked').count(),0);
  await page.screenshot({path:path.join(captures,'undownloaded-bookmark.png')});
  await pending.locator('.playlist-toggle').click();
  assert.deepEqual((await snapshot(page))[0].workIds,[]);
  await pending.locator('.playlist-toggle').click();
  await nav(page,'playlists').click();
  await page.locator('.playlists-view .product-card').first().waitFor();
  assert.equal(await page.locator('.playlists-view .work-id').first().innerText(),pendingId);
  assert.equal(await page.locator('.playlists-view .card-play-target').count(),0);
  await page.locator('.playlists-view .playlist-toggle').first().click();
  await page.getByText('No works in this playlist match your search.',{exact:true}).waitFor();
  assert.deepEqual((await snapshot(page))[0].workIds,[]);
  await nav(page,'downloaded').click();await page.locator('.playlist-toggle').first().waitFor();
  const cards=page.locator('.product-card'),first=cards.first();
  const workId=await first.locator('.work-id').innerText();
  const secondId=await cards.nth(1).locator('.work-id').innerText();
  const aligned=await first.evaluate(card=>{
   const id=card.querySelector('.work-id').getBoundingClientRect(),badges=card.querySelector('.work-badges').getBoundingClientRect(),cover=card.querySelector('.cover-frame').getBoundingClientRect();
   const chip=card.querySelector('.voice-chip'),label=card.querySelector('.has-voices dt .card-text');
   const range=document.createRange();range.selectNodeContents(chip);const text=range.getClientRects()[0],caption=label.getBoundingClientRect();
   return {idCenter:(id.top+id.bottom)/2,badgeCenter:(badges.top+badges.bottom)/2,bottom:cover.bottom-id.bottom,voiceOffset:text.top-caption.top};
  });
  assert.ok(Math.abs(aligned.idCenter-aligned.badgeCenter)<1);
  assert.ok(aligned.bottom>=9 && aligned.bottom<=11);
  assert.ok(Math.abs(aligned.voiceOffset)<2);
  assert.equal(await first.locator('.file-meta .work-folder').count(),0);
  const actions=first.locator('.card-actions');
  assert.deepEqual(await actions.locator('button').allTextContents(),['DLPLAY','Open directory']);
  const left=await actions.locator('.online-play').boundingBox(),right=await actions.locator('.work-folder').boundingBox();
  assert.ok(Math.abs(left.width-right.width)<2 && Math.abs(left.y-right.y)<1);
  await first.locator('.work-folder').click();
  assert.deepEqual(await page.evaluate(()=>window.__playlistQa.folders),[workId]);
  assert.equal(await page.evaluate(()=>window.__playlistQa.opens),0);
  const toggle=first.locator('.playlist-toggle');
  await toggle.click();await page.waitForFunction(()=>document.querySelector('.playlist-toggle')?.getAttribute('aria-pressed')==='true');
  await toggle.click();assert.deepEqual((await snapshot(page))[0].workIds,[]);
  await toggle.click();assert.deepEqual((await snapshot(page))[0].workIds,[workId]);
  assert.equal(await page.evaluate(()=>window.__playlistQa.opens),0);
  await toggle.click({button:'right'});
  const menu=page.getByRole('menu',{name:'Playlist membership',exact:true});await menu.waitFor();
  assert.equal(await menu.getByRole('menuitemcheckbox',{name:'Playlist 0',exact:true}).getAttribute('aria-checked'),'true');
  await page.keyboard.press('Escape');await menu.waitFor({state:'hidden'});
  await page.waitForFunction(()=>document.querySelector('.playlist-toggle')===document.activeElement);
  await toggle.focus();await page.keyboard.press('Shift+F10');await menu.waitFor();
  await menu.getByRole('menuitem',{name:'Manage playlists',exact:true}).click();
  await page.getByRole('heading',{name:'Playlists',exact:true}).waitFor();
  await page.locator('.playlists-view .product-card').first().waitFor();
  assert.equal(await page.locator('.playlists-view .product-card').count(),1);
  await page.getByRole('button',{name:'New playlist',exact:true}).click();await editor(page,'New playlist','睡前・おやすみ');
  const list=(await snapshot(page)).find(list=>list.name==='睡前・おやすみ');assert.ok(list);
  await page.getByRole('button',{name:'Rename playlist',exact:true}).click();await editor(page,'Rename playlist','夜の音聲');
  assert.equal((await snapshot(page)).find(p=>p.id===list.id).name,'夜の音聲');
  await nav(page,'downloaded').click();await toggle.waitFor();
  await toggle.click({button:'right'});await menu.getByRole('menuitemcheckbox',{name:'夜の音聲',exact:true}).click();
  await menu.waitFor({state:'hidden'});assert.deepEqual((await snapshot(page)).find(p=>p.id===list.id).workIds,[workId]);
  await toggle.click({button:'right'});
  const custom=menu.getByRole('menuitemcheckbox',{name:'夜の音聲',exact:true});
  assert.equal(await custom.getAttribute('aria-checked'),'true');await custom.click();
  await menu.waitFor({state:'hidden'});assert.deepEqual((await snapshot(page)).find(p=>p.id===list.id).workIds,[]);
  assert.deepEqual((await snapshot(page)).find(p=>p.id==='0').workIds,[workId]);
  await toggle.click({button:'right'});await custom.click();await menu.waitFor({state:'hidden'});
  await nav(page,'playlists').click();await page.locator('.playlists-view select').selectOption(list.id);
  await page.locator('.playlists-view .product-card').first().waitFor();
  await page.getByRole('textbox',{name:'Search this playlist',exact:true}).fill('no matching work');
  await page.locator('.playlist-search').getByRole('button',{name:'Search',exact:true}).click();
  await page.getByText('No works in this playlist match your search.',{exact:true}).waitFor();
  await page.getByRole('textbox',{name:'Search this playlist',exact:true}).fill('');
  await page.locator('.playlist-search').getByRole('button',{name:'Search',exact:true}).click();
  await page.locator('.card-play-target').first().click({position:{x:6,y:6}});
  await page.getByRole('dialog',{name:'Audio player',exact:true}).waitFor();
  assert.equal(await page.locator('.player-panel .primary-control').getAttribute('aria-label'),'Play');
  await page.keyboard.press('Escape');
  await page.getByRole('button',{name:'Remove from playlist',exact:true}).click();
  await page.getByText('No works in this playlist match your search.',{exact:true}).waitFor();
  assert.deepEqual((await snapshot(page)).find(p=>p.id===list.id).workIds,[]);
  await page.locator('.playlists-view select').selectOption('0');
  await page.getByRole('button',{name:'Rename playlist',exact:true}).click();await editor(page,'Rename playlist','いつもの');
  await nav(page,'downloaded').click();await cards.nth(1).locator('.playlist-toggle').click();
  await page.waitForFunction(()=>document.querySelectorAll('.playlist-toggle')[1]?.getAttribute('aria-pressed')==='true');
  assert.deepEqual((await snapshot(page)).find(p=>p.id==='0').workIds,[workId,secondId]);
  await page.evaluate(()=>window.__playlistQa.fail=true);
  await cards.nth(2).locator('.playlist-toggle').click();
  await page.getByText('Simulated playlist failure',{exact:false}).first().waitFor();
  assert.equal((await snapshot(page)).find(p=>p.id==='0').workIds.length,2);
  assert.deepEqual(await page.evaluate(()=>({deleted:window.__playlistQa.deleted,downloads:window.__playlistQa.downloads})),{deleted:0,downloads:0});
  await page.reload();await nav(page,'playlists').click();
  await page.locator('.playlists-view .product-card').nth(1).waitFor();
  assert.equal(await page.locator('.playlists-view .product-card').count(),2);
  assert.equal(await page.locator('.playlists-view select option:checked').innerText(),'いつもの (2)');
  for(const locale of ['zh-TW','zh-CN','ja','en']){
   await nav(page,'settings').click();await page.locator('#ui-locale').selectOption(locale);
   await nav(page,'playlists').click();await page.locator('.playlists-view .product-card').first().waitFor();
   await page.setViewportSize({width:390,height:844});
   assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
   await page.locator('.playlist-toggle').first().click({button:'right'});await page.locator('.playlist-menu').waitFor();
   const bounds=await page.locator('.playlist-menu').boundingBox();assert.ok(bounds.x>=0 && bounds.y>=0 && bounds.x+bounds.width<=390 && bounds.y+bounds.height<=844);
   await page.keyboard.press('Escape');await page.screenshot({path:path.join(captures,`${locale}-narrow.png`)});
   await page.setViewportSize({width:1440,height:1000});
  }
  // Large playlists use the same 30-work pages and bottom jump controls.
  await page.evaluate(async()=>{
   const invoke=window.__TAURI_INTERNALS__.invoke;
   const result=await invoke('list_products',{request:{search:null,accountIds:[],typeGroups:[],ageCategories:[],sourceGroups:[],makerKeys:[],sort:'titleAsc',limit:65,offset:0}});
   for(const work of result.products)await invoke('edit_playlist',{request:{action:'add',playlistId:'0',workId:work.workId}});
  });
  await page.getByRole('button',{name:'Reload',exact:true}).click();
  await page.waitForFunction(()=>document.querySelectorAll('.playlists-view .product-card').length===30);
  const pages=page.getByRole('navigation',{name:'Playlist pages at bottom',exact:true});
  await pages.getByRole('spinbutton',{name:'Page number',exact:true}).fill('2');
  await pages.getByRole('button',{name:'Go',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('.playlists-view .page-status')?.textContent==='Page 2 of 3');
  await page.waitForFunction(()=>document.querySelectorAll('.playlists-view .product-card').length===30);
  await pages.getByRole('button',{name:'Next',exact:true}).click();
  await page.waitForFunction(()=>document.querySelectorAll('.playlists-view .product-card').length>0&&document.querySelectorAll('.playlists-view .product-card').length<=7);
  assert.equal(await pages.getByRole('button',{name:'Next',exact:true}).isDisabled(),true);
  await page.screenshot({path:path.join(captures,'playlists-desktop.png')});
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({passed:true,captures,checks:['undownloaded bookmarks and gallery','default and custom membership toggles','RJ and voice alignment','folder dispatch','context and keyboard menu','create/rename','search','remove without deleting','no autoplay','renamed default identity','reload persistence','failure recovery','four locales / 390px menu']}));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});

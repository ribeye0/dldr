// Production route against isolated native fixtures; never reads the real profile.
const assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE_PATH||'playwright');
const captures=fs.mkdtempSync(path.join(os.tmpdir(),'dldr-player-dock-qa-'));
const url=`${process.env.STORYBOOK_URL||'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
async function centered(page) {
 await page.waitForFunction(()=>{
  const viewport=document.querySelector('.script-viewport');if(!viewport)return false;
  const lines=viewport.querySelectorAll('[aria-current=true]');if(!lines.length)return false;
  const first=lines[0].getBoundingClientRect(),last=lines[lines.length-1].getBoundingClientRect(),box=viewport.getBoundingClientRect();
  return Math.abs((first.top+last.bottom)/2-(box.top+viewport.clientTop+viewport.clientHeight/2))<2;
 });
}
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
 try {
  const page=await browser.newPage({viewport:{width:1360,height:1000}});page.setDefaultTimeout(12000);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(url);await page.locator('.product-card').first().waitFor();
  await page.locator('aside nav').getByRole('button',{name:'Downloaded',exact:true}).click();
  await page.locator('.downloaded-player-hint').getByText('Click the blank area of a work card to open the player.',{exact:true}).waitFor();
  await page.locator('.card-play-target').first().click({position:{x:6,y:6}});
  const dock=page.locator('.player-dock');
  const panel=page.getByRole('dialog',{name:'Audio player',exact:true});await panel.waitFor();
  assert.equal(await panel.getAttribute('aria-modal'),'false');assert.equal(await page.locator('dialog[open]').count(),0);
  assert.equal(await panel.locator('.track-list li').count(),2);
  await panel.getByRole('combobox',{name:'Folder / version',exact:true}).selectOption('MP3');
  assert.equal(await panel.locator('.track-list li').count(),1);
  await panel.getByRole('combobox',{name:'Folder / version',exact:true}).selectOption('WAV');
  await panel.locator('.track-list button').first().click();
  await panel.getByRole('button',{name:'Script',exact:true}).waitFor();
  await panel.locator('.script-viewport').waitFor();
  assert.equal(await panel.locator('.cue-text').first().evaluate(el=>getComputedStyle(el).fontSize),'16px');
  assert.equal(await panel.locator('.panel-header h2').evaluate(el=>getComputedStyle(el).fontSize),'16px');
  assert.equal(await panel.locator('.cue-time').first().evaluate(el=>getComputedStyle(el).fontSize),'14px');
  assert.deepEqual(await panel.getByRole('button',{name:'Collapse player',exact:true}).evaluate(el=>({button:el.clientWidth,icon:el.querySelector('svg').getBoundingClientRect().width})),{button:44,icon:24});
  // Drag across incoming native clock updates, then submit several destinations
  // while IPC is delayed. The final request wins without more user clicks.
  await page.evaluate(()=>{
   const original=window.__TAURI_INTERNALS__.invoke;
   window.__seekQa={original,held:[],positions:[]};
   window.__TAURI_INTERNALS__.invoke=(command,args,...rest)=>{
    const qa=window.__seekQa;
    if(command==='control_player' && args.request.control.action==='seek') {
     qa.positions.push(args.request.control.position);
     return new Promise((resolve,reject)=>qa.held.push(()=>Promise.resolve(original(command,args,...rest)).then(resolve,reject)));
    }
    return original(command,args,...rest);
   };
  });
  const seek=panel.getByRole('slider',{name:'Playback position',exact:true});
  const rangeBox=await seek.boundingBox(), y=rangeBox.y+rangeBox.height/2;
  const emitClock=async position=>page.evaluate(async position=>{
   const qa=window.__seekQa,s=await qa.original('player_snapshot');s.position=position;s.revision++;
   await qa.original('plugin:event|emit',{event:'dldr-player-state',payload:s});
  },position);
  await page.mouse.move(rangeBox.x+rangeBox.width*.2,y);await page.mouse.down();
  await page.mouse.move(rangeBox.x+rangeBox.width*.8,y,{steps:8});
  const dragged=Number(await seek.inputValue());assert.ok(dragged>85);
  await emitClock(4);
  assert.equal(Number(await seek.inputValue()),dragged,'Clock updates must not overwrite an active drag');
  await page.mouse.up();
  await page.waitForFunction(()=>window.__seekQa.held.length===1);
  assert.equal(await seek.isEnabled(),true,'An in-flight seek must not disable the next seek');
  await seek.fill('60');await seek.dispatchEvent('change');
  await seek.fill('90');await seek.dispatchEvent('change');
  assert.equal(await seek.inputValue(),'90');
  await page.evaluate(()=>window.__seekQa.held.shift()());
  await page.waitForFunction(()=>window.__seekQa.positions.length===2);
  assert.equal(await seek.inputValue(),'90','The previous response must not replace the latest preview');
  assert.deepEqual(await page.evaluate(()=>window.__seekQa.positions.slice(1)),[90]);
  await page.evaluate(()=>window.__seekQa.held.shift()());
  await page.waitForFunction(()=>document.querySelector('.seek input').value==='90' && !document.querySelector('.primary-control').disabled);
  await seek.focus();await page.keyboard.press('Home');
  await page.waitForFunction(()=>window.__seekQa.held.length===1);
  await page.evaluate(()=>window.__seekQa.held.shift()());
  await page.waitForFunction(()=>document.querySelector('.seek input').value==='0' && !document.querySelector('.primary-control').disabled);
  await page.evaluate(()=>{window.__TAURI_INTERNALS__.invoke=window.__seekQa.original;delete window.__seekQa;});

  await panel.getByRole('slider',{name:'Playback position',exact:true}).fill('2');
  await panel.getByRole('slider',{name:'Playback position',exact:true}).dispatchEvent('change');
  await panel.locator('.script-viewport button.active').getByText('歡迎回來。',{exact:false}).waitFor();await centered(page);
  await panel.locator('.script-viewport button').nth(1).click();
  assert.equal(await panel.getByRole('slider',{name:'Playback position',exact:true}).inputValue(),'3');
  assert.equal(await panel.locator('.script-viewport button.active').count(),2);await centered(page);
  await panel.getByRole('button',{name:'Player settings',exact:true}).click();
  const subtitleSize=panel.getByRole('combobox',{name:'Subtitle size',exact:true});
  assert.equal(await subtitleSize.inputValue(),'16');
  for(const size of ['14','18','16']) {
   await subtitleSize.selectOption(size);
   assert.equal(await panel.locator('.cue-text').first().evaluate(el=>getComputedStyle(el).fontSize),`${size}px`);
   await centered(page);
  }
  await panel.getByRole('spinbutton',{name:'Subtitle offset (seconds)',exact:true}).fill('1');
  await panel.getByRole('spinbutton',{name:'Subtitle offset (seconds)',exact:true}).dispatchEvent('change');
  await panel.locator('.script-viewport button').nth(1).click();
  assert.equal(await panel.getByRole('slider',{name:'Playback position',exact:true}).inputValue(),'4');await centered(page);
  await panel.getByRole('combobox',{name:'Playback speed',exact:true}).selectOption('1.25');
  await panel.getByRole('combobox',{name:'Subtitles',exact:true}).selectOption('');
  await panel.getByText('Select a VTT subtitle file to display captions.',{exact:true}).waitFor();
  await panel.getByRole('combobox',{name:'Subtitles',exact:true}).selectOption('subtitles/01.vtt');
  await panel.locator('.script-viewport button').last().waitFor();
  assert.equal(await panel.locator('.script-viewport b').count(),0);
  await panel.getByRole('button',{name:'Player settings',exact:true}).first().click();await centered(page);
  await page.screenshot({path:path.join(captures,'player-expanded.png')});
  // Passive playback-clock events advance the centered script; preserve the
  // surrounding page/panel scroll and allow first/last cues to reach the center.
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.evaluate(()=>{
   const original=window.__TAURI_INTERNALS__.invoke;
   window.__TAURI_INTERNALS__.invoke=async(command,args,...rest)=>{
    if(command==='player_subtitle')return Array.from({length:50},(_,i)=>({start:i*10,end:i*10+6,text:i===25?'目前台本句子\n第二行字幕':`台本第 ${i+1} 句`}));
    return original(command,args,...rest);
   };
  });
  await panel.getByRole('button',{name:'Player settings',exact:true}).click();
  await panel.getByRole('combobox',{name:'Subtitles',exact:true}).selectOption('subtitles/02.vtt');
  await panel.getByRole('button',{name:'Player settings',exact:true}).click();
  await panel.locator('.script-viewport button').nth(49).waitFor();
  const outerBefore=await page.evaluate(()=>({page:document.querySelector('.workspace').scrollTop,panel:document.querySelector('.panel-body').scrollTop}));
  for(const position of [1,251,491]){
   await page.evaluate(async position=>{
    const s=await window.__TAURI_INTERNALS__.invoke('player_snapshot');s.position=position;s.revision++;
    await window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'dldr-player-state',payload:s});
   },position);
   await centered(page);
   const highlighted=panel.locator('.script-viewport button.active');
   assert.equal(await highlighted.count(),1);
   const expected=position===1?'台本第 1 句':position===251?'目前台本句子':'台本第 50 句';
   assert.ok((await highlighted.innerText()).includes(expected));
  }
  assert.deepEqual(await page.evaluate(()=>({page:document.querySelector('.workspace').scrollTop,panel:document.querySelector('.panel-body').scrollTop})),outerBefore);
  const follow=panel.getByRole('switch',{name:'Auto-follow playback',exact:true});
  assert.equal(await follow.isChecked(),true);
  await follow.uncheck();
  const readingTop=await panel.locator('.script-viewport').evaluate(el=>{el.scrollTop=700;return el.scrollTop;});
  for(const position of [1,251,71]) {
   await page.evaluate(async position=>{
    const state=await window.__TAURI_INTERNALS__.invoke('player_snapshot');state.position=position;state.revision++;
    await window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'dldr-player-state',payload:state});
   },position);
   await page.waitForFunction(position=>document.querySelector('.script-viewport button.active .cue-time')?.textContent===`${Math.floor(position/60)}:${String(Math.floor(position%60/10)*10).padStart(2,'0')}`,position);
   assert.ok(Math.abs(await panel.locator('.script-viewport').evaluate(el=>el.scrollTop)-readingTop)<1,'Clock events must not move the script while following is off');
  }
  await panel.getByRole('button',{name:'Collapse player',exact:true}).click();
  await dock.locator('.mini-identity').click();
  assert.equal(await follow.isChecked(),false,'Collapsing the player keeps free-reading mode');
  assert.equal(await page.evaluate(()=>localStorage.getItem('dldr.player.follow-subtitles')),'false');
  await page.screenshot({path:path.join(captures,'player-follow-off.png')});
  await follow.focus();await page.keyboard.press('Space');await centered(page);
  assert.equal(await follow.isChecked(),true,'Enabling follow returns to the current cue');
  // Return to a normal fixture clock, then collapse without stopping playback.
  await panel.getByRole('button',{name:'Collapse player',exact:true}).click();await panel.waitFor({state:'hidden'});
  assert.equal(await dock.locator('.mini-player').count(),1);
  const box=await dock.boundingBox();assert.ok(box.width<=400 && box.x>800 && box.y>800);
  await dock.getByRole('button',{name:'Pause',exact:true}).waitFor();
  await page.screenshot({path:path.join(captures,'player-collapsed.png')});
  // A captured pointer can leave the handle. Short/upward/cancelled drags must
  // snap back without the generated click collapsing the player.
  await dock.locator('.mini-identity').click();
  const handle=panel.getByRole('button',{name:'Drag down to collapse',exact:true});
  for(const distance of [28,-40]) {
   const box=await handle.boundingBox(),x=box.x+box.width/2,y=box.y+box.height/2;
   await page.mouse.move(x,y);await page.mouse.down();await page.mouse.move(x,y+distance,{steps:8});
   assert.equal(await dock.evaluate(el=>new DOMMatrix(getComputedStyle(el).transform).m42),Math.max(0,distance));
   await page.mouse.up();assert.equal(await panel.isVisible(),true);
   assert.equal(await dock.evaluate(el=>new DOMMatrix(getComputedStyle(el).transform).m42),0);
  }
  {
   const box=await handle.boundingBox(),x=box.x+box.width/2,y=box.y+box.height/2;
   await page.mouse.move(x,y);await page.mouse.down();await page.mouse.move(x,y+90,{steps:8});
   await handle.evaluate(el=>el.dispatchEvent(new PointerEvent('pointercancel',{pointerId:1,bubbles:true})));
   await page.mouse.up();assert.equal(await panel.isVisible(),true);
   assert.equal(await dock.evaluate(el=>new DOMMatrix(getComputedStyle(el).transform).m42),0);
   await page.mouse.move(x,y);await page.mouse.down();await page.mouse.move(x-100,y+90,{steps:8});await page.mouse.up();
   await panel.waitFor({state:'hidden'});
   await dock.getByRole('button',{name:'Pause',exact:true}).waitFor();
   assert.equal(await dock.locator('.mini-identity').evaluate(el=>el===document.activeElement),true);
  }
  for(const key of ['Enter','Space']) {
   await dock.locator('.mini-identity').click();await handle.focus();await page.keyboard.press(key);
   await panel.waitFor({state:'hidden'});
  }
  await page.locator('aside nav').getByRole('button',{name:'Settings',exact:true}).click();
  await dock.getByRole('button',{name:'Pause',exact:true}).click();
  await dock.getByRole('button',{name:'Play',exact:true}).waitFor();
  for(const [locale,title,followLabel,hint] of [
   ['zh-TW','音訊播放器','自動跟隨播放','點擊作品卡片的空白處，即可開啟播放器。'],
   ['zh-CN','音频播放器','自动跟随播放','点击作品卡片的空白处，即可打开播放器。'],
   ['ja','オーディオプレーヤー','再生位置に自動追従','作品カードの余白をクリックすると、プレーヤーが開きます。'],
   ['en','Audio player','Auto-follow playback','Click the blank area of a work card to open the player.']]) {
   await page.locator('#ui-locale').selectOption(locale);
   await dock.locator('.mini-identity').click();
   await page.getByRole('dialog',{name:title,exact:true}).waitFor();
   await page.getByRole('switch',{name:followLabel,exact:true}).waitFor();
   await page.keyboard.press('Escape');await page.locator('.player-panel').waitFor({state:'hidden'});
   assert.equal(await page.locator('.mini-identity').evaluate(el=>el===document.activeElement),true);
   await page.locator('aside nav button[data-view=downloaded]').click();
   await page.locator('.downloaded-player-hint').getByText(hint,{exact:true}).waitFor();
   await page.locator('aside nav button[data-view=settings]').click();
  }
  // Expanded panel stays nonmodal; background navigation remains available.
  await dock.locator('.mini-identity').click();
  await page.locator('aside nav').getByRole('button',{name:'Library',exact:true}).click();assert.equal(await panel.isVisible(),true);
  // Download and playback panels do not obscure one another.
  await page.locator('.download-status').click();await page.locator('.download-panel').waitFor();
  await panel.waitFor({state:'hidden'});
  await dock.locator('.mini-identity').click();await panel.waitFor();await page.locator('.download-panel').waitFor({state:'hidden'});
  for(const width of [800,390,320]){
   await page.setViewportSize({width,height:width===800?600:844});
   assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
   assert.equal(await panel.evaluate(el=>el.scrollWidth>el.clientWidth+1),false);
   const box=await dock.boundingBox();assert.ok(box.x>=0 && box.y>=0 && box.x+box.width<=width+1);
   await centered(page);
   await page.screenshot({path:path.join(captures,`player-${width}px.png`)});
  }
  // Chromium touch input exercises actual pointer capture and touch-action,
  // rather than calling handlers directly. Cancelling a touch never collapses.
  const cdp=await page.context().newCDPSession(page);
  await cdp.send('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:1});
  for(const end of ['touchCancel','touchEnd']) {
   const box=await handle.boundingBox(),x=box.x+box.width/2,y=box.y+box.height/2;
   await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x,y}]});
   for(let delta=10;delta<=90;delta+=10) await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x,y:y+delta}]});
   await page.waitForFunction(()=>new DOMMatrix(getComputedStyle(document.querySelector('.player-dock')).transform).m42===90);
   await cdp.send('Input.dispatchTouchEvent',{type:end,touchPoints:[]});
   if(end==='touchCancel') {
    assert.equal(await panel.isVisible(),true);
    await page.waitForFunction(()=>new DOMMatrix(getComputedStyle(document.querySelector('.player-dock')).transform).m42===0);
   } else {
    await panel.waitFor({state:'hidden'});await dock.getByRole('button',{name:'Play',exact:true}).waitFor();
   }
  }
  await cdp.send('Emulation.setTouchEmulationEnabled',{enabled:false});await cdp.detach();
  await dock.locator('.mini-identity').click();
  await page.keyboard.press('Escape');
  await dock.locator('.mini-identity').click();await panel.waitFor();
  await panel.getByRole('button',{name:'Player settings',exact:true}).click();
  await panel.getByRole('button',{name:'Stop playback',exact:true}).click();
  await panel.getByText('Choose a track',{exact:true}).waitFor();
  await page.setViewportSize({width:1360,height:1000});
  await panel.getByRole('button',{name:'Collapse player',exact:true}).click();
  await page.locator('aside nav button[data-view=downloaded]').click();
  await page.screenshot({path:path.join(captures,'downloaded-player-hint.png')});
  // The preference survives a fresh app mount, independent of playback history.
  await page.evaluate(()=>localStorage.setItem('dldr.player.follow-subtitles','false'));
  await page.reload();await page.locator('.product-card').first().waitFor();
  await page.locator('aside nav button[data-view=downloaded]').click();
  await page.locator('.card-play-target').first().click({position:{x:6,y:6}});
  await panel.locator('.track-list button').first().click();
  await follow.waitFor();assert.equal(await follow.isChecked(),false);
  await follow.check();
  await panel.getByRole('slider',{name:'Playback position',exact:true}).fill('2');
  await panel.getByRole('slider',{name:'Playback position',exact:true}).dispatchEvent('change');
  await centered(page);
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({passed:true,captures,locales:4,checks:['right-corner dock','nonmodal collapse','mouse/touch drag and cancellation','keyboard handle','drag shielded from native clock','latest seek wins with slow IPC','keyboard seeking','larger player controls','16px default and 14/18px sizes','track groups','controls','centered first/middle/last script','overlapping cues','passive clock','manual script browsing while clock advances','keyboard follow switch returns to current cue','follow preference survives collapse and reload','downloaded player hint in four languages','offset and click seek','isolated scrolling','background navigation','download panel coordination','four locales','800/390/320px','stop']}));
 } catch(error) {
  const page=browser.contexts()[0]?.pages()[0];
  if(page) await page.screenshot({path:path.join(captures,'failure.png'),fullPage:true});
  console.error('QA captures:',captures);
  throw error;
 } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});

function installDldrPlay(config) {
  'use strict';
  if (window.top !== window || location.origin !== 'https://play.dlsite.com') return;
  const dictionary = config.messages;
  const text = (key, value) => (dictionary[key] || key).replace('{0}', String(value ?? ''));
  const idPattern = /^(RJ|BJ|VJ)\d{1,10}$/i;
  function currentWork() {
    const parts = location.pathname.replace(/^\/+|\/+$/g, '').split('/');
    if (parts[0] === 'admin') parts.shift();
    if (parts[0] === 'dllibrary') parts.shift();
    return parts[0] === 'work' && idPattern.test(parts[1] || '') ? parts[1].toUpperCase() : null;
  }
  let panel, action, note, workLabel, current = null, sequence = 0, pending = null, lastCheck = 0, timeout;
  let downloading = false;
  function createPanel() {
    if (panel?.isConnected) return;
    panel = document.createElement('aside');
    panel.id = 'dldr-play-panel';
    panel.setAttribute('aria-label', 'DLDR');
    // Shadow DOM keeps official styles and our small panel independent.
    const root = panel.attachShadow({mode: 'closed'});
    const style = document.createElement('style');
    style.textContent = `:host{position:fixed;right:16px;top:76px;z-index:2147483647;width:min(290px,calc(100vw - 32px));color:#092a83;font:14px/1.45 system-ui,sans-serif;color-scheme:light}*{box-sizing:border-box}.panel{padding:12px;border:1px solid #2255aa;border-radius:12px;background:#fff;box-shadow:0 4px 20px #092a8333}header{display:flex;align-items:center;gap:8px;margin-bottom:8px}strong{font-size:14px}code{margin-left:auto;font-size:12px}button{width:100%;min-height:40px;border:1px solid #092a83;border-radius:8px;background:#092a83;color:white;font:700 14px system-ui;cursor:pointer}button:focus-visible{outline:3px solid #4baef3;outline-offset:2px}button:disabled{background:#e9f5ff;color:#2255aa;cursor:default}p{margin:7px 0 0;overflow-wrap:anywhere}small{display:block;margin-top:8px;font-size:11px;opacity:.85}details{margin-top:6px}summary{cursor:pointer;font-size:12px} :host([hidden]){display:none}`;
    const box = document.createElement('div'); box.className = 'panel';
    const header = document.createElement('header');
    const brand = document.createElement('strong'); brand.textContent = 'DLDR';
    workLabel = document.createElement('code');
    header.append(brand, workLabel);
    action = document.createElement('button'); action.type = 'button'; action.disabled = true;
    action.textContent = text('Checking download status…');
    action.addEventListener('click', () => { if (!pending && current) { downloading = true; request('download'); } });
    note = document.createElement('p'); note.setAttribute('role', 'status'); note.setAttribute('aria-live', 'polite');
    const details = document.createElement('details');
    const summary = document.createElement('summary'); summary.textContent = text('DLDR account: {0}', config.accountLabel);
    const hint = document.createElement('small'); hint.textContent = text('Sign into the same account on the official website.');
    details.append(summary, hint);
    box.append(header, action, note, details); root.append(style, box);
    document.body.append(panel);
  }
  function request(kind) {
    if (!current || pending || document.visibilityState === 'hidden') return;
    const id = ++sequence;
    pending = {id, work: current}; lastCheck = Date.now();
    action.disabled = true;
    if (kind === 'download') { action.textContent = text('Checking download status…'); }
    const url = new URL('/__dldr_bridge', location.origin);
    url.search = new URLSearchParams({token:config.token, action:kind, work:current, sequence:String(id)}).toString();
    // Native navigation interception cancels this URL; no HTTP request is sent.
    location.assign(url.href);
    clearTimeout(timeout);
    timeout = setTimeout(() => {
      if (pending?.id !== id) return;
      pending = null; downloading = false;
      note.textContent = text('Could not read download status. Try again.');
      action.textContent = text('Retry download');
      // A timeout must never turn an uncertain download into another automatic submission.
      action.disabled = true;
      lastCheck = 0;
    }, 15000);
  }
  window.addEventListener('dldr-play-status', (event) => {
    const response = event.detail;
    if (!response || !pending || response.sequence !== pending.id || response.workId !== pending.work || current !== pending.work) return;
    clearTimeout(timeout); pending = null; downloading = false;
    const states = {ready:'Download to DLDR', queued:'Download queued', downloading:'Downloading', downloaded:'Downloaded', unavailable:'Download to DLDR', error:'Retry download'};
    const key = states[response.status]; if (!key) return;
    action.textContent = text(response.status === 'ready' ? response.message : key);
    action.disabled = !['ready','error'].includes(response.status);
    note.textContent = ['ready','downloaded'].includes(response.status) ? '' : text(response.message);
  });
  function update() {
    if (!document.body) return;
    const work = currentWork();
    if (!work) {
      if (panel) panel.hidden = true;
      current = null; pending = null; clearTimeout(timeout); return;
    }
    createPanel(); panel.hidden = false;
    if (work !== current) {
      current = work; workLabel.textContent = work;
      pending = null; downloading = false; lastCheck = 0; clearTimeout(timeout);
      action.disabled = true; action.textContent = text('Checking download status…'); note.textContent = '';
    }
    if (!pending && !downloading && Date.now() - lastCheck >= 5000) request('status');
  }
  // Polling observes history.pushState/replaceState without patching official APIs.
  // It avoids coupling to private React components or scanning unrelated work links.
  window.addEventListener('popstate', update);
  window.addEventListener('visibilitychange', update);
  window.addEventListener('DOMContentLoaded', update, {once:true});
  setInterval(update, 750);
  update();
}

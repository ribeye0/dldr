// Isolated IPC: never downloads a release or replaces an installed application.
const assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const captures = fs.mkdtempSync(path.join(os.tmpdir(), 'dldr-updater-'));
const url = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
(async () => {
 const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE_PATH });
 try {
  for (const [locale, title, check, download, install, cancel, local, inspect, progress, ready] of [
   ['en', 'Update DLDR', 'Check for updates', 'Download update', 'Install and restart', 'Cancel download', 'Install local package', 'Inspect package', 'View update progress', 'Update ready'],
   ['zh-TW', '更新 DLDR', '檢查更新', '下載更新', '安裝並重新啟動', '取消下載', '安裝本機套件', '檢查套件', '查看更新進度', '更新已就緒'],
   ['zh-CN', '更新 DLDR', '检查更新', '下载更新', '安装并重新启动', '取消下载', '安装本地安装包', '检查安装包', '查看更新进度', '更新已就绪'],
   ['ja', 'DLDR を更新', '更新を確認', '更新をダウンロード', 'インストールして再起動', 'ダウンロードをキャンセル', 'ローカルパッケージをインストール', 'パッケージを確認', '更新の進捗を表示', '更新の準備完了'],
  ]) {
   const page = await browser.newPage({ viewport: { width: locale === 'en' ? 1280 : 390, height: 900 } });
   const errors = []; page.on('pageerror', error => errors.push(error.message));
   await page.addInitScript(locale => {
    sessionStorage.setItem('dldr-production-story-settings', JSON.stringify({ uiLocale: locale }));
    localStorage.setItem('dldr.color-theme', locale === 'zh-CN' ? 'dark' : 'light');
   }, locale);
   await page.goto(url); await page.locator('.product-card').first().waitFor();
   await page.evaluate(() => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const qa = window.__updateQa = { checks: 0, downloads: 0, installs: 0, cancellations: [], kind: 'windowsPortable', release: { currentVersion: '0.1.4', version: '0.2.0', notes: 'Short changes.\n<script>not HTML</script>' }, installError: null, trusts: [] };
    window.__TAURI_INTERNALS__.invoke = async (command, args, ...rest) => {
     if (command === 'check_app_update') { qa.checks++; return { kind: qa.kind, release: qa.release }; }
     if (command === 'download_app_update') {
      qa.downloads++; qa.id = args.transferId; qa.progress = args.onEvent.onmessage;
      return new Promise((resolve, reject) => { qa.resolve = resolve; qa.reject = reject; });
     }
     if (command === 'cancel_app_update') { qa.cancellations.push(args.transferId); qa.reject('update_cancelled'); return; }
     if (command === 'install_app_update') { qa.installs++; qa.trusts.push(args.trustLocal); if (qa.installError) throw qa.installError; return; }
     if (command === 'plugin:dialog|open') return '/tmp/DLDR_0.1.2_windows_x64.zip';
     if (command === 'prepare_local_app_update') return { release: { ...qa.release, version: '0.1.2' }, fileName: 'DLDR_0.1.2_windows_x64.zip', size: 200, source: 'local', direction: 'downgrade', verified: false };
     return invoke(command, args, ...rest);
    };
   });
   const settings = page.locator('aside nav button[data-view=settings]');
   await settings.click(); await page.locator('.settings-layout .tabs button').last().click();
   const button = name => page.getByRole('button', { name, exact: true });
   const dialog = page.getByRole('dialog', { name: title, exact: true });
   await button(check).click(); await dialog.waitFor();
   assert.equal(await page.evaluate(() => window.__updateQa.installs), 0);
   await dialog.locator('summary').click(); assert.ok((await dialog.innerText()).includes('<script>not HTML</script>'));
   assert.equal(await dialog.locator('script').count(), 0);
   await dialog.screenshot({ path: path.join(captures, `${locale}-available.png`) });
   assert.equal(await dialog.evaluate(node => node.scrollWidth > node.clientWidth), false);
   await button(download).click();
   await page.waitForFunction(() => window.__updateQa.downloads === 1);
   await page.evaluate(() => window.__updateQa.progress({ event: 'progress', downloaded: 5000, total: 10000 }));
   await page.waitForFunction(() => document.querySelector('dialog progress')?.value === 50);
   await page.keyboard.press('Escape'); await dialog.waitFor({ state: 'hidden' });
   // Navigation remains available while the update transfer is active.
   await page.locator('aside nav button[data-view=library]').click();
   await page.locator('.product-card').first().waitFor();
   await settings.click(); await page.locator('.settings-layout .tabs button').last().click();
   await button(progress).click();
   assert.equal(await page.evaluate(() => window.__updateQa.checks), 1);
   await button(cancel).click(); await button(download).waitFor();
   assert.equal(await page.evaluate(() => window.__updateQa.cancellations[0] === window.__updateQa.id), true);
   await button(download).click();
   await page.waitForFunction(() => window.__updateQa.downloads === 2);
   await page.evaluate(() => window.__updateQa.progress({ event: 'verifying' }));
   await button(cancel).waitFor();
   await page.keyboard.press('Escape');
   await page.evaluate(() => { const qa = window.__updateQa; qa.resolve({ release: qa.release, fileName: 'DLDR_0.2.0_windows_x64.zip', size: 10000, source: 'online', direction: 'upgrade', verified: true }); });
   await button(ready).waitFor();
   assert.equal(await dialog.isVisible(), false);
   assert.equal(await page.evaluate(() => window.__updateQa.installs), 0);
   await button(ready).click(); await button(install).waitFor();
   await page.evaluate(() => window.__updateQa.installError = 'update_jobs_active');
   await button(install).click(); await dialog.getByRole('alert').waitFor();
   assert.equal(await page.evaluate(() => window.__updateQa.downloads), 2);
   await dialog.screenshot({ path: path.join(captures, `${locale}-ready.png`) });
   await page.keyboard.press('Escape');
   await button(local).click(); await button(inspect).click(); await button(install).waitFor();
   assert.equal(await button(install).isDisabled(), true);
   const trust = dialog.getByRole('checkbox'); await trust.check();
   assert.equal(await button(install).isEnabled(), true);
   await dialog.screenshot({ path: path.join(captures, `${locale}-unsigned-downgrade.png`) });
   assert.equal(await dialog.evaluate(node => node.scrollWidth > node.clientWidth), false);
   await page.evaluate(() => window.__updateQa.installError = null);
   await button(install).click();
   await page.waitForFunction(() => window.__updateQa.installs === 2);
   assert.deepEqual(await page.evaluate(() => window.__updateQa.trusts), [false, true]);
   assert.deepEqual(errors, []); await page.close();
  }
  console.log(JSON.stringify({ passed: true, captures }));
 } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });

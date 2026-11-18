// First-run flows use the production route with isolated, synthetic accounts.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const base = `${process.env.STORYBOOK_URL || 'http://localhost:6006'}/iframe.html?id=app-production-workspace--interactive&viewMode=story`;
const captures = fs.mkdtempSync(path.join(os.tmpdir(), 'dldr-first-run-'));

(async () => {
  const browser = await chromium.launch({headless:true, executablePath:process.env.CHROMIUM_EXECUTABLE_PATH});
  const errors = [];
  const page = await browser.newPage({viewport:{width:1200,height:900}});
  page.setDefaultTimeout(15000);
  page.on('pageerror', e => errors.push(e.message));
  try {
    await page.goto(base);
    await page.locator('.product-card').first().waitFor();
    assert.equal(await page.getByRole('dialog',{name:'Welcome to DLDR'}).count(),0);
    await page.goto(`${base}&setup=1&setupSyncError=1`);
    const wizard = page.getByRole('dialog',{name:'Welcome to DLDR'});
    await wizard.waitFor();
    const next = wizard.getByRole('button',{name:'Save and continue',exact:true});
    assert.equal(await next.isDisabled(),true);
    await page.keyboard.press('Escape');
    assert.equal(await wizard.isVisible(),true);
    for (const [code,title] of [['zh-TW','歡迎使用 DLDR'],['zh-CN','欢迎使用 DLDR'],['ja','DLDR へようこそ'],['en','Welcome to DLDR']]) {
      await page.locator('.first-run select').first().selectOption(code);
      await page.getByRole('dialog',{name:title,exact:true}).waitFor();
    }
    await page.screenshot({path:path.join(captures,'account.png')});
    await page.evaluate(() => {document.documentElement.dataset.theme='dark';document.documentElement.style.colorScheme='dark';});
    await page.waitForFunction(() => getComputedStyle(document.querySelector('#setup-login')).backgroundColor === 'rgb(16, 28, 47)');
    await page.setViewportSize({width:800,height:600});
    await page.screenshot({path:path.join(captures,'account-dark.png')});
    assert.equal(await wizard.evaluate(element => element.scrollWidth <= element.clientWidth),true);
    await page.setViewportSize({width:380,height:640});
    assert.equal(await wizard.evaluate(element => element.scrollWidth <= element.clientWidth),true);
    await page.setViewportSize({width:1200,height:900});
    await page.evaluate(() => {document.documentElement.dataset.theme='light';document.documentElement.style.colorScheme='light';});
    await wizard.getByLabel('Email or username',{exact:true}).fill('test@example.invalid');
    await wizard.getByLabel('Password',{exact:true}).fill('synthetic-test-password');
    await next.click();
    await wizard.getByRole('button',{name:'Sign in and sync',exact:true}).waitFor();
    // A saved account survives interruption without being mistaken for a sync.
    await page.reload();
    await wizard.getByRole('button',{name:'Sign in and sync',exact:true}).click();
    await wizard.getByText(/Sign-in or sync failed/).waitFor();
    await wizard.getByRole('button',{name:'Back',exact:true}).click();
    assert.equal(await wizard.getByLabel('Email or username',{exact:true}).inputValue(),'test@example.invalid');
    assert.equal(await wizard.getByLabel('Password',{exact:true}).inputValue(),'');
    await wizard.getByLabel('Password',{exact:true}).fill('corrected-synthetic-password');
    await next.click();
    await wizard.getByRole('button',{name:'Sign in and sync',exact:true}).click();
    // Verification must appear above the setup modal and remain interactive.
    await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{
      event:'dldr-two-factor-request',payload:{requestId:'setup-code',accountId:'test',accountLabel:'DLsite',jobId:'test',attempt:1,previousCodeRejected:false}
    }));
    const verification = page.getByRole('dialog',{name:'Verify DLsite',exact:true});
    await verification.getByLabel('Verification code',{exact:true}).fill('123456');
    await verification.getByRole('button',{name:'Verify',exact:true}).click();
    await verification.waitFor({state:'hidden'});
    await wizard.getByText(/Your purchases are synced\./).waitFor();
    await wizard.getByRole('button',{name:'Continue',exact:true}).click();
    assert.equal(await wizard.getByLabel('Library folder',{exact:true}).inputValue(),'/tmp/Downloads/dldr');
    // Resume after successful sync at folder setup, never restart authentication.
    await page.reload();
    await wizard.getByRole('button',{name:'Finish setup',exact:true}).waitFor();
    await wizard.getByLabel('Library folder',{exact:true}).fill('/not-writable');
    await wizard.getByRole('button',{name:'Finish setup',exact:true}).click();
    await wizard.getByRole('alert').getByText(/Permission denied/).waitFor();
    assert.equal(await page.evaluate(() => JSON.parse(sessionStorage.getItem('dldr-story-setup-settings') || '{}').libraryRoot ?? null),null);
    await wizard.getByRole('button',{name:'Browse',exact:true}).click();
    assert.equal(await wizard.getByLabel('Library folder',{exact:true}).inputValue(),'/tmp/中文 日本語 Downloads/dldr');
    await wizard.getByRole('button',{name:'Use default folder',exact:true}).click();
    await page.screenshot({path:path.join(captures,'folders.png')});
    await wizard.getByRole('button',{name:'Finish setup',exact:true}).click();
    await wizard.waitFor({state:'hidden'});
    await page.locator('.product-card').first().waitFor();
    const accounts = await page.evaluate(() => JSON.parse(sessionStorage.getItem('dldr-story-setup-accounts')));
    assert.equal(accounts.length,1);
    const settings = await page.evaluate(() => JSON.parse(sessionStorage.getItem('dldr-story-setup-settings')));
    assert.equal(settings.libraryRoot,'/tmp/Downloads/dldr');
    assert.equal(settings.downloadRoot,'/tmp/Downloads/dldr/.staging');
    await page.reload();
    await page.locator('.product-card').first().waitFor();
    assert.equal(await wizard.count(),0);
    // An existing custom destination stays intact when only sync was missing.
    await page.evaluate(() => {
      const accounts = JSON.parse(sessionStorage.getItem('dldr-story-setup-accounts'));
      accounts[0].lastSyncAt = null;
      sessionStorage.setItem('dldr-story-setup-accounts',JSON.stringify(accounts));
      const settings = JSON.parse(sessionStorage.getItem('dldr-story-setup-settings'));
      settings.libraryRoot='/custom/library';settings.downloadRoot='/custom/staging';
      sessionStorage.setItem('dldr-story-setup-settings',JSON.stringify(settings));
    });
    await page.goto(`${base}&setup=1`);
    await wizard.getByRole('button',{name:'Sign in and sync',exact:true}).click();
    await wizard.getByRole('button',{name:'Continue',exact:true}).click();
    assert.equal(await wizard.getByLabel('Library folder',{exact:true}).inputValue(),'/custom/library');
    await wizard.getByText('Existing staging folder: /custom/staging',{exact:true}).waitFor();
    await wizard.getByRole('button',{name:'Finish setup',exact:true}).click();
    await wizard.waitFor({state:'hidden'});
    assert.equal(await page.evaluate(() => JSON.parse(sessionStorage.getItem('dldr-story-setup-settings')).downloadRoot),'/custom/staging');
    assert.deepEqual(errors,[]);
    console.log(`First-run checks passed; screenshots: ${captures}`);
  } catch (error) {
    await page.screenshot({path:path.join(captures,'failure.png')});
    console.error('Screenshot:',path.join(captures,'failure.png'));
    console.error((await page.locator('.first-run').allTextContents()).join('\n'));
    throw error;
  } finally { await browser.close(); }
})().catch(error => {console.error(error);process.exit(1);});

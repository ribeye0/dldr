<script lang="ts">
  import { t } from "$lib/i18n";
  import Button from "$lib/components/ui/Button.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import * as api from "$lib/api/tauri";
  import { chooseBackupFile, chooseBackupDestination } from "$lib/api/native";
  import { errorMessage } from "$lib/utils/format";
  let busy = $state(false);
  let message = $state("");
  let error = $state("");
  let restorePath = $state<string | null>(null);
  let report = $state<api.VerificationReport | null>(null);
  async function run(action: () => Promise<void>) {
    busy = true; error = ""; message = "";
    try { await action(); } catch(e) { error = errorMessage(e); } finally { busy = false; }
  }
  async function backup() {
    const path = await chooseBackupDestination();
    if (path) { await api.exportLibraryBackup(path); message = $t("備份已儲存：{0}", path); }
  }
  async function restore() {
    const path = restorePath; restorePath = null;
    if (path) { const count = await api.restoreLibraryBackup(path); message = $t("已匯入 {0} 筆資料。請重新載入設定與作品列表；下載項目保持暫停。", count); }
  }
</script>

<section aria-label={$t("Library maintenance")} class="maintenance">
  <h2>{$t("作品庫維護")}</h2>
  <p>{$t("備份包含作品資訊、帳號識別、設定與下載狀態，不包含作品檔案、密碼或登入憑證。請先等待背景工作結束。")}</p>
  <div class="tools">
    <Button disabled={busy} onclick={() => run(async () => { report = await api.verifyLibrary(); message = $t("已核對 {0} 部：{1} 部存在，{2} 部遺失。", report.checked, report.present, report.missing); })}>{$t("核對已下載檔案")}</Button>
    <Button disabled={busy} onclick={() => run(backup)}>{$t("匯出備份")}</Button>
    <Button disabled={busy} onclick={() => run(async () => { restorePath = await chooseBackupFile(); })}>{$t("匯入備份")}</Button>
  </div>
  <p>{$t("核對會將確認遺失的資料夾標記為未完整下載，方便重新下載。無法讀取、位於作品庫外或涉及復原的項目只會列出，不會刪除檔案。")}</p>
  {#if busy}<p role="status">{$t("處理中…")}</p>{/if}
  {#if message}<p role="status">{message}</p>{/if}
  {#if error}<p role="alert">{$t(error)}</p>{/if}
  {#if report?.issues.length}<ul aria-label={$t("Library verification issues")}>{#each report.issues as issue}<li>{issue.workId} · {issue.status}<br />{issue.path}</li>{/each}</ul>{/if}
</section>
{#if restorePath}<Modal title={$t("匯入作品庫備份？")} onClose={() => restorePath = null}>
  <p>{$t("將合併備份中的資料；相同作品、標籤和設定會以備份為準，原有密碼與登入憑證保留。備份內的路徑也會還原，下載不會自動開始。")}</p>
  <p>{restorePath}</p>
  {#snippet actions()}<Button onclick={() => restorePath = null}>{$t("取消")}</Button><Button variant="primary" onclick={() => run(restore)}>{$t("確認匯入")}</Button>{/snippet}
</Modal>{/if}
<style>
  .maintenance { margin-top: 24px; padding: 16px; border: 1px solid var(--dldr-color-ui-line); border-radius: 8px; }
  h2 { font-size: 20px; margin-top: 0; }
  p,li { font-size: var(--dldr-settings-detail-size, 13px); color: var(--dldr-color-ui-dim); overflow-wrap: anywhere; }
  .tools { display: flex; flex-wrap: wrap; gap: 8px; }
  [role=alert] { color: var(--dldr-color-ui-error); }
</style>

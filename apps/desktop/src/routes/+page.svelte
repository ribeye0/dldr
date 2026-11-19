<script lang="ts">
  import { t, locale } from "$lib/i18n";
  import { onDestroy, onMount, untrack } from "svelte";
  import { AccountsController } from "$lib/controllers/accounts-controller.svelte";
  import { SettingsController } from "$lib/controllers/settings-controller.svelte";
  import { DecisionController, NotificationController } from "$lib/controllers/feedback-controller.svelte";
  import { RequestGeneration } from "$lib/utils/request-generation";
  import { initializeTheme } from "$lib/theme";
  import { appendSearchToken, parseProductSearch } from "$lib/utils/product-search";
  import { downloadMetrics, updateDownloadSession } from "$lib/utils/download-progress";
  import { DOWNLOAD_CONCURRENCY_KEY, DEFAULT_DOWNLOAD_CONCURRENCY, downloadConcurrency } from "$lib/utils/download-concurrency";
  import { installDiagnosticHandlers } from "$lib/api/diagnostics";
  import * as native from "$lib/api/native";
  import * as commands from "$lib/api/tauri";
  import PlayerView from "$lib/features/player/PlayerView.svelte";
  import { PlayerController } from "$lib/controllers/player-controller.svelte";
  import PlayLaunchDialog from "$lib/features/online/PlayLaunchDialog.svelte";
  import AppShell from "$lib/components/layout/AppShell.svelte";
  import BulkDownloadDialogView from "$lib/features/downloads/BulkDownloadDialog.svelte";
  import ConfirmationDialogView from "$lib/components/ui/ConfirmationDialog.svelte";
  import ToastStack from "$lib/components/ui/ToastStack.svelte";
  import TwoFactorDialog from "$lib/features/accounts/TwoFactorDialog.svelte";
  import { JobController } from "$lib/controllers/job-controller.svelte";
  import { CreditHydrationController } from "$lib/controllers/credit-hydration-controller";
  import { GalleryController } from "$lib/controllers/gallery-controller.svelte";
  import { TwoFactorController } from "$lib/controllers/two-factor-controller.svelte";
  import AccountsView from "$lib/features/accounts/AccountsView.svelte";
  import FirstRunSetup from "$lib/features/accounts/FirstRunSetup.svelte";
  import { firstRunStep } from "$lib/utils/accounts";
  import LogsView from "$lib/features/logs/LogsView.svelte";
  import LogOperationDialog from "$lib/features/logs/LogOperationDialog.svelte";
  import { hasOpenModal } from "$lib/components/ui/modal-stack";
  import type { DiagnosticTarget } from "$lib/utils/diagnostic-message";
  import DownloadStatus from "$lib/features/downloads/DownloadStatus.svelte";
  import DownloadProgressPanel from "$lib/features/downloads/DownloadProgressPanel.svelte";
  import { DownloadQueueController } from "$lib/controllers/download-queue-controller.svelte";
  import LibraryView from "$lib/features/library/LibraryView.svelte";
  import PlaylistsView from "$lib/features/player/PlaylistsView.svelte";
  import PlaylistMenu from "$lib/features/player/PlaylistMenu.svelte";
  import { PlaylistController } from "$lib/controllers/playlist-controller.svelte";
  import SelectedWorksDialog from "$lib/features/library/SelectedWorksDialog.svelte";
  import DownloadedWorksDialog from "$lib/features/library/DownloadedWorksDialog.svelte";
  import { runLocalWorkBatch, type LocalWorkOperation, type LocalWorkProgress } from "$lib/utils/local-work-batch";
  import {
    SELECTION_STORAGE_KEY, addProducts, restoreSelection,
    selectedDownloadRequest, toggleProductSelection, selectedProduct, selectionSizeLabel,
  } from "$lib/utils/product-selection";
  import ProductDetailDialog from "$lib/features/library/ProductDetailDialog.svelte";
  import ProductImagePreviewView from "$lib/features/library/ProductImagePreview.svelte";
  import SettingsView from "$lib/features/settings/SettingsView.svelte";
  import UpdateDialog from "$lib/features/settings/UpdateDialog.svelte";
  import { UpdateController } from "$lib/controllers/update-controller.svelte";
  import {
    DLSITE_URL,
    GITHUB_URL,
  } from "$lib/model/constants";
  import {
    errorMessage,
    shortDate,
    valueOrNull,
  } from "$lib/utils/format";
  import {
    activeJobDetail,
    isActiveJob,
    isDownloadQueueJob,
    isTerminalJob,
    jobAccountId,
    jobLabel,
    jobOutputNumber,
    jobOutputString,
    jobWorkId,
    metadataNumber,
  } from "$lib/utils/jobs";
  import { dlsiteProductPageUrl } from "$lib/utils/products";
  import type {
    Account,
    AuditEvent,
    BulkDownloadDialog,
    BulkWorkDownloadPreview,
    ConfirmationDialog,
    JobEvent,
    JobSnapshot,
    Product,
    SelectedProduct,
    ProductCreditField,
    ProductDetail,
    ProductDownload,
    ProductImagePreview,
    StartWorkDownloadOptions,
    View,
  } from "$lib/model/types";

  onMount(() => initializeTheme(dark => {
    void native.setNativeHeaderTheme(dark).catch(error => console.warn("Native titlebar appearance:", error));
  }));

  const notifications = new NotificationController();
  const confirmations = new DecisionController<ConfirmationDialog>();
  const bulkConfirmation = new DecisionController<BulkDownloadDialog>();
  const settings = new SettingsController(notifications, () => refreshLibraryAfterMutation(true));
  const accountState = new AccountsController(
    notifications, confirmations.show,
    resetPage => gallery.load({ resetPage, clampInvalidPage: true }),
  );

  const player = new PlayerController();
  const playlists = new PlaylistController();
  let playlistMenu = $state<{work:Product;point:{x:number;y:number;trigger:HTMLButtonElement}}|null>(null);
  onMount(() => {void playlists.load().catch(cause=>notifications.error(errorMessage(cause)));});
  function choosePlaylist(work:Product,point:{x:number;y:number;trigger:HTMLButtonElement}) {playlistMenu={work,point};}
  async function togglePlaylist(work:Product,id="0") {
    const removing = playlists.lists.find(list=>list.id===id)?.workIds.includes(work.workId) ?? false;
    try {
      const updated = await playlists.edit({action:removing ? "remove" : "add",playlistId:id,workId:work.workId});
      if (updated) {
        const list=playlists.lists.find(list=>list.id===id);
        notifications.success($t(removing ? "Removed {0} from {1}" : "Added {0} to {1}",work.workId,list?.name || $t("Playlist {0}",0)));
        playlistMenu=null;
      }
    } catch(cause) {notifications.error(errorMessage(cause));}
  }
  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void player.start().then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(cause => notifications.error(errorMessage(cause)));
    return () => { disposed = true; player.dispose(); unlisten?.(); };
  });
  function openPlayer(work: Product | ProductDetail) {
    downloadPanelOpen = false;
    closeProductDetail();
    void player.open(work);
  }

  let playLaunch = $state<{workId:string|null; accounts:Account[]} | null>(null);
  let playAccountId = $state("");
  let playOpening = $state(false);
  let playError = $state("");
  function requestOnlinePlay(product?: Product) {
    const available = accountState.accounts.filter(account => account.enabled && (!product || product.owners.some(owner => owner.accountId === account.id)));
    if (!available.length) {
      notifications.error($t("Add or enable a matching account in Settings before opening DLsite Play."));
      return;
    }
    playError = "";
    playAccountId = available.some(a => a.id === playAccountId) ? playAccountId : available.length === 1 ? available[0].id : "";
    playLaunch = {workId:product?.workId ?? null,accounts:available};
  }
  async function launchOnlinePlay() {
    if (!playLaunch || !playAccountId || playOpening) return;
    playOpening = true; playError = "";
    try {
      await native.openDlsitePlay(playAccountId, playLaunch.workId);
      playLaunch = null;
    } catch (cause) { playError = errorMessage(cause); }
    finally { playOpening = false; }
  }
  let activeView = $state<View>("library");
  let settingsTab = $state("general");
  let downloadPanelOpen = $state(false);
  const downloadQueue = new DownloadQueueController();
  onMount(() => downloadQueue.start());

  function toggleDownloadPanel() {
    downloadPanelOpen = !downloadPanelOpen;
    if (downloadPanelOpen) player.expanded = false;
  }
  function closeDownloadPanel() {
    downloadPanelOpen = false;
    document.querySelector<HTMLButtonElement>('[aria-controls="download-progress-panel"]')?.focus();
  }
  function openSettingsTab(tab: string) {
    settingsTab = tab;
    downloadPanelOpen = false;
    navigate("settings");
  }

  let setupOpen = $state(false);
  const appUpdate = new UpdateController();
  onDestroy(() => appUpdate.dispose());

  let creditLoadingWorkIds = $state<string[]>([]);
  let bulkDownloadPlanning = $state(false);
  let selectedProducts = $state<SelectedProduct[]>([]);
  let downloadedSelection = $state<SelectedProduct[]>([]);
  const currentSelection = $derived(activeView === "downloaded" ? downloadedSelection : selectedProducts);
  let localWorkBusy = $state(false);
  let localWorkProgress = $state<LocalWorkProgress | null>(null);
  let selectionReviewOpen = $state(false);
  let selectionAnchorId: string | null = null;
  const jobController = new JobController();
  let downloadSessionIds = $state<string[]>([]);
  $effect(() => {
    const jobs = jobController.jobs;
    const previous = untrack(() => downloadSessionIds);
    const next = updateDownloadSession(previous, jobs);
    if (next.join() !== previous.join()) downloadSessionIds = next;
  });
  let auditEvents = $state<AuditEvent[]>([]);
  let auditLoading = $state(true);
  let auditLogDir = $state("");
  let activityTarget = $state<DiagnosticTarget | null>(null);
  let productImagePreview = $state<ProductImagePreview | null>(null);
  let productDetail = $state<ProductDetail | null>(null);
  let productDetailLoadingWorkId = $state<string | null>(null);
  let batchConcurrency = $state(DEFAULT_DOWNLOAD_CONCURRENCY);
  const twoFactor = new TwoFactorController((error) =>
    notifications.error(errorMessage(error)),
  );
  const detailGeneration = new RequestGeneration();
  const auditGeneration = new RequestGeneration();
  const creditHydration = new CreditHydrationController(commands, {
    updated: applyProductCredits,
    pending: (ids) => { creditLoadingWorkIds = ids; },
  });
  const gallery = new GalleryController({
    beforeLoad: () => creditHydration.clearQueue(),
    page: (page) => {
      const current = new Map(page.products.map((product) => [product.workId, product]));
      const refreshed = selectedProducts.map((product) => current.has(product.workId) ? selectedProduct(current.get(product.workId)!) : product);
      if (JSON.stringify(refreshed) !== JSON.stringify(selectedProducts)) updateSelection(refreshed);
      downloadedSelection = downloadedSelection.flatMap((item) => {
        const product = current.get(item.workId);
        if (!product) return [item];
        return product.download.status === "downloaded" ? [selectedProduct(product)] : [];
      });
      creditHydration.show(page.products);
    },
    error: error => notifications.error(errorMessage(error)),
  });

  onMount(() => {
    try {
      batchConcurrency = downloadConcurrency(localStorage.getItem(DOWNLOAD_CONCURRENCY_KEY));
    } catch { /* The default still works if preference storage is unavailable. */ }
    try {
      selectedProducts = restoreSelection(localStorage.getItem(SELECTION_STORAGE_KEY));
    } catch {
      notifications.error($t("Could not restore saved selections. Your library is unchanged."));
    }
    void loadInitial();

    const unlisteners: (() => void)[] = [installDiagnosticHandlers()];
    let disposed = false;

    const register = (pending: Promise<() => void>) => {
      void pending.then((cleanup) => {
        if (disposed) {
          cleanup();
        } else {
          unlisteners.push(cleanup);
        }
      }).catch(error => { if (!disposed) notifications.error(errorMessage(error)); });
    };

    register(jobController.listen(handleJobEvent));
    register(
      native.listenToTwoFactorRequests((request) => twoFactor.enqueue(request)),
    );
    register(
      native.listenToTwoFactorClosures((closed) => {
        // The job stopped waiting (timeout, cancellation, or another window answered).
        twoFactor.close(closed.requestId);
      }),
    );

    return () => {
      disposed = true;
      twoFactor.dispose();
      invalidateLibrary();

      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  });

  onDestroy(() => {
    creditHydration.dispose();
    auditGeneration.invalidate();
    gallery.dispose();
    jobController.dispose();
    settings.dispose();
    accountState.dispose();
    notifications.dispose();
    confirmations.dispose();
    bulkConfirmation.dispose();
  });

  async function loadInitial() {
    const [settingsReady, , accountsReady] = await Promise.all([
      settings.load(),
      settings.loadAppInfo(),
      accountState.load(),
      gallery.load(),
      loadJobs(),
      loadAuditLogDir(),
      loadAuditEvents(),
    ]);
    if (settingsReady && accountsReady) setupOpen = firstRunStep(accountState.accounts, settings.libraryRoot) !== null;
  }

  async function refreshSetup() {
    const [accountsReady, jobsReady] = await Promise.all([accountState.load(), loadJobs()]);
    if (!accountsReady || !jobsReady) throw new Error($t("Could not refresh setup. Please try again."));
  }

  async function finishSetup() {
    const [settingsReady, accountsReady] = await Promise.all([settings.load(), accountState.load(), gallery.load()]);
    if (!settingsReady || !accountsReady) throw new Error($t("Could not refresh setup. Please try again."));
    setupOpen = false;
    accountState.reset();
  }

  async function openReleases() { await openExternalUrl(`${GITHUB_URL}/releases`, "GitHub"); }

  function invalidateLibrary() {
    gallery.invalidate();
    detailGeneration.invalidate();
  }

  async function refreshLibraryAfterMutation(resetPage = false) {
    invalidateLibrary();
    const detailId = productDetailLoadingWorkId ?? productDetail?.workId;
    await Promise.all([
      activeView === "playlists" ? playlists.load().catch(cause=>notifications.error(errorMessage(cause))) : Promise.resolve(),
      gallery.load({ resetPage, clampInvalidPage: !resetPage, background: !resetPage }),
      detailId ? loadProductDetail(detailId, !resetPage) : Promise.resolve(),
    ]);
  }

  async function reloadProducts() {
    creditHydration.retry();
    await gallery.load({ clampInvalidPage: true });
  }

  function productBulkRequest(): commands.BulkWorkDownloadRequest {
    return {
      ...gallery.metadata,
      search: valueOrNull(gallery.search),
      accountIds: [],
      typeGroups: gallery.typeGroups,
      ageCategories: gallery.ageCategories,
      sourceGroups: gallery.sourceGroups,
      makerKeys: gallery.makerKeys,
      sort: gallery.sort,
      unpackPolicy: settings.unpackPolicy,
      skipDownloaded: true,
    };
  }

  function updateSelection(next: SelectedProduct[]) {
    selectedProducts = next;
    try {
      localStorage.setItem(SELECTION_STORAGE_KEY, JSON.stringify(next));
    } catch {
      notifications.error($t("Selections are kept for this session, but could not be saved for the next launch."));
    }
  }

  function toggleSelection(product: Product, range: boolean) {
    if (localWorkBusy) return;
    updateCurrentSelection(toggleProductSelection(currentSelection, product, gallery.products, selectionAnchorId, range, activeView === "downloaded"));
    selectionAnchorId = product.workId;
  }

  function selectCurrentPage() {
    if (localWorkBusy) return;
    updateCurrentSelection(addProducts(currentSelection, gallery.products, activeView === "downloaded"));
    selectionAnchorId = null;
  }

  function deselectCurrentPage() {
    if (localWorkBusy) return;
    const visibleIds = new Set(gallery.products.map((product) => product.workId));
    updateCurrentSelection(currentSelection.filter((product) => !visibleIds.has(product.workId)));
    selectionAnchorId = null;
  }

  function removeFromSelection(workId: string) {
    if (!localWorkBusy) updateCurrentSelection(currentSelection.filter((product) => product.workId !== workId));
  }

  function clearSelection() {
    if (localWorkBusy) return;
    updateCurrentSelection([]);
    selectionAnchorId = null;
  }

  function updateCurrentSelection(next: SelectedProduct[]) {
    if (activeView === "downloaded") downloadedSelection = next;
    else updateSelection(next);
  }

  async function manageDownloadedSelection(operation: LocalWorkOperation) {
    if (localWorkBusy || !downloadedSelection.length) return;
    const snapshot = downloadedSelection.map((item) => item.workId);
    localWorkBusy = true;
    localWorkProgress = null;
    try {
      let destination: string | null = null;
      if (operation === "delete") {
        const confirmed = await confirmations.show({
          eyebrow: $t("Delete selected files"),
          title: $t("Delete local files for {0} selected works?", snapshot.length),
          message: $t("This permanently deletes the selected work folders and staging files. Library metadata and purchase records are kept. This cannot be undone."),
          confirmLabel: $t("Delete selected files"), cancelLabel: $t("Cancel"), tone: "danger",
        });
        if (!confirmed) return;
      } else {
        const chosen = await native.chooseDirectory({
          title: $t("Choose export destination"), canCreateDirectories: true,
        });
        if (typeof chosen !== "string") return;
        destination = chosen;
      }
      // Open progress only after the decision; otherwise this modal can cover
      // the confirmation in the browser's top layer and block every action.
      selectionReviewOpen = true;
      const result = await runLocalWorkBatch(snapshot, operation, destination, {
        deleteWorkDownload: (id) => commands.deleteWorkDownload(id, true),
        exportWorkDownload: commands.exportWorkDownload,
      },
        (progress) => { localWorkProgress = progress; }, setProductDownload);
      if (operation === "delete") {
        const removed = new Set(result.succeeded);
        downloadedSelection = downloadedSelection.filter((item) => !removed.has(item.workId));
        await refreshLibraryAfterMutation();
      }
      const message = $t("{0} succeeded, {1} failed", result.succeeded.length, result.failures.length);
      if (result.failures.length) notifications.error(message);
      else notifications.success(message);
    } catch (error) {
      notifications.error(errorMessage(error));
    } finally {
      localWorkBusy = false;
    }
  }

  function downloadAccountId() {
    const enabled = accountState.accounts.filter(account => account.enabled);
    return enabled.length === 1 ? enabled[0].id : null;
  }

  async function loadJobs() {
    try {
      await jobController.load();
      return true;
    } catch (err) {
      notifications.error(errorMessage(err));
      return false;
    }
  }

  async function handleJobEvent(event: JobEvent) {
    if (event.kind === "accountSync" && isTerminalJob(event.snapshot)) {
      await Promise.all([
        accountState.load(),
        refreshLibraryAfterMutation(),
        loadAuditEvents(),
      ]);
    }

    if (
      (event.kind === "workDownload" || event.kind === "bulkWorkDownload") &&
      isTerminalJob(event.snapshot)
    ) {
      await Promise.all([refreshLibraryAfterMutation(), loadAuditEvents()]);
    }
  }

  function setProductDownload(workId: string, download: ProductDownload) {
    if (download.status !== "downloaded") {
      downloadedSelection = downloadedSelection.filter((item) => item.workId !== workId);
    }
    invalidateLibrary();
    gallery.products = gallery.products.map((product) =>
      product.workId === workId
        ? {
            ...product,
            download,
          }
        : product,
    );

    if (productDetail?.workId === workId) {
      productDetail = {
        ...productDetail,
        download,
      };
    }
  }

  async function searchProducts() {
    await gallery.load({ resetPage: true });
  }

  async function copyWorkId(workId: string) {
    try {
      await navigator.clipboard.writeText(workId);
      notifications.success($t("Copied work ID {0}", workId));
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function copyCreditField(field: ProductCreditField, workId?: string) {
    if (field.missing) {
      return;
    }

    try {
      await navigator.clipboard.writeText(field.value);
      notifications.success(
        workId
          ? $t("Copied {0} for {1}", $t(field.label), workId)
          : $t("Copied {0}", $t(field.label)),
      );
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function copyText(
    label: string,
    value: string | null | undefined,
    workId?: string,
  ) {
    const normalized = value?.trim();

    if (!normalized) {
      return;
    }

    try {
      await navigator.clipboard.writeText(normalized);
      notifications.success(
        workId ? $t("Copied {0} for {1}", label, workId) : $t("Copied {0}", label),
      );
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function openProductDetail(product: Product) {
    await loadProductDetail(product.workId);
  }

  function applyProductCredits(detail: ProductDetail) {
    gallery.products = gallery.products.map((product) => product.workId === detail.workId
      ? { ...product, creditGroups: detail.creditGroups, tags: detail.tags }
      : product);
    if (productDetail?.workId === detail.workId) {
      productDetail = { ...productDetail, creditGroups: detail.creditGroups, tags: detail.tags };
    }
  }

  async function loadProductDetail(workId: string, background = false) {
    const generation = detailGeneration.invalidate();
    if (!background) productDetailLoadingWorkId = workId;

    try {
      const detail = await commands.getProductDetail(workId);
      if (detailGeneration.current(generation)) {
        productDetail = detail;
        applyProductCredits(detail);
      }
    } catch (err) {
      if (detailGeneration.current(generation)) notifications.error(errorMessage(err));
    } finally {
      if (detailGeneration.current(generation))
        productDetailLoadingWorkId = null;
    }
  }

  function closeProductDetail() {
    detailGeneration.invalidate();
    productDetailLoadingWorkId = null;
    productDetail = null;
  }

  function openProductImage(product: Product) {
    if (!product.thumbnailUrl) {
      return;
    }

    productImagePreview = {
      url: product.thumbnailUrl,
      title: product.title,
      workId: product.workId,
    };
  }

  function openProductImageFromDetail(detail: ProductDetail) {
    openProductImage(detail);
  }

  function closeProductImage() {
    productImagePreview = null;
  }

  async function syncAccount(account: Account): Promise<boolean> {
    try {
      const response = await commands.startAccountSync(
        account.id,
        accountState.editingId === account.id ? valueOrNull(accountState.password) : null,
      );
      notifications.info($t("Sync queued"));
      jobController.setMessage(response.jobId, $t("Sync queued"));
      accountState.password = "";
      await loadJobs();
      return true;
    } catch (err) {
      notifications.error(errorMessage(err));
      return false;
    }
  }

  async function syncEnabledAccounts() {
    const enabledAccounts = accountState.accounts.filter(
      (account) => account.enabled && !activeAccountSyncJob(account.id),
    );

    for (const account of enabledAccounts) {
      const started = await syncAccount(account);
      if (!started) {
        break;
      }
    }
  }

  async function cancelAccountSync(account: Account) {
    const job = activeAccountSyncJob(account.id);

    if (!job) {
      return;
    }

    await cancelJob(job);
  }

  async function startWorkDownload(
    product: Product,
    options: StartWorkDownloadOptions = {},
  ) {
    if (activeWorkDownloadJob(product.workId)) {
      return;
    }

    try {
      const response = await commands.startWorkDownload({
        workId: product.workId,
        accountId: downloadAccountId(),
        password: null,
        unpackPolicy: options.unpackPolicy ?? settings.unpackPolicy,
        replaceExisting: options.replaceExisting ?? false,
      });
      const queuedMessage = options.queuedMessage ?? $t("Download queued");
      notifications.info($t("{0} for {1}", queuedMessage, product.workId));
      jobController.setMessage(response.jobId, queuedMessage);
      await loadJobs();
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function startBulkWorkDownload(selectedOnly = false) {
    if (bulkDownloadPlanning || (selectedOnly && !selectedProducts.length)) return;
    // Freeze IDs and metadata before the async preview. The confirmation must start
    // exactly this selection even if the Library query changes while planning.
    const selection = selectedOnly ? selectedProducts.map((item) => ({ ...item })) : undefined;
    const request = { ...(selection ? selectedDownloadRequest(selection, settings.unpackPolicy) : productBulkRequest()), maxConcurrentWorks: batchConcurrency };
    selectionReviewOpen = false;
    bulkDownloadPlanning = true;

    try {
      const preview =
        await commands.previewBulkWorkDownload(request);

      if (preview.requestedCount === 0) {
        await showBulkDownloadDialog(preview, "notice", selection);
        return;
      }

      const space = await commands.checkDownloadSpace(request);
      const confirmed = await showBulkDownloadDialog(preview, "confirm", selection, space);

      if (!confirmed) {
        return;
      }

      request.maxConcurrentWorks = batchConcurrency;
      try { localStorage.setItem(DOWNLOAD_CONCURRENCY_KEY, String(batchConcurrency)); }
      catch { /* Keep the chosen limit for this session. */ }

      const response =
        await commands.startBulkWorkDownload(request);
      notifications.info($t("Bulk download queued"));
      jobController.setMessage(response.jobId, $t("Bulk download queued"));
      await loadJobs();
    } catch (err) {
      notifications.error(errorMessage(err));
    } finally {
      bulkDownloadPlanning = false;
    }
  }

  async function openDownloadedProduct(product: Product) {
    if (!product.download.localPath) {
      return;
    }

    try {
      await commands.openWorkDownload(product.workId);
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function downloadProductArchivesOnly(product: Product) {
    await startWorkDownload(product, {
      unpackPolicy: "keepArchives",
      queuedMessage: $t("Archive-only download queued"),
    });
  }

  async function redownloadProduct(product: Product) {
    const confirmed = await confirmations.show({
      eyebrow: $t("Re-download"),
      title: $t("Re-download {0}?", product.workId),
      message:
        $t("This will replace the local folder after the new download completes. Any changes inside that folder will be removed."),
      confirmLabel: $t("Re-download"),
      cancelLabel: $t("Cancel"),
      tone: "danger",
    });

    if (!confirmed) {
      return;
    }

    await startWorkDownload(product, {
      unpackPolicy: settings.unpackPolicy,
      replaceExisting: true,
      queuedMessage: $t("Re-download queued"),
    });
  }

  async function deleteDownloadedProduct(product: Product) {
    const confirmed = await confirmations.show({
      eyebrow: $t("Delete Download"),
      title: $t("Delete downloaded files for {0}?", product.workId),
      message:
        $t("This removes the local downloaded folder and any staging files. Cached ownership stays intact, so you can download it again later."),
      confirmLabel: $t("Delete Download"),
      cancelLabel: $t("Cancel"),
      tone: "danger",
    });

    if (!confirmed) {
      return;
    }

    try {
      const download = await commands.deleteWorkDownload(product.workId);
      notifications.success($t("Deleted download for {0}", product.workId));
      setProductDownload(product.workId, download);
      await refreshLibraryAfterMutation();
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function markProductDownloaded(product: Product) {
    try {
      const fallbackRoot = settings.libraryRoot.trim() || (await settings.systemDownloadRoot());
      const selected = await native.chooseDirectory({
        canCreateDirectories: false,
        defaultPath: fallbackRoot || undefined,
        title: $t("Choose local folder for {0}", product.workId),
      });

      if (!selected) {
        return;
      }

      const download = await commands.markWorkDownloaded(
        product.workId,
        selected,
      );
      notifications.success($t("Marked {0} as downloaded", product.workId));
      setProductDownload(product.workId, download);
      await refreshLibraryAfterMutation();
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function cancelJob(job: JobSnapshot) {
    try {
      await commands.cancelJob(job.id);
      const workId = jobWorkId(job) ?? jobOutputString(job, "workId");
      notifications.info(
        workId
          ? $t("Cancellation requested for {0}", workId)
          : $t("Cancellation requested"),
      );
      await loadJobs();
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function loadAuditEvents() {
    const generation = auditGeneration.invalidate();
    auditLoading = true;

    try {
      const events = await commands.listAuditEvents(500);
      if (auditGeneration.current(generation)) auditEvents = events;
    } catch (err) {
      if (auditGeneration.current(generation)) notifications.error(errorMessage(err));
    } finally {
      if (auditGeneration.current(generation)) auditLoading = false;
    }
  }

  async function loadAuditLogDir() {
    try {
      const result = await commands.getAuditLogDir();
      auditLogDir = result.path;
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function openAuditLogDir() {
    try {
      await commands.openAuditLogDir();
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function openDownloadFolder() {
    try {
      await commands.openDownloadFolder();
    } catch (err) {
      notifications.error(errorMessage(err));
    }
  }

  async function openExternalUrl(url: string, label: string) {
    try {
      await native.openExternalUrl(url);
    } catch (err) {
      notifications.error($t("Failed to open {0}: {1}", label, errorMessage(err)));
    }
  }

  async function openDlsiteProductPage(workId: string) {
    await openExternalUrl(dlsiteProductPageUrl(workId), $t("{0} on DLsite", workId));
  }

  function accountStatusLabel(account: Account) {
    const activeJob = activeAccountSyncJob(account.id);

    if (activeJob) {
      return jobLabel(activeJob, $locale);
    }

    if (!account.enabled) {
      return $t("Disabled");
    }

    const latestJob = latestAccountSyncJob(account.id);

    if (latestJob?.status === "failed") {
      return $t("Sync failed");
    }

    if (latestJob?.status === "cancelled") {
      return $t("Sync cancelled");
    }

    if (account.lastSyncAt) {
      return $t("Synced");
    }

    return $t("Not synced");
  }

  function accountStatusTone(account: Account) {
    const activeJob = activeAccountSyncJob(account.id);

    if (activeJob) {
      return "syncing";
    }

    if (!account.enabled) {
      return "disabled";
    }

    const latestJob = latestAccountSyncJob(account.id);

    if (latestJob?.status === "failed") {
      return "failed";
    }

    if (latestJob?.status === "cancelled") {
      return "warning";
    }

    if (account.lastSyncAt) {
      return "synced";
    }

    return "idle";
  }

  function accountSyncJobs(accountId: string) {
    return jobController.jobs.filter(
      (job) => job.kind === "accountSync" && jobAccountId(job) === accountId,
    );
  }

  function activeAccountSyncJob(accountId: string) {
    return [...accountSyncJobs(accountId)].reverse().find(isActiveJob) ?? null;
  }

  function latestAccountSyncJob(accountId: string) {
    return [...accountSyncJobs(accountId)].reverse()[0] ?? null;
  }

  function activeWorkDownloadJob(workId: string) {
    return [...jobController.jobs].reverse().find((job) => isActiveJob(job) && (
      (job.kind === "workDownload" && jobWorkId(job) === workId) ||
      (job.kind === "bulkWorkDownload" && downloadMetrics(job)?.activeWorks.some((work) => work.workId === workId))
    )) ?? null;
  }

  function activeBulkDownloadPlanningJob() {
    return (
      [...jobController.jobs]
        .reverse()
        .find(
          (job) => job.kind === "bulkWorkDownloadPreview" && isActiveJob(job),
        ) ?? null
    );
  }

  function bulkDownloadButtonLabel() {
    if (!bulkDownloadPlanning) {
      return $t("Download filtered results");
    }

    const planningJob = activeBulkDownloadPlanningJob();

    return planningJob ? jobLabel(planningJob, $locale) : $t("Planning");
  }

  function currentDownloadJobs() {
    return jobController.jobs.filter(
      (job) => isDownloadQueueJob(job) && isActiveJob(job),
    );
  }

  function visibleAuditEvents() {
    return auditEvents;
  }

  function hasSyncableEnabledAccount() {
    return accountState.accounts.some(
      (account) => account.enabled && !activeAccountSyncJob(account.id),
    );
  }

  function jobDetail(job: JobSnapshot) {
    if (job.error?.message) {
      return job.error.message;
    }

    if (isActiveJob(job)) {
      const activeDetail = activeJobDetail(job, $locale);

      if (activeDetail) {
        return activeDetail;
      }
    }

    return (
      jobController.messages[job.id] ??
      shortDate(job.finishedAt ?? job.startedAt ?? job.createdAt)
    );
  }

  function downloadQueueTitle(job: JobSnapshot) {
    if (job.kind === "bulkWorkDownload") {
      const requested =
        jobOutputNumber(job, "requestedCount") ??
        metadataNumber(job, "reservedCount");
      return typeof requested === "number" && requested > 0
        ? $t("Bulk download ({0} works)", requested)
        : $t("Bulk download");
    }

    const workId = jobWorkId(job) ?? jobOutputString(job, "workId");
    const product = workId
      ? gallery.products.find((item) => item.workId === workId)
      : null;
    return product?.title ?? workId ?? job.title;
  }

  function showBulkDownloadDialog(
    preview: BulkWorkDownloadPreview, kind: BulkDownloadDialog["kind"],
    selectedProducts?: SelectedProduct[], space?: commands.SpaceReport,
  ) { return bulkConfirmation.show({ kind, preview, selectedProducts, space }); }

  function productDownloadActionDisabled(
    product: Product,
    job: JobSnapshot | null,
  ) {
    return (
      !!job ||
      (product.download.status === "downloaded" && !product.download.localPath)
    );
  }

  async function runProductDownloadAction(product: Product) {
    if (product.download.status === "downloaded") {
      await openDownloadedProduct(product);
      return;
    }

    await startWorkDownload(product);
  }

  function viewErrorActivity(target: DiagnosticTarget) {
    // Preserve an open editor, confirmation, or MFA prompt beneath the details.
    if (!hasOpenModal()) {
      openSettingsTab("logs");
    }
    activityTarget = target;
  }

  function navigate(view: View) {
    playlistMenu=null;
    creditHydration.clearQueue();
    if (view === "library" || view === "downloaded") {
      if (gallery.setScope(view)) selectionAnchorId = null;
      void gallery.load({ clampInvalidPage: true });
    }
    activeView = view;
    if (view === "playlists") void playlists.load().catch(cause=>notifications.error(errorMessage(cause)));
    if (view === "settings" && settingsTab === "logs") void loadAuditEvents();
  }

  $effect(() => {
    if (typeof document !== "undefined") document.documentElement.lang = $locale;
  });
</script>

<svelte:head>
  <title>DLDR — DLsite Downloader</title>
</svelte:head>

<AppShell
  {activeView}
  playerActive={Boolean(player.media)}
  version={settings.appInfo?.version ?? ""}
  onNavigate={navigate}
  onOpenDownloadFolder={openDownloadFolder}
  onOpenPlay={() => requestOnlinePlay()}
>
  {#snippet downloadStatus()}
    <DownloadStatus queue={downloadQueue} jobs={jobController.jobs.filter(job => downloadSessionIds.includes(job.id))}
      open={downloadPanelOpen} onToggle={toggleDownloadPanel} />
  {/snippet}
  {#if activeView === "library" || activeView === "downloaded"}
    <LibraryView
      metadataFilters={gallery.metadata}
      onCycleMetadataFilter={gallery.cycleMetadata}
      downloadedOnly={activeView === "downloaded"}
      {creditLoadingWorkIds}
      products={gallery.products}
      loading={gallery.loading}
      bind:search={gallery.search}
      filtersOpen={gallery.filtersOpen}
      facets={gallery.facets}
      sort={gallery.sort}
      selectedSources={gallery.sourceGroups}
      selectedAges={gallery.ageCategories}
      selectedTypes={gallery.typeGroups}
      selectedMakers={gallery.makerKeys}
      rangeLabel={gallery.rangeLabel($t)}
      pageNumber={gallery.pageIndex + 1}
      pageCount={gallery.pageCount}
      syncDisabled={accountState.loading ||
        jobController.loading ||
        !hasSyncableEnabledAccount()}
      bulkDisabled={bulkDownloadPlanning ||
        gallery.loading ||
        jobController.loading ||
        gallery.total === 0}
      bulkLabel={bulkDownloadButtonLabel()}
      selectedWorkIds={currentSelection.map((product) => product.workId)}
      selectedSizeLabel={activeView === "library" ? selectionSizeLabel(selectedProducts, $locale) : ""}
      selectedDownloadDisabled={localWorkBusy || bulkDownloadPlanning || jobController.loading}
      onToggleSelection={toggleSelection}
      onSelectPage={selectCurrentPage}
      onClearPage={deselectCurrentPage}
      onClearSelection={clearSelection}
      onReviewSelection={() => { if (!localWorkBusy) localWorkProgress = null; selectionReviewOpen = true; }}
      onDownloadSelected={() => startBulkWorkDownload(true)}
      onDeleteSelected={() => manageDownloadedSelection("delete")}
      onExportSelected={() => manageDownloadedSelection("export")}
      detailLoadingWorkId={productDetailLoadingWorkId}
      getActiveJob={(product) => activeWorkDownloadJob(product.workId)}
      onSearch={searchProducts}
      onReset={gallery.resetFilters}
      onToggleFilters={() => (gallery.filtersOpen = !gallery.filtersOpen)}
      onReload={reloadProducts}
      onSync={syncEnabledAccounts}
      onBulkDownload={() => startBulkWorkDownload(false)}
      onSetSort={gallery.setSort}
      onClearSources={() => gallery.clearFilter("sourceGroups")}
      onToggleSource={value => gallery.toggleFilter("sourceGroups", value)}
      onClearAges={() => gallery.clearFilter("ageCategories")}
      onToggleAge={value => gallery.toggleFilter("ageCategories", value)}
      onClearTypes={() => gallery.clearFilter("typeGroups")}
      onToggleType={value => gallery.toggleFilter("typeGroups", value)}
      onClearMakers={() => gallery.clearFilter("makerKeys")}
      onToggleMaker={value => gallery.toggleFilter("makerKeys", value)}
      onGoToPage={gallery.goToPage}
      onOpenDetails={openProductDetail}
      onOpenDlsite={openDlsiteProductPage}
      onOpenOnline={requestOnlinePlay}
      onDownload={runProductDownloadAction}
      onCopyWorkId={copyWorkId}
      onOpenPlayer={openPlayer}
      onOpenFolder={openDownloadedProduct}
      onTogglePlaylist={(work)=>void togglePlaylist(work)}
      onChoosePlaylist={choosePlaylist}
      playlistBusy={playlists.busy}
      defaultPlaylistWorkIds={playlists.lists.find(list=>list.id==="0")?.workIds ?? []}
      onTagSearch={(name) => {
        gallery.search = appendSearchToken(gallery.search,"tag",name);
        if (!parseProductSearch(gallery.search).error) void searchProducts();
      }}
      onPrecisionSearch={(kind,name) => {
        gallery.search = appendSearchToken(gallery.search,kind,name);
        if (!parseProductSearch(gallery.search).error) void searchProducts();
      }}
    />
  {:else if activeView === "playlists"}
    <PlaylistsView controller={playlists} onOpenDetails={openProductDetail} onCopyWorkId={copyWorkId} onOpenPlayer={openPlayer}
      onOpenFolder={openDownloadedProduct} onOpenDlsite={openDlsiteProductPage} onOpenOnline={requestOnlinePlay} onDownload={runProductDownloadAction} getActiveJob={(product) => activeWorkDownloadJob(product.workId)} onTogglePlaylist={(work)=>void togglePlaylist(work)}
      onChoosePlaylist={choosePlaylist} onError={notifications.error} detailLoadingWorkId={productDetailLoadingWorkId} />
  {:else}
    <SettingsView
      bind:tab={settingsTab}
      onTabChange={(tab) => { if (tab === "logs") void loadAuditEvents(); }}
      bind:uiLocale={$locale}
      onLocaleChange={settings.changeLocale}
      bind:libraryRoot={settings.libraryRoot}
      bind:downloadRoot={settings.downloadRoot}
      bind:autoExtract={settings.autoExtract}
      bind:languagePriority={settings.draftLanguagePriority}
      bind:titleLanguageFollowsUi={settings.titleLanguageFollowsUi}
      loading={settings.loading}
      saving={settings.saving}
      appInfo={settings.appInfo}
      appInfoLoading={settings.appInfoLoading}
      updateBusy={appUpdate.busy}
      onReload={settings.load}
      onChooseDirectory={settings.chooseDirectory}
      onUseDefaultDownloadRoot={settings.useDefaultDownloadRoot}
      onSave={settings.save}
      onOpenGitHub={() => openExternalUrl(GITHUB_URL, "GitHub")}
      onOpenDlsite={() => openExternalUrl(DLSITE_URL, "DLsite")}
      onCheckForUpdates={() => appUpdate.check()}
    >
      {#snippet accountSettings()}
        <AccountsView embedded
          accounts={accountState.accounts}
          loading={accountState.loading}
          saving={accountState.saving}
          jobsLoading={jobController.loading}
          editingAccountId={accountState.editingId}
          bind:label={accountState.label}
          bind:loginName={accountState.loginName}
          bind:password={accountState.password}

          syncAllDisabled={!hasSyncableEnabledAccount()}
          getActiveSyncJob={activeAccountSyncJob}
          getStatusLabel={accountStatusLabel}
          getStatusTone={accountStatusTone}
          onReload={accountState.load}
          onSyncAll={syncEnabledAccounts}
          onToggleEnabled={accountState.setEnabled}
          onEdit={accountState.edit}
          onSync={syncAccount}
          onCancelSync={cancelAccountSync}
          onRemove={accountState.remove}
          onReset={accountState.reset}
          onSave={accountState.save}
        />
      {/snippet}
      {#snippet logs()}
        <LogsView embedded
          onViewOperation={(target) => (activityTarget = target)}
          auditEvents={visibleAuditEvents()}
          {auditLoading}
          {auditLogDir}
          onOpenAuditFolder={openAuditLogDir}
          onReloadAudit={loadAuditEvents}
        />
      {/snippet}
    </SettingsView>
  {/if}

  <UpdateDialog update={appUpdate} onOpenReleases={openReleases} />
  <ConfirmationDialogView
    dialog={confirmations.dialog}
    onClose={confirmations.close}
  />

  {#if setupOpen}
    <FirstRunSetup
      accounts={accountState.accounts}
      jobs={jobController.jobs}
      initialLibraryRoot={settings.libraryRoot}
      initialDownloadRoot={settings.configuredDownloadRoot}
      onRefresh={refreshSetup}
      onComplete={finishSetup}
    />
  {/if}
  <TwoFactorDialog
    request={twoFactor.active}
    submitting={twoFactor.submitting}
    onSubmit={(code) => void twoFactor.submit(code)}
    onCancel={() => void twoFactor.cancel()}
  />
  <BulkDownloadDialogView
    bind:concurrency={batchConcurrency}
    dialog={bulkConfirmation.dialog}
    onClose={bulkConfirmation.close}
  />
  {#if selectionReviewOpen}
    {#if activeView === "downloaded"}
    <DownloadedWorksDialog
      products={downloadedSelection}
      busy={localWorkBusy}
      progress={localWorkProgress}
      onRemove={removeFromSelection}
      onClose={() => (selectionReviewOpen = false)}
      onDelete={() => manageDownloadedSelection("delete")}
      onExport={() => manageDownloadedSelection("export")}
    />
    {:else}
    <SelectedWorksDialog
      products={selectedProducts}
      disabled={bulkDownloadPlanning || jobController.loading}
      onRemove={removeFromSelection}
      onClose={() => (selectionReviewOpen = false)}
      onDownload={() => startBulkWorkDownload(true)}
    />
    {/if}
  {/if}
  <ProductDetailDialog
    detail={productDetail}
    activeJob={productDetail
      ? activeWorkDownloadJob(productDetail.workId)
      : null}
    downloadDisabled={productDetail
      ? productDownloadActionDisabled(
          productDetail,
          activeWorkDownloadJob(productDetail.workId),
        )
      : true}
    onDownload={() => productDetail && runProductDownloadAction(productDetail)}
    onDownloadArchives={() =>
      productDetail && downloadProductArchivesOnly(productDetail)}
    onMarkDownloaded={() =>
      productDetail && markProductDownloaded(productDetail)}
    onRedownload={() => productDetail && redownloadProduct(productDetail)}
    onDeleteDownload={() =>
      productDetail && deleteDownloadedProduct(productDetail)}
    onClose={closeProductDetail}
    onPreview={openProductImageFromDetail}
    onCopyText={copyText}
    onCopyWorkId={copyWorkId}
    onCopyCredit={copyCreditField}
    onOpenDlsite={openDlsiteProductPage}
    onOpenPlayer={openPlayer}
  />

  {#if productImagePreview}
    <ProductImagePreviewView
      onSave={commands.saveProductCover}
      preview={productImagePreview}
      onClose={closeProductImage}
    />
  {/if}

  {#if activityTarget}
    {#key activityTarget}
      <LogOperationDialog target={activityTarget} onClose={() => (activityTarget = null)} />
    {/key}
  {/if}

  <ToastStack toasts={notifications.toasts} onDismiss={notifications.dismiss} onViewActivity={viewErrorActivity} />
</AppShell>

{#if playlistMenu}
  <PlaylistMenu work={playlistMenu.work} point={playlistMenu.point} lists={playlists.lists} busy={playlists.busy}
    onToggle={(id)=>playlistMenu && void togglePlaylist(playlistMenu.work,id)} onClose={()=>playlistMenu=null}
    onManage={()=>navigate("playlists")} />
{/if}
<PlayerView {player} onExpand={() => (downloadPanelOpen = false)} />
{#if downloadPanelOpen}
  <DownloadProgressPanel queue={downloadQueue} jobs={currentDownloadJobs()}
    summaryJobs={jobController.jobs.filter(job => downloadSessionIds.includes(job.id))}
    getTitle={downloadQueueTitle} getDetail={jobDetail} onReload={loadJobs} onCancel={cancelJob}
    onClose={closeDownloadPanel} onSettings={() => openSettingsTab("downloads")} onLogs={() => openSettingsTab("logs")} />
{/if}

<style>
  :global(html),
  :global(body) {
    height: 100%;
    overflow: hidden;
  }

  :global(body) {
    margin: 0;
  }
</style>

{#if playLaunch}
  <PlayLaunchDialog accounts={playLaunch.accounts} workId={playLaunch.workId} bind:selectedAccount={playAccountId}
    busy={playOpening} error={playError} onOpen={() => void launchOnlinePlay()} onClose={() => { if (!playOpening) playLaunch = null; }} />
{/if}

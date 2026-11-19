<script lang="ts">
  import { t, locale, normalizeLocale, type UiLocale } from "$lib/i18n";
  import { onDestroy, onMount, untrack } from "svelte";
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
  import {
    LibraryQueryController,
    RequestGeneration,
  } from "$lib/controllers/library-query-controller";
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
    DEFAULT_LANGUAGE_PRIORITY,
    PRODUCT_PAGE_SIZE,
    DLSITE_URL,
    GITHUB_URL,
  } from "$lib/model/constants";
  import {
    buildLanguagePriority,
    defaultLanguagePriority,
    sameLanguagePriority,
  } from "$lib/utils/name-language-priority";
  import {
    errorMessage,
    formatBytes,
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
  import { ageTooltip, creditTextForKind, dlsiteProductPageUrl } from "$lib/utils/products";
  import type {
    Account,
    AppInfo,
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
    ProductFilterFacets,
    ProductImagePreview,
    StartWorkDownloadOptions,
    Toast,
    ToastKind,
    View,
  } from "$lib/model/types";

  onMount(() => initializeTheme(dark => {
    void native.setNativeHeaderTheme(dark).catch(error => console.warn("Native titlebar appearance:", error));
  }));

  const player = new PlayerController();
  const playlists = new PlaylistController();
  let playlistMenu = $state<{work:Product;point:{x:number;y:number;trigger:HTMLButtonElement}}|null>(null);
  onMount(() => {void playlists.load().catch(cause=>notifyError(errorMessage(cause)));});
  function choosePlaylist(work:Product,point:{x:number;y:number;trigger:HTMLButtonElement}) {playlistMenu={work,point};}
  async function togglePlaylist(work:Product,id="0") {
    const removing = playlists.lists.find(list=>list.id===id)?.workIds.includes(work.workId) ?? false;
    try {
      const updated = await playlists.edit({action:removing ? "remove" : "add",playlistId:id,workId:work.workId});
      if (updated) {
        const list=playlists.lists.find(list=>list.id===id);
        notifySuccess($t(removing ? "Removed {0} from {1}" : "Added {0} to {1}",work.workId,list?.name || $t("Playlist {0}",0)));
        playlistMenu=null;
      }
    } catch(cause) {notifyError(errorMessage(cause));}
  }
  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void player.start().then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(cause => notifyError(errorMessage(cause)));
    return () => { disposed = true; player.dispose(); unlisten?.(); };
  });
  function openPlayer(work: Product | ProductDetail) {
    downloadPanelOpen = false;
    closeProductDetail();
    void player.open(work);
  }

  type ProductLoadOptions = {
    resetPage?: boolean;
    clampInvalidPage?: boolean;
    background?: boolean;
  };

  let playLaunch = $state<{workId:string|null; accounts:Account[]} | null>(null);
  let playAccountId = $state("");
  let playOpening = $state(false);
  let playError = $state("");
  function requestOnlinePlay(product?: Product) {
    const available = accounts.filter(account => account.enabled && (!product || product.owners.some(owner => owner.accountId === account.id)));
    if (!available.length) {
      notifyError($t("Add or enable a matching account in Settings before opening DLsite Play."));
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

  let autoExtract = $state(true);
  let savedAutoExtract = $state(true);
  const downloadUnpackPolicy = $derived(savedAutoExtract ? "unpackWhenRecognized" as const : "keepArchives" as const);
  let libraryRoot = $state("");
  let setupOpen = $state(false);
  let configuredDownloadRoot = $state<string | null>(null);
  let downloadRoot = $state("");
  let settingsLoading = $state(true);
  let settingsSaving = $state(false);
  let draftLanguagePriority = $state([...DEFAULT_LANGUAGE_PRIORITY]);
  let savedLanguagePriority = $state([...DEFAULT_LANGUAGE_PRIORITY]);
  let titleLanguageFollowsUi = $state(true);
  let appInfo = $state<AppInfo | null>(null);
  let appInfoLoading = $state(true);
  const appUpdate = new UpdateController();
  onDestroy(() => appUpdate.dispose());

  let accounts = $state<Account[]>([]);
  let accountsLoading = $state(true);
  let accountSaving = $state(false);
  let editingAccountId = $state<string | null>(null);
  let accountLabel = $state("");
  let accountLoginName = $state("");
  let accountPassword = $state("");

  let products = $state<Product[]>([]);
  let creditLoadingWorkIds = $state<string[]>([]);
  let totalProducts = $state(0);
  let productsLoading = $state(true);
  let productPageIndex = $state(0);
  let bulkDownloadPlanning = $state(false);
  let selectedProducts = $state<SelectedProduct[]>([]);
  let selectionReviewOpen = $state(false);
  let selectionAnchorId: string | null = null;
  let metadataFilters = $state<commands.MetadataFilters>({ genreNames: [], excludedGenreNames: [], voiceNames: [], excludedVoiceNames: [] });
  let productSearch = $state("");
  let selectedProductTypes = $state<string[]>([]);
  let selectedAgeCategories = $state<string[]>([]);
  let selectedProductSources = $state<string[]>([]);
  let selectedMakerKeys = $state<string[]>([]);
  let productFilterFacets = $state<ProductFilterFacets>({
    makers: [],

  });
  let productSort = $state("latestPurchaseDesc");
  let libraryFiltersOpen = $state(false);
  let galleryScope = $state<"library" | "downloaded">("library");
  const galleryQueries: Partial<Record<"library" | "downloaded", {
    request: commands.ProductListRequest;
    filtersOpen: boolean;
  }>> = {};

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
  let toasts = $state<Toast[]>([]);
  let productImagePreview = $state<ProductImagePreview | null>(null);
  let productDetail = $state<ProductDetail | null>(null);
  let productDetailLoadingWorkId = $state<string | null>(null);
  let bulkDownloadDialog = $state<BulkDownloadDialog | null>(null);
  let batchConcurrency = $state(DEFAULT_DOWNLOAD_CONCURRENCY);
  let confirmationDialog = $state<ConfirmationDialog | null>(null);
  const twoFactor = new TwoFactorController((error) =>
    notifyError(errorMessage(error)),
  );
  const detailGeneration = new RequestGeneration();
  const settingsGeneration = new RequestGeneration();
  const auditGeneration = new RequestGeneration();
  const creditHydration = new CreditHydrationController(commands, {
    updated: applyProductCredits,
    pending: (ids) => { creditLoadingWorkIds = ids; },
  });
  const libraryQuery = new LibraryQueryController(commands, {
    loading: (value) => {
      productsLoading = value;
    },
    page: (page, index) => {
      products = page.products;
      totalProducts = page.totalCount;
      productPageIndex = index;
      const current = new Map(page.products.map((product) => [product.workId, product]));
      const refreshed = selectedProducts.map((product) => current.has(product.workId) ? selectedProduct(current.get(product.workId)!) : product);
      if (JSON.stringify(refreshed) !== JSON.stringify(selectedProducts)) updateSelection(refreshed);
      creditHydration.show(page.products);
    },
    facets: (value) => {
      productFilterFacets = value;
    },
    error: (error) => notifyError(errorMessage(error)),
  });

  let toastSequence = 0;
  let bulkDownloadDialogResolve: ((confirmed: boolean) => void) | null = null;
  let confirmationDialogResolve: ((confirmed: boolean) => void) | null = null;
  const toastTimers = new Map<string, ReturnType<typeof setTimeout>>();

  onMount(() => {
    try {
      batchConcurrency = downloadConcurrency(localStorage.getItem(DOWNLOAD_CONCURRENCY_KEY));
    } catch { /* The default still works if preference storage is unavailable. */ }
    try {
      selectedProducts = restoreSelection(localStorage.getItem(SELECTION_STORAGE_KEY));
    } catch {
      notifyError($t("Could not restore saved selections. Your library is unchanged."));
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
      });
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
    settingsGeneration.invalidate();
    auditGeneration.invalidate();
    for (const timer of toastTimers.values()) {
      clearTimeout(timer);
    }
    toastTimers.clear();

    if (bulkDownloadDialogResolve) {
      bulkDownloadDialogResolve(false);
      bulkDownloadDialogResolve = null;
    }

    if (confirmationDialogResolve) {
      confirmationDialogResolve(false);
      confirmationDialogResolve = null;
    }
  });

  async function loadInitial() {
    const [settingsReady, , accountsReady] = await Promise.all([
      loadSettings(),
      loadAppInfo(),
      loadAccounts(),
      loadProducts(),
      loadJobs(),
      loadAuditLogDir(),
      loadAuditEvents(),
    ]);
    if (settingsReady && accountsReady) setupOpen = firstRunStep(accounts, libraryRoot) !== null;
  }

  async function refreshSetup() {
    const [accountsReady, jobsReady] = await Promise.all([loadAccounts(), loadJobs()]);
    if (!accountsReady || !jobsReady) throw new Error($t("Could not refresh setup. Please try again."));
  }

  async function finishSetup() {
    const [settingsReady, accountsReady] = await Promise.all([loadSettings(), loadAccounts(), loadProducts()]);
    if (!settingsReady || !accountsReady) throw new Error($t("Could not refresh setup. Please try again."));
    setupOpen = false;
    resetAccountForm();
  }

  async function loadAppInfo() {
    appInfoLoading = true;

    try {
      appInfo = await native.getAppInfo();
    } catch (err) {
      appInfo = null;
      notifyError(errorMessage(err));
    } finally {
      appInfoLoading = false;
    }
  }

  async function loadSettings() {
    if (settingsSaving) return;
    const generation = settingsGeneration.invalidate();
    settingsLoading = true;

    try {
      const [settings, defaultDownloadRoot] = await Promise.all([
        commands.getSettings(),
        systemDownloadRoot(),
      ]);
      if (!settingsGeneration.current(generation)) return;
      $locale = normalizeLocale(settings.uiLocale);
      autoExtract = savedAutoExtract = settings.autoExtract ?? true;
      libraryRoot = settings.libraryRoot ?? "";
      configuredDownloadRoot = settings.downloadRoot;
      downloadRoot = settings.downloadRoot ?? defaultDownloadRoot;
      savedLanguagePriority = [...settings.languagePriority];
      draftLanguagePriority = buildLanguagePriority(
        savedLanguagePriority,
        normalizeLocale(settings.uiLocale),
      );
      titleLanguageFollowsUi = settings.titleLanguageFollowsUi;
      return true;
    } catch (err) {
      if (settingsGeneration.current(generation))
        notifyError(errorMessage(err));
      return false;
    } finally {
      if (settingsGeneration.current(generation)) settingsLoading = false;
    }
  }

  function changeUiLocale(next: UiLocale) {
    if (settingsLoading || settingsSaving) return;
    if (titleLanguageFollowsUi) draftLanguagePriority = defaultLanguagePriority(next);
    $locale = next;
  }

  async function checkForUpdates() {
    await appUpdate.check();
  }

  async function openReleases() {
    try {
      await native.openExternalUrl(`${GITHUB_URL}/releases`);
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function saveSettings(event: Event) {
    event.preventDefault();
    if (settingsLoading || settingsSaving) return;
    settingsSaving = true;

    try {
      const settings = await commands.saveSettings({
        autoExtract,
        uiLocale: $locale,
        libraryRoot: valueOrNull(libraryRoot),
        downloadRoot: valueOrNull(downloadRoot),
        titleLanguageFollowsUi,
        ...(titleLanguageFollowsUi ? {} : { languagePriority: draftLanguagePriority }),
      });
      autoExtract = savedAutoExtract = settings.autoExtract ?? true;
      const priorityChanged = !sameLanguagePriority(
        settings.languagePriority,
        savedLanguagePriority,
      );
      savedLanguagePriority = [...settings.languagePriority];
      draftLanguagePriority = buildLanguagePriority(
        savedLanguagePriority,
        normalizeLocale(settings.uiLocale),
      );
      titleLanguageFollowsUi = settings.titleLanguageFollowsUi;
      // Start invalidation synchronously after commit, before another await can
      // allow an old list/detail response to overwrite the saved projection.
      const refresh = priorityChanged
        ? refreshLibraryAfterMutation(true)
        : Promise.resolve();
      const [defaultDownloadRoot] = await Promise.all([
        systemDownloadRoot(),
        refresh,
      ]);
      $locale = normalizeLocale(settings.uiLocale);
      libraryRoot = settings.libraryRoot ?? "";
      downloadRoot = settings.downloadRoot ?? defaultDownloadRoot;
      notifySuccess($t("Settings saved"));
    } catch (err) {
      notifyError(errorMessage(err));
    } finally {
      settingsSaving = false;
    }
  }

  async function chooseSettingsDirectory(kind: "library" | "download") {
    try {
      const fallbackRoot = await systemDownloadRoot();
      const currentRoot = kind === "library" ? libraryRoot : downloadRoot;
      const selected = await native.chooseDirectory({
        canCreateDirectories: true,
        defaultPath: currentRoot.trim() || fallbackRoot || undefined,
        title:
          kind === "library"
            ? $t("Choose library folder")
            : $t("Choose download staging folder"),
      });

      if (!selected) {
        return;
      }

      if (kind === "library") {
        libraryRoot = selected;
      } else {
        downloadRoot = selected;
      }
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function useDefaultDownloadRoot() {
    const root = await systemDownloadRoot();

    if (root) {
      downloadRoot = root;
    }
  }

  async function systemDownloadRoot() {
    try {
      return await native.getSystemDownloadDirectory();
    } catch {
      return "";
    }
  }

  async function loadAccounts() {
    accountsLoading = true;

    try {
      accounts = await commands.listAccounts();
      return true;
    } catch (err) {
      notifyError(errorMessage(err));
      return false;
    } finally {
      accountsLoading = false;
    }
  }

  async function saveAccount(event: Event) {
    event.preventDefault();
    accountSaving = true;

    try {
      const account = await commands.saveAccount({
        id: editingAccountId,
        label: accountLabel,
        loginName: valueOrNull(accountLoginName),
        password: valueOrNull(accountPassword),
      });
      notifySuccess(editingAccountId ? $t("Account updated") : $t("Account added"));
      editAccount(account);
      accountPassword = "";
      await loadAccounts();
      return true;
    } catch (err) {
      notifyError(errorMessage(err));
      return false;
    } finally {
      accountSaving = false;
    }
  }

  async function setAccountEnabled(account: Account, enabled: boolean) {
    try {
      await commands.setAccountEnabled(account.id, enabled);
      await loadAccounts();
      await loadProducts({ clampInvalidPage: true });
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function removeAccount(account: Account) {
    const confirmed = await showConfirmationDialog({
      eyebrow: $t("Account source"),
      title: $t("Remove account?"),
      message: $t("Remove {0}. Its saved credential and ownership source will be deleted. Cached product metadata and downloaded local folders are kept.", account.label),
      confirmLabel: $t("Remove Account"),
      cancelLabel: $t("Cancel"),
      tone: "danger",
    });

    if (!confirmed) {
      return;
    }

    try {
      const report = await commands.removeAccount(account.id);

      notifySuccess($t("Removed {0}", report.label));

      if (editingAccountId === account.id) {
        resetAccountForm();
      }


      await Promise.all([loadAccounts(), loadProducts({ resetPage: true })]);
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  function editAccount(account: Account) {
    editingAccountId = account.id;
    accountLabel = account.label;
    accountLoginName = account.loginName ?? "";
    accountPassword = "";
  }

  function resetAccountForm() {
    editingAccountId = null;
    accountLabel = "";
    accountLoginName = "";
    accountPassword = "";
  }

  async function loadProducts(options: ProductLoadOptions = {}) {
    creditHydration.clearQueue();
    if (options.resetPage) productPageIndex = 0;
    await libraryQuery.load(productListRequest(), options.clampInvalidPage, { background: options.background });
  }

  function invalidateLibrary() {
    libraryQuery.invalidate();
    detailGeneration.invalidate();
  }

  async function refreshLibraryAfterMutation(resetPage = false) {
    invalidateLibrary();
    const detailId = productDetailLoadingWorkId ?? productDetail?.workId;
    await Promise.all([
      activeView === "playlists" ? playlists.load().catch(cause=>notifyError(errorMessage(cause))) : Promise.resolve(),
      loadProducts({ resetPage, clampInvalidPage: !resetPage, background: !resetPage }),
      detailId ? loadProductDetail(detailId, !resetPage) : Promise.resolve(),
    ]);
  }

  function cycleMetadataFilter(kind: "genre"|"voice", name: string) {
    const include = kind === "genre" ? "genreNames" : "voiceNames";
    const exclude = kind === "genre" ? "excludedGenreNames" : "excludedVoiceNames";
    if (metadataFilters[include].includes(name)) {
      metadataFilters[include] = metadataFilters[include].filter(v => v !== name);
      metadataFilters[exclude] = [...metadataFilters[exclude],name];
    } else if (metadataFilters[exclude].includes(name)) {
      metadataFilters[exclude] = metadataFilters[exclude].filter(v => v !== name);
    } else { metadataFilters[include] = [...metadataFilters[include],name]; }
    void loadProducts({resetPage:true});
  }

  function productListRequest(): commands.ProductListRequest {
    return {
      ...metadataFilters,
      downloadedOnly: galleryScope === "downloaded",
      search: valueOrNull(productSearch),
      accountIds: [],
      typeGroups: selectedProductTypes,
      ageCategories: selectedAgeCategories,
      sourceGroups: selectedProductSources,
      makerKeys: selectedMakerKeys,
      sort: productSort,
      limit: PRODUCT_PAGE_SIZE,
      offset: productPageOffset(),
    };
  }

  function productPageOffset() {
    return productPageIndex * PRODUCT_PAGE_SIZE;
  }

  function productPageCount() {
    return Math.max(1, Math.ceil(totalProducts / PRODUCT_PAGE_SIZE));
  }

  function productTotalLabel() {
    return $t(totalProducts === 1 ? "{0} work" : "{0} works", totalProducts);
  }

  function productRangeLabel() {
    if (productsLoading) {
      return totalProducts > 0
        ? $t("Loading {0}", productTotalLabel())
        : $t("Loading products");
    }

    if (totalProducts === 0 || products.length === 0) {
      return productTotalLabel();
    }

    const start = Math.min(productPageOffset() + 1, totalProducts);
    const end = Math.min(productPageOffset() + products.length, totalProducts);

    return $t("{0}-{1} of {2}", start, end, totalProducts);
  }

  async function reloadProducts() {
    creditHydration.retry();
    await loadProducts({ clampInvalidPage: true });
  }

  async function goToProductPage(page: number) {
    if (productsLoading || !Number.isInteger(page) || page < 1 || page > productPageCount()) return;
    productPageIndex = page - 1;
    await loadProducts({ clampInvalidPage: true });
  }

  function productBulkRequest(): commands.BulkWorkDownloadRequest {
    return {
      ...metadataFilters,
      search: valueOrNull(productSearch),
      accountIds: [],
      typeGroups: selectedProductTypes,
      ageCategories: selectedAgeCategories,
      sourceGroups: selectedProductSources,
      makerKeys: selectedMakerKeys,
      sort: productSort,
      unpackPolicy: downloadUnpackPolicy,
      skipDownloaded: true,
    };
  }

  function updateSelection(next: SelectedProduct[]) {
    selectedProducts = next;
    try {
      localStorage.setItem(SELECTION_STORAGE_KEY, JSON.stringify(next));
    } catch {
      notifyError($t("Selections are kept for this session, but could not be saved for the next launch."));
    }
  }

  function toggleSelection(product: Product, range: boolean) {
    updateSelection(toggleProductSelection(selectedProducts, product, products, selectionAnchorId, range));
    selectionAnchorId = product.workId;
  }

  function selectCurrentPage() {
    updateSelection(addProducts(selectedProducts, products));
    selectionAnchorId = null;
  }

  function deselectCurrentPage() {
    const visibleIds = new Set(products.map((product) => product.workId));
    updateSelection(selectedProducts.filter((product) => !visibleIds.has(product.workId)));
    selectionAnchorId = null;
  }

  function removeFromSelection(workId: string) {
    updateSelection(selectedProducts.filter((product) => product.workId !== workId));
  }

  function clearSelection() {
    updateSelection([]);
    selectionAnchorId = null;
  }

  function downloadAccountId() {
    const enabled = accounts.filter(account => account.enabled);
    return enabled.length === 1 ? enabled[0].id : null;
  }

  function toggleFilterValue(values: string[], value: string) {
    return values.includes(value)
      ? values.filter((candidate) => candidate !== value)
      : [...values, value];
  }

  async function toggleProductTypeFilter(typeGroup: string) {
    selectedProductTypes = toggleFilterValue(selectedProductTypes, typeGroup);
    await loadProducts({ resetPage: true });
  }

  async function toggleAgeFilter(ageCategory: string) {
    selectedAgeCategories = toggleFilterValue(
      selectedAgeCategories,
      ageCategory,
    );
    await loadProducts({ resetPage: true });
  }

  async function toggleProductSourceFilter(sourceGroup: string) {
    selectedProductSources = toggleFilterValue(
      selectedProductSources,
      sourceGroup,
    );
    await loadProducts({ resetPage: true });
  }

  async function toggleMakerFilter(makerKey: string) {
    selectedMakerKeys = toggleFilterValue(selectedMakerKeys, makerKey);
    await loadProducts({ resetPage: true });
  }

  async function clearTypeFilters() {
    selectedProductTypes = [];
    await loadProducts({ resetPage: true });
  }

  async function clearAgeFilters() {
    selectedAgeCategories = [];
    await loadProducts({ resetPage: true });
  }

  async function clearSourceFilters() {
    selectedProductSources = [];
    await loadProducts({ resetPage: true });
  }

  async function clearMakerFilters() {
    selectedMakerKeys = [];
    await loadProducts({ resetPage: true });
  }

  async function setProductSort(sort: string) {
    productSort = sort;
    await loadProducts({ resetPage: true });
  }

  async function resetLibraryFilters() {
    metadataFilters = { genreNames: [], excludedGenreNames: [], voiceNames: [], excludedVoiceNames: [] };
    productSearch = "";
    selectedProductTypes = [];
    selectedAgeCategories = [];
    selectedProductSources = [];
    selectedMakerKeys = [];
    productSort = "latestPurchaseDesc";
    await loadProducts({ resetPage: true });
  }

  async function loadJobs() {
    try {
      await jobController.load();
      return true;
    } catch (err) {
      notifyError(errorMessage(err));
      return false;
    }
  }

  async function handleJobEvent(event: JobEvent) {
    if (event.kind === "accountSync" && isTerminalJob(event.snapshot)) {
      await Promise.all([
        loadAccounts(),
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
    invalidateLibrary();
    products = products.map((product) =>
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
    await loadProducts({ resetPage: true });
  }

  async function copyWorkId(workId: string) {
    try {
      await navigator.clipboard.writeText(workId);
      notifySuccess($t("Copied work ID {0}", workId));
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function copyCreditField(field: ProductCreditField, workId?: string) {
    if (field.missing) {
      return;
    }

    try {
      await navigator.clipboard.writeText(field.value);
      notifySuccess(
        workId
          ? $t("Copied {0} for {1}", $t(field.label), workId)
          : $t("Copied {0}", $t(field.label)),
      );
    } catch (err) {
      notifyError(errorMessage(err));
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
      notifySuccess(
        workId ? $t("Copied {0} for {1}", label, workId) : $t("Copied {0}", label),
      );
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function openProductDetail(product: Product) {
    await loadProductDetail(product.workId);
  }

  function applyProductCredits(detail: ProductDetail) {
    products = products.map((product) => product.workId === detail.workId
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
      if (detailGeneration.current(generation)) notifyError(errorMessage(err));
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
        editingAccountId === account.id ? valueOrNull(accountPassword) : null,
      );
      notifyInfo($t("Sync queued"));
      jobController.setMessage(response.jobId, $t("Sync queued"));
      accountPassword = "";
      await loadJobs();
      return true;
    } catch (err) {
      notifyError(errorMessage(err));
      return false;
    }
  }

  async function syncEnabledAccounts() {
    const enabledAccounts = accounts.filter(
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
        unpackPolicy: options.unpackPolicy ?? downloadUnpackPolicy,
        replaceExisting: options.replaceExisting ?? false,
      });
      const queuedMessage = options.queuedMessage ?? $t("Download queued");
      notifyInfo($t("{0} for {1}", queuedMessage, product.workId));
      jobController.setMessage(response.jobId, queuedMessage);
      await loadJobs();
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function startBulkWorkDownload(selectedOnly = false) {
    if (bulkDownloadPlanning || (selectedOnly && !selectedProducts.length)) return;
    // Freeze IDs and metadata before the async preview. The confirmation must start
    // exactly this selection even if the Library query changes while planning.
    const selection = selectedOnly ? selectedProducts.map((item) => ({ ...item })) : undefined;
    const request = { ...(selection ? selectedDownloadRequest(selection, downloadUnpackPolicy) : productBulkRequest()), maxConcurrentWorks: batchConcurrency };
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
      notifyInfo($t("Bulk download queued"));
      jobController.setMessage(response.jobId, $t("Bulk download queued"));
      await loadJobs();
    } catch (err) {
      notifyError(errorMessage(err));
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
      notifyError(errorMessage(err));
    }
  }

  async function downloadProductArchivesOnly(product: Product) {
    await startWorkDownload(product, {
      unpackPolicy: "keepArchives",
      queuedMessage: $t("Archive-only download queued"),
    });
  }

  async function redownloadProduct(product: Product) {
    const confirmed = await showConfirmationDialog({
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
      unpackPolicy: downloadUnpackPolicy,
      replaceExisting: true,
      queuedMessage: $t("Re-download queued"),
    });
  }

  async function deleteDownloadedProduct(product: Product) {
    const confirmed = await showConfirmationDialog({
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
      notifySuccess($t("Deleted download for {0}", product.workId));
      setProductDownload(product.workId, download);
      await refreshLibraryAfterMutation();
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function markProductDownloaded(product: Product) {
    try {
      const fallbackRoot = libraryRoot.trim() || (await systemDownloadRoot());
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
      notifySuccess($t("Marked {0} as downloaded", product.workId));
      setProductDownload(product.workId, download);
      await refreshLibraryAfterMutation();
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function cancelJob(job: JobSnapshot) {
    try {
      await commands.cancelJob(job.id);
      const workId = jobWorkId(job) ?? jobOutputString(job, "workId");
      notifyInfo(
        workId
          ? $t("Cancellation requested for {0}", workId)
          : $t("Cancellation requested"),
      );
      await loadJobs();
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function clearFinishedJobs() {
    try {
      await commands.clearFinishedJobs();
      await loadJobs();
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function loadAuditEvents() {
    const generation = auditGeneration.invalidate();
    auditLoading = true;

    try {
      const events = await commands.listAuditEvents(500);
      if (auditGeneration.current(generation)) auditEvents = events;
    } catch (err) {
      if (auditGeneration.current(generation)) notifyError(errorMessage(err));
    } finally {
      if (auditGeneration.current(generation)) auditLoading = false;
    }
  }

  async function loadAuditLogDir() {
    try {
      const result = await commands.getAuditLogDir();
      auditLogDir = result.path;
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function openAuditLogDir() {
    try {
      await commands.openAuditLogDir();
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function openDownloadFolder() {
    try {
      await commands.openDownloadFolder();
    } catch (err) {
      notifyError(errorMessage(err));
    }
  }

  async function openExternalUrl(url: string, label: string) {
    try {
      await native.openExternalUrl(url);
    } catch (err) {
      notifyError($t("Failed to open {0}: {1}", label, errorMessage(err)));
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

  function syncingAccountCount() {
    return accounts.filter((account) => activeAccountSyncJob(account.id))
      .length;
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

  function visibleJobs() {
    return [...jobController.jobs].reverse();
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
    return accounts.some(
      (account) => account.enabled && !activeAccountSyncJob(account.id),
    );
  }

  function jobAccountLabel(job: JobSnapshot) {
    if (job.kind === "bulkWorkDownload") {
      return $t("Bulk download");
    }

    if (job.kind === "bulkWorkDownloadPreview") {
      return $t("Bulk planning");
    }

    if (job.kind === "workDownload") {
      return jobWorkId(job) ?? job.title;
    }

    const accountId = jobAccountId(job);
    const account = accounts.find((item) => item.id === accountId);
    return account?.label ?? accountId ?? job.title;
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
      ? products.find((item) => item.workId === workId)
      : null;
    return product?.title ?? workId ?? job.title;
  }

  function showBulkDownloadDialog(
    preview: BulkWorkDownloadPreview,
    kind: BulkDownloadDialog["kind"],
    selectedProducts?: SelectedProduct[],
    space?: commands.SpaceReport,
  ) {
    if (bulkDownloadDialogResolve) {
      bulkDownloadDialogResolve(false);
    }

    return new Promise<boolean>((resolve) => {
      bulkDownloadDialogResolve = resolve;
      bulkDownloadDialog = { kind, preview, selectedProducts, space };
    });
  }

  function closeBulkDownloadDialog(confirmed = false) {
    const resolve = bulkDownloadDialogResolve;

    bulkDownloadDialogResolve = null;
    bulkDownloadDialog = null;
    resolve?.(confirmed);
  }

  function showConfirmationDialog(dialog: ConfirmationDialog) {
    if (confirmationDialogResolve) {
      confirmationDialogResolve(false);
    }

    return new Promise<boolean>((resolve) => {
      confirmationDialogResolve = resolve;
      confirmationDialog = dialog;
    });
  }

  function closeConfirmationDialog(confirmed = false) {
    const resolve = confirmationDialogResolve;

    confirmationDialogResolve = null;
    confirmationDialog = null;
    resolve?.(confirmed);
  }

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

  function notifySuccess(message: string) {
    pushToast("success", message);
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
      if (galleryScope !== view) {
        // Each gallery keeps its own query and page; saved selections remain global.
        galleryQueries[galleryScope] = {
          request: JSON.parse(JSON.stringify(productListRequest())),
          filtersOpen: libraryFiltersOpen,
        };
        const saved = galleryQueries[view];
        const query = saved?.request;
        galleryScope = view;
        metadataFilters = { genreNames: query?.genreNames ?? [], excludedGenreNames: query?.excludedGenreNames ?? [], voiceNames: query?.voiceNames ?? [], excludedVoiceNames: query?.excludedVoiceNames ?? [] };
        productSearch = query?.search ?? "";
        selectedProductTypes = query?.typeGroups ?? [];
        selectedAgeCategories = query?.ageCategories ?? [];
        selectedProductSources = query?.sourceGroups ?? [];
        selectedMakerKeys = query?.makerKeys ?? [];
        productSort = query?.sort ?? "latestPurchaseDesc";
        productPageIndex = Math.floor((query?.offset ?? 0) / PRODUCT_PAGE_SIZE);
        libraryFiltersOpen = saved?.filtersOpen ?? false;
        products = [];
        totalProducts = 0;
        productFilterFacets = { makers: [] };
        selectionAnchorId = null;
      }
      void loadProducts({ clampInvalidPage: true });
    }
    activeView = view;
    if (view === "playlists") void playlists.load().catch(cause=>notifyError(errorMessage(cause)));
    if (view === "settings" && settingsTab === "logs") void loadAuditEvents();
  }

  function notifyInfo(message: string) {
    pushToast("info", message);
  }

  function notifyError(message: string) {
    pushToast("error", message, 7000);
  }

  function pushToast(kind: ToastKind, message: string, duration = 3600) {
    const id = `toast-${Date.now()}-${toastSequence++}`;
    const toast = { id, kind, message };
    toasts = [toast, ...toasts].slice(0, 5);

    const timer = setTimeout(() => dismissToast(id), duration);
    toastTimers.set(id, timer);
    clearOrphanedToastTimers();
  }

  function dismissToast(id: string) {
    toasts = toasts.filter((toast) => toast.id !== id);

    const timer = toastTimers.get(id);
    if (timer) {
      clearTimeout(timer);
      toastTimers.delete(id);
    }
  }

  function clearOrphanedToastTimers() {
    const visibleToastIds = new Set(toasts.map((toast) => toast.id));

    for (const [id, timer] of toastTimers.entries()) {
      if (!visibleToastIds.has(id)) {
        clearTimeout(timer);
        toastTimers.delete(id);
      }
    }
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
  version={appInfo?.version ?? ""}
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
      {metadataFilters}
      onCycleMetadataFilter={cycleMetadataFilter}
      downloadedOnly={activeView === "downloaded"}
      {creditLoadingWorkIds}
      {products}
      loading={productsLoading}
      bind:search={productSearch}
      filtersOpen={libraryFiltersOpen}
      facets={productFilterFacets}
      sort={productSort}
      selectedSources={selectedProductSources}
      selectedAges={selectedAgeCategories}
      selectedTypes={selectedProductTypes}
      selectedMakers={selectedMakerKeys}
      rangeLabel={productRangeLabel()}
      pageNumber={productPageIndex + 1}
      pageCount={productPageCount()}
      syncDisabled={accountsLoading ||
        jobController.loading ||
        !hasSyncableEnabledAccount()}
      bulkDisabled={bulkDownloadPlanning ||
        productsLoading ||
        jobController.loading ||
        totalProducts === 0}
      bulkLabel={bulkDownloadButtonLabel()}
      selectedWorkIds={activeView === "library" ? selectedProducts.map((product) => product.workId) : []}
      selectedSizeLabel={selectionSizeLabel(selectedProducts, $locale)}
      selectedDownloadDisabled={bulkDownloadPlanning || jobController.loading}
      onToggleSelection={activeView === "library" ? toggleSelection : undefined}
      onSelectPage={activeView === "library" ? selectCurrentPage : undefined}
      onClearPage={deselectCurrentPage}
      onClearSelection={clearSelection}
      onReviewSelection={() => (selectionReviewOpen = true)}
      onDownloadSelected={() => startBulkWorkDownload(true)}
      detailLoadingWorkId={productDetailLoadingWorkId}
      getActiveJob={(product) => activeWorkDownloadJob(product.workId)}
      onSearch={searchProducts}
      onReset={resetLibraryFilters}
      onToggleFilters={() => (libraryFiltersOpen = !libraryFiltersOpen)}
      onReload={reloadProducts}
      onSync={syncEnabledAccounts}
      onBulkDownload={() => startBulkWorkDownload(false)}
      onSetSort={setProductSort}
      onClearSources={clearSourceFilters}
      onToggleSource={toggleProductSourceFilter}
      onClearAges={clearAgeFilters}
      onToggleAge={toggleAgeFilter}
      onClearTypes={clearTypeFilters}
      onToggleType={toggleProductTypeFilter}
      onClearMakers={clearMakerFilters}
      onToggleMaker={toggleMakerFilter}
      onGoToPage={goToProductPage}
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
        productSearch = appendSearchToken(productSearch,"tag",name);
        if (!parseProductSearch(productSearch).error) void searchProducts();
      }}
      onPrecisionSearch={(kind,name) => {
        productSearch = appendSearchToken(productSearch,kind,name);
        if (!parseProductSearch(productSearch).error) void searchProducts();
      }}
    />
  {:else if activeView === "playlists"}
    <PlaylistsView controller={playlists} onOpenDetails={openProductDetail} onCopyWorkId={copyWorkId} onOpenPlayer={openPlayer}
      onOpenFolder={openDownloadedProduct} onOpenDlsite={openDlsiteProductPage} onOpenOnline={requestOnlinePlay} onDownload={runProductDownloadAction} getActiveJob={(product) => activeWorkDownloadJob(product.workId)} onTogglePlaylist={(work)=>void togglePlaylist(work)}
      onChoosePlaylist={choosePlaylist} onError={notifyError} detailLoadingWorkId={productDetailLoadingWorkId} />
  {:else}
    <SettingsView
      bind:tab={settingsTab}
      onTabChange={(tab) => { if (tab === "logs") void loadAuditEvents(); }}
      bind:uiLocale={$locale}
      onLocaleChange={changeUiLocale}
      bind:libraryRoot
      bind:downloadRoot
      bind:autoExtract
      bind:languagePriority={draftLanguagePriority}
      bind:titleLanguageFollowsUi
      loading={settingsLoading}
      saving={settingsSaving}
      {appInfo}
      {appInfoLoading}
      updateBusy={appUpdate.busy}
      onReload={loadSettings}
      onChooseDirectory={chooseSettingsDirectory}
      onUseDefaultDownloadRoot={useDefaultDownloadRoot}
      onSave={saveSettings}
      onOpenGitHub={() => openExternalUrl(GITHUB_URL, "GitHub")}
      onOpenDlsite={() => openExternalUrl(DLSITE_URL, "DLsite")}
      onCheckForUpdates={checkForUpdates}
    >
      {#snippet accountSettings()}
        <AccountsView embedded
          {accounts}
          loading={accountsLoading}
          saving={accountSaving}
          jobsLoading={jobController.loading}
          {editingAccountId}
          bind:label={accountLabel}
          bind:loginName={accountLoginName}
          bind:password={accountPassword}
          syncingCount={syncingAccountCount()}
          syncAllDisabled={!hasSyncableEnabledAccount()}
          getActiveSyncJob={activeAccountSyncJob}
          getStatusLabel={accountStatusLabel}
          getStatusTone={accountStatusTone}
          onReload={loadAccounts}
          onSyncAll={syncEnabledAccounts}
          onToggleEnabled={setAccountEnabled}
          onEdit={editAccount}
          onSync={syncAccount}
          onCancelSync={cancelAccountSync}
          onRemove={removeAccount}
          onReset={resetAccountForm}
          onSave={saveAccount}
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
    dialog={confirmationDialog}
    onClose={closeConfirmationDialog}
  />

  {#if setupOpen}
    <FirstRunSetup
      {accounts}
      jobs={jobController.jobs}
      initialLibraryRoot={libraryRoot}
      initialDownloadRoot={configuredDownloadRoot}
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
    dialog={bulkDownloadDialog}
    onClose={closeBulkDownloadDialog}
  />
  {#if selectionReviewOpen}
    <SelectedWorksDialog
      products={selectedProducts}
      disabled={bulkDownloadPlanning || jobController.loading}
      onRemove={removeFromSelection}
      onClose={() => (selectionReviewOpen = false)}
      onDownload={() => startBulkWorkDownload(true)}
    />
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

  <ToastStack {toasts} onDismiss={dismissToast} onViewActivity={viewErrorActivity} />
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

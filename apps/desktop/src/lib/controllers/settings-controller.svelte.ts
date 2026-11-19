import { get } from "svelte/store";
import { locale, normalizeLocale, t, type UiLocale } from "$lib/i18n";
import * as commands from "$lib/api/tauri";
import * as native from "$lib/api/native";
import type { AppInfo, AppSettings } from "$lib/model/types";
import { DEFAULT_LANGUAGE_PRIORITY } from "$lib/model/constants";
import { buildLanguagePriority, defaultLanguagePriority, sameLanguagePriority } from "$lib/utils/name-language-priority";
import { errorMessage, valueOrNull } from "$lib/utils/format";
import { RequestGeneration } from "$lib/utils/request-generation";
import type { Notifications } from "./feedback-controller.svelte";

type SettingsPort = Pick<typeof commands, "getSettings" | "saveSettings"> &
  Pick<typeof native, "getSystemDownloadDirectory" | "chooseDirectory" | "getAppInfo">;

export class SettingsController {
  autoExtract = $state(true);
  savedAutoExtract = $state(true);
  libraryRoot = $state("");
  downloadRoot = $state("");
  configuredDownloadRoot = $state<string | null>(null);
  loading = $state(true);
  saving = $state(false);
  draftLanguagePriority = $state([...DEFAULT_LANGUAGE_PRIORITY]);
  savedLanguagePriority = $state([...DEFAULT_LANGUAGE_PRIORITY]);
  titleLanguageFollowsUi = $state(true);
  appInfo = $state<AppInfo | null>(null);
  appInfoLoading = $state(true);
  unpackPolicy = $derived(this.savedAutoExtract ? "unpackWhenRecognized" as const : "keepArchives" as const);
  private generation = new RequestGeneration();
  private disposed = false;

  constructor(
    private readonly notices: Notifications,
    private readonly languageChanged: () => Promise<void>,
    private readonly api: SettingsPort = {
      getSettings: commands.getSettings,
      saveSettings: commands.saveSettings,
      getSystemDownloadDirectory: native.getSystemDownloadDirectory,
      chooseDirectory: native.chooseDirectory,
      getAppInfo: native.getAppInfo,
    },
  ) {}

  load = async () => {
    if (this.saving || this.disposed) return false;
    const generation = this.generation.invalidate();
    this.loading = true;
    try {
      const [settings, fallback] = await Promise.all([this.api.getSettings(), this.systemDownloadRoot()]);
      if (!this.generation.current(generation)) return false;
      this.apply(settings, fallback);
      return true;
    } catch (error) {
      if (this.generation.current(generation)) this.notices.error(errorMessage(error));
      return false;
    } finally {
      if (this.generation.current(generation)) this.loading = false;
    }
  };

  loadAppInfo = async () => {
    if (this.disposed) return;
    this.appInfoLoading = true;
    try {
      const info = await this.api.getAppInfo();
      if (!this.disposed) this.appInfo = info;
    } catch (error) {
      if (!this.disposed) this.notices.error(errorMessage(error));
    } finally {
      if (!this.disposed) this.appInfoLoading = false;
    }
  };

  changeLocale = (next: UiLocale) => {
    if (this.loading || this.saving || this.disposed) return;
    if (this.titleLanguageFollowsUi) this.draftLanguagePriority = defaultLanguagePriority(next);
    locale.set(next);
  };

  save = async (event: Event) => {
    event.preventDefault();
    if (this.loading || this.saving || this.disposed) return;
    this.saving = true;
    try {
      const saved = await this.api.saveSettings({
        autoExtract: this.autoExtract,
        uiLocale: get(locale),
        libraryRoot: valueOrNull(this.libraryRoot),
        downloadRoot: valueOrNull(this.downloadRoot),
        titleLanguageFollowsUi: this.titleLanguageFollowsUi,
        ...(this.titleLanguageFollowsUi ? {} : { languagePriority: [...this.draftLanguagePriority] }),
      });
      if (this.disposed) return;
      const changed = !sameLanguagePriority(saved.languagePriority, this.savedLanguagePriority);
      this.apply(saved, this.downloadRoot);
      // Invalidate list/detail requests immediately after committing the new projection.
      const refresh = changed ? this.languageChanged() : Promise.resolve();
      const [fallback] = await Promise.all([this.systemDownloadRoot(), refresh]);
      if (this.disposed) return;
      this.downloadRoot = saved.downloadRoot ?? fallback;
      this.notices.success(get(t)("Settings saved"));
    } catch (error) {
      if (!this.disposed) this.notices.error(errorMessage(error));
    } finally {
      if (!this.disposed) this.saving = false;
    }
  };

  chooseDirectory = async (kind: "library" | "download") => {
    if (this.disposed) return;
    try {
      const fallback = await this.systemDownloadRoot();
      if (this.disposed) return;
      const current = kind === "library" ? this.libraryRoot : this.downloadRoot;
      const selected = await this.api.chooseDirectory({
        canCreateDirectories: true,
        defaultPath: current.trim() || fallback || undefined,
        title: get(t)(kind === "library" ? "Choose library folder" : "Choose download staging folder"),
      });
      if (!selected || this.disposed) return;
      if (kind === "library") this.libraryRoot = selected;
      else this.downloadRoot = selected;
    } catch (error) {
      if (!this.disposed) this.notices.error(errorMessage(error));
    }
  };

  useDefaultDownloadRoot = async () => {
    const root = await this.systemDownloadRoot();
    if (root && !this.disposed) this.downloadRoot = root;
  };

  async systemDownloadRoot() {
    try { return await this.api.getSystemDownloadDirectory(); }
    catch { return ""; }
  }

  dispose() { this.disposed = true; this.generation.invalidate(); }

  private apply(settings: AppSettings, fallback: string) {
    const nextLocale = normalizeLocale(settings.uiLocale);
    locale.set(nextLocale);
    this.autoExtract = this.savedAutoExtract = settings.autoExtract ?? true;
    this.libraryRoot = settings.libraryRoot ?? "";
    this.configuredDownloadRoot = settings.downloadRoot;
    this.downloadRoot = settings.downloadRoot ?? fallback;
    this.savedLanguagePriority = [...settings.languagePriority];
    this.draftLanguagePriority = buildLanguagePriority(this.savedLanguagePriority, nextLocale);
    this.titleLanguageFollowsUi = settings.titleLanguageFollowsUi;
  }
}

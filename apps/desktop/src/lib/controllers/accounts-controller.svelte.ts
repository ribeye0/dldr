import { get } from "svelte/store";
import { t } from "$lib/i18n";
import * as commands from "$lib/api/tauri";
import type { Account, ConfirmationDialog } from "$lib/model/types";
import { errorMessage, valueOrNull } from "$lib/utils/format";
import { RequestGeneration } from "$lib/utils/request-generation";
import type { Notifications } from "./feedback-controller.svelte";

type AccountsPort = Pick<typeof commands, "listAccounts" | "saveAccount" | "setAccountEnabled" | "removeAccount">;

export class AccountsController {
  accounts = $state<Account[]>([]);
  loading = $state(true);
  saving = $state(false);
  editingId = $state<string | null>(null);
  label = $state("");
  loginName = $state("");
  password = $state("");
  private generation = new RequestGeneration();
  private disposed = false;

  constructor(
    private readonly notices: Notifications,
    private readonly confirm: (dialog: ConfirmationDialog) => Promise<boolean>,
    private readonly changed: (resetPage: boolean) => Promise<void>,
    private readonly api: AccountsPort = {
      listAccounts: commands.listAccounts,
      saveAccount: commands.saveAccount,
      setAccountEnabled: commands.setAccountEnabled,
      removeAccount: commands.removeAccount,
    },
  ) {}

  load = async () => {
    if (this.disposed) return false;
    const generation = this.generation.invalidate();
    this.loading = true;
    try {
      const accounts = await this.api.listAccounts();
      if (!this.generation.current(generation)) return false;
      this.accounts = accounts;
      return true;
    } catch (error) {
      if (this.generation.current(generation)) this.notices.error(errorMessage(error));
      return false;
    } finally {
      if (this.generation.current(generation)) this.loading = false;
    }
  };

  save = async (event: Event) => {
    event.preventDefault();
    if (this.saving || this.disposed) return false;
    this.saving = true;
    const editing = this.editingId;
    try {
      const account = await this.api.saveAccount({
        id: editing, label: this.label,
        loginName: valueOrNull(this.loginName), password: valueOrNull(this.password),
      });
      if (this.disposed) return false;
      this.edit(account);
      this.notices.success(get(t)(editing ? "Account updated" : "Account added"));
      return await this.load();
    } catch (error) {
      if (!this.disposed) this.notices.error(errorMessage(error));
      return false;
    } finally {
      if (!this.disposed) this.saving = false;
    }
  };

  setEnabled = async (account: Account, enabled: boolean) => {
    if (this.disposed) return;
    try {
      await this.api.setAccountEnabled(account.id, enabled);
      if (!this.disposed) await Promise.all([this.load(), this.changed(false)]);
    } catch (error) {
      if (!this.disposed) this.notices.error(errorMessage(error));
    }
  };

  remove = async (account: Account) => {
    if (this.disposed) return;
    const translate = get(t);
    const confirmed = await this.confirm({
      eyebrow: translate("Account source"), title: translate("Remove account?"),
      message: translate("Remove {0}. Its saved credential and ownership source will be deleted. Cached product metadata and downloaded local folders are kept.", account.label),
      confirmLabel: translate("Remove Account"), cancelLabel: translate("Cancel"), tone: "danger",
    });
    if (!confirmed || this.disposed) return;
    try {
      const report = await this.api.removeAccount(account.id);
      if (this.disposed) return;
      if (this.editingId === account.id) this.reset();
      this.notices.success(get(t)("Removed {0}", report.label));
      await Promise.all([this.load(), this.changed(true)]);
    } catch (error) {
      if (!this.disposed) this.notices.error(errorMessage(error));
    }
  };

  edit = (account: Account) => {
    this.editingId = account.id;
    this.label = account.label;
    this.loginName = account.loginName ?? "";
    this.password = "";
  };

  reset = () => {
    this.editingId = null;
    this.label = this.loginName = this.password = "";
  };

  dispose() {
    this.disposed = true;
    this.generation.invalidate();
    this.password = "";
  }
}

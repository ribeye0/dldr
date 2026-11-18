<script lang="ts">
  import { t } from "$lib/i18n";
  import Button from "$lib/components/ui/Button.svelte";
  import Field from "$lib/components/ui/Field.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";
  import FormActions from "$lib/components/ui/FormActions.svelte";

  let {
    editing = false,
    saving = false,
    label = $bindable(""),
    loginName = $bindable(""),
    password = $bindable(""),
    onReset,
    onSave,
  }: {
    editing?: boolean;
    saving?: boolean;
    label?: string;
    loginName?: string;
    password?: string;
    onReset: () => void;
    onSave: (event: SubmitEvent) => void;
  } = $props();
</script>

<form onsubmit={onSave} class="dldr:flex dldr:flex-col dldr:gap-4">
  <Field id="account-label" label={$t("Account name")}
    ><TextInput
      id="account-label"
      autocomplete="off"
      disabled={saving}
      bind:value={label}
    /></Field
  >
  <Field id="account-login" label={$t("Email or username")}
    ><TextInput
      id="account-login"
      autocomplete="username"
      spellcheck={false}
      disabled={saving}
      bind:value={loginName}
    /></Field
  >
  <Field id="account-password" label={$t("Password")}
    ><TextInput
      id="account-password"
      type="password"
      autocomplete="current-password"
      placeholder={editing ? $t("Leave blank to keep saved password") : ""}
      disabled={saving}
      bind:value={password}
    /></Field
  >
  <FormActions>
    <Button variant="primary" type="submit" disabled={saving}
      >{saving ? $t("Saving…") : $t("Save account")}</Button
    >
  </FormActions>
</form>

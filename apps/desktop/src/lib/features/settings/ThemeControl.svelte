<script lang="ts">
  import { t } from "$lib/i18n";
  import { theme, setTheme, themePreferences, type ThemePreference } from "$lib/theme";
  import Field from "$lib/components/ui/Field.svelte";
  const labels = { light: "Light", dark: "Dark", system: "Follow system" };
</script>

<div class="dldr:mb-6">
  <Field id="ui-theme" label={$t("Color theme")}>
    <select id="ui-theme" value={$theme} aria-describedby="ui-theme-hint"
      onchange={event => setTheme(event.currentTarget.value as ThemePreference)}
      class="dldr:w-full dldr:max-w-sm dldr:rounded-md dldr:border dldr:border-solid dldr:border-ui-line dldr:bg-ui-surface dldr:p-3 dldr:text-ui-ink dldr:text-base dldr:ui-focus-field">
      {#each themePreferences as preference}<option value={preference}>{$t(labels[preference])}</option>{/each}
    </select>
    <p id="ui-theme-hint" class="dldr:max-w-2xl dldr:text-sm dldr:text-ui-dim">{$t("Applies immediately and is remembered on this device. Follow system changes with your system appearance.")}</p>
  </Field>
</div>

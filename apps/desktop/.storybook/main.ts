import type { StorybookConfig } from "@storybook/sveltekit";
import { fileURLToPath } from "node:url";
import { mergeConfig } from "vite";

const config: StorybookConfig = {
  stories: ["../previews/**/*.stories.@(js|ts|svelte)"],
  addons: ["@storybook/addon-a11y"],
  framework: {
    name: "@storybook/sveltekit",
    options: {},
  },
  staticDirs: ["../static"],
  viteFinal: (config) => mergeConfig(config, {
    server: {
      fs: {
        allow: [
          fileURLToPath(new URL("../previews", import.meta.url)),
          fileURLToPath(new URL("../../../resources", import.meta.url)),
        ],
      },
    },
  }),
};

export default config;

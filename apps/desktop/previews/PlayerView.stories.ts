import type { Meta, StoryObj } from "@storybook/sveltekit";
import PlayerViewStory from "./harnesses/PlayerViewStory.svelte";
const meta = {
  title: "Player/Audio player",
  component: PlayerViewStory,
  parameters: {layout:"fullscreen"},
} satisfies Meta<typeof PlayerViewStory>;
export default meta;
export const TraditionalChinese: StoryObj<typeof meta> = {};
export const English: StoryObj<typeof meta> = {args:{language:"en"}};
export const Japanese: StoryObj<typeof meta> = {args:{language:"ja"}};
export const SimplifiedChinese: StoryObj<typeof meta> = {args:{language:"zh-CN"}};

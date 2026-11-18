import type { Meta, StoryObj } from "@storybook/sveltekit";
import AppShellStory from "./harnesses/AppShellStory.svelte";

const meta = {
  title: "Shell/App Shell",
  component: AppShellStory,
  parameters: { layout: "fullscreen" },
  args: { initialView: "library" },
  argTypes: {
    initialView: {
      control: "select",
      options: ["library", "downloaded", "playlists", "settings"],
    },
  },
} satisfies Meta<typeof AppShellStory>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Library: Story = {};

export const Downloaded: Story = {
  args: { initialView: "downloaded" },
};

export const Settings: Story = {
  args: { initialView: "settings" },
};

import type { Meta, StoryObj } from "@storybook/sveltekit";
import LogsViewStory from "./harnesses/LogsViewStory.svelte";

const meta = {
  title: "Activity/View",
  component: LogsViewStory,
  parameters: { layout: "fullscreen" },
  args: { viewState: "populated", withAuditDirectory: true },
} satisfies Meta<typeof LogsViewStory>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Populated: Story = {};
export const Loading: Story = { args: { viewState: "loading" } };
export const Empty: Story = { args: { viewState: "empty" } };
export const MissingAuditDirectory: Story = { args: { withAuditDirectory: false } };
export const InAppShell: Story = { args: { inAppShell: true } };

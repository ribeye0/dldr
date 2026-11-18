// Keep notifications in the active modal for input; a manual popover lifts them
// above the native dialog clipping boundary.
const dialogs: HTMLDialogElement[] = [];
const listeners = new Set<() => void>();
function activeDialog() {
  return dialogs.findLast(dialog => dialog.open && dialog.isConnected);
}
export function hasOpenModal() {
  return Boolean(activeDialog());
}
export function registerModal(dialog: HTMLDialogElement) {
  dialogs.push(dialog);
  for (const notify of listeners) notify();
  return () => {
    const index = dialogs.indexOf(dialog);
    if (index >= 0) dialogs.splice(index, 1);
    for (const notify of listeners) notify();
  };
}
export function notificationPortal(node: HTMLElement) {
  const parent = node.parentNode;
  const observer = new ResizeObserver(position);
  function position() {
    const dialog = activeDialog();
    const header = dialog?.querySelector(":scope > header");
    const top = header ? header.getBoundingClientRect().bottom + 8 : 56;
    node.style.setProperty("--toast-top", `${top}px`);
  }
  function move() {
    observer.disconnect();
    if (node.matches(":popover-open")) node.hidePopover();
    (activeDialog() ?? parent)?.appendChild(node);
    if (node.isConnected) node.showPopover();
    position();
    const dialog = activeDialog();
    if (dialog) observer.observe(dialog);
  }
  window.addEventListener("resize", position);
  listeners.add(move);
  move();
  return {
    destroy() {
      if (node.matches(":popover-open")) node.hidePopover();
      observer.disconnect();
      window.removeEventListener("resize", position);
      listeners.delete(move);
      node.remove();
    },
  };
}

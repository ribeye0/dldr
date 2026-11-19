import { appendSearchToken, parseProductSearch } from "./product-search";

export type MetadataSearchKind = "circle" | "va" | "tag";
export type ListPosition = { scrollTop: number; focusLabel?: string; workId?: string };

/** Consume native browser navigation while a filter page owns the Back action. */
export function installFilterBackNavigation(target: Window, canGoBack: () => boolean, onBack: () => void) {
  let mousePressed = false;
  function consume(event: Event) { event.preventDefault(); event.stopPropagation(); }
  function keyboard(event: KeyboardEvent) {
    if (event.defaultPrevented || event.isComposing || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (!(event.altKey && event.key === "ArrowLeft") && event.key !== "BrowserBack") return;
    consume(event);
    if (!event.repeat && canGoBack()) onBack();
  }
  function mouseDown(event: MouseEvent) {
    if (event.button !== 3) return;
    mousePressed = !event.defaultPrevented && canGoBack();
    consume(event);
  }
  function mouseUp(event: MouseEvent) {
    if (event.button !== 3) return;
    const pressed = mousePressed;
    mousePressed = false;
    consume(event);
    if (pressed && canGoBack()) onBack();
  }
  function auxiliary(event: MouseEvent) { if (event.button === 3) consume(event); }
  function resetMouse() { mousePressed = false; }
  target.addEventListener("keydown", keyboard, true);
  target.addEventListener("mousedown", mouseDown, true);
  target.addEventListener("mouseup", mouseUp, true);
  target.addEventListener("auxclick", auxiliary, true);
  target.addEventListener("blur", resetMouse);
  return () => {
    target.removeEventListener("keydown", keyboard, true);
    target.removeEventListener("mousedown", mouseDown, true);
    target.removeEventListener("mouseup", mouseUp, true);
    target.removeEventListener("auxclick", auxiliary, true);
    target.removeEventListener("blur", resetMouse);
  };
}

/** A drill-down keeps the applied query, never an unfinished search-box draft. */
export function refineSearch(input: string, kind: MetadataSearchKind, name: string): string | null {
  const value = name.trim();
  if (!value || /[$\r\n]/.test(value) || parseProductSearch(input).error) return null;
  const next = appendSearchToken(input, kind, value);
  return next === input || parseProductSearch(next).error ? null : next;
}

export class FilterHistory<T> {
  entries = $state<{ state: T; position: ListPosition }[]>([]);
  push(state: T, position: ListPosition) {
    // Query arrays can be Svelte proxies; later edits must not alter older pages.
    this.entries = [...this.entries, JSON.parse(JSON.stringify({ state, position }))];
  }
  pop() {
    const entry = this.entries.at(-1);
    this.entries = this.entries.slice(0, -1);
    return entry;
  }
  clear() { this.entries = []; }
}

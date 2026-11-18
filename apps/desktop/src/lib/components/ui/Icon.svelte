<script lang="ts">
  let { name }: { name: string } = $props();
  const paths: Record<string, string> = {
    playlists: "M4 5h12M4 10h12M4 15h7M16 14v7M12.5 17.5h7",
    chevronDown: "m6 9 6 6 6-6",
    chevronUp: "m6 15 6-6 6 6",
    music: "M9 18V5l12-2v13M9 8l12-2M9 18a3 3 0 1 1-3-3c1.7 0 3 1.3 3 3M21 16a3 3 0 1 1-3-3c1.7 0 3 1.3 3 3",
    script: "M5 3h14v18H5zM8 7h8M8 11h8M8 15h5",
    play: "m8 5 11 7-11 7z",
    pause: "M8 5v14M16 5v14",
    previous: "M5 5v14M19 5 8 12l11 7z",
    next: "M19 5v14M5 5l11 7-11 7z",
    stop: "M6 6h12v12H6z",
    list: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
    library: "M4 4h6v16H4zM14 4h6v16h-6zM4 8h6M14 15h6",
    downloaded: "M3 6h7l2 3h9v11H3zM8 14l3 3 5-5",
    downloads: "M12 3v12m-4-4 4 4 4-4M4 17v4h16v-4",
    activity: "M3 12h4l3-7 4 14 3-7h4",
    accounts:
      "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M13 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0M18 8a3 3 0 0 1 0 6M22 21v-2a4 4 0 0 0-3-4",
    settings: "M4 6h16M4 12h16M4 18h16M8 3v6M16 9v6M10 15v6",
    filter: "M4 5h16M7 12h10M10 19h4",
    search: "M21 21l-6-6M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0",
    up: "M12 20V5m-6 6 6-6 6 6M5 3h14",
    copy: "M9 9h11v11H9zM15 5V3H3v12h2",
    arrow: "M5 12h14m-5-5 5 5-5 5",
    close: "m6 6 12 12M6 18 18 6",
    check: "m5 12 4 4L19 6",
    clock: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0M12 6v6l4 2",
    unpack: "m3 8 9 4 9-4M12 12v9M3 8v9l9 4 9-4V8M3 8l4-5 5 4 5-4 4 5M12 7v5",
    plus: "M12 4v16M4 12h16",
    refresh: "M20 7v5h-5M4 17v-5h5M6 6a8 8 0 0 1 13 3M5 15a8 8 0 0 0 13 3",
    folder: "M3 6h7l2 3h9v11H3z",
    info: "M12 11v6M12 7h.01M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0",
  };
</script>

<svg
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.65"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"><path d={paths[name] ?? paths.info} /></svg
>

<style>
  svg {
    width: 17px;
    height: 17px;
    flex: none;
  }
</style>

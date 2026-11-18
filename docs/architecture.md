# Architecture

DLDR uses Svelte 5 for presentation, Tauri 2 for desktop integration and shared
Rust services for its GUI and CLI.

## Source layout

| Path | Responsibility |
| --- | --- |
| `apps/desktop/src/routes/` | Application composition and page state |
| `apps/desktop/src/lib/features/` | Library, accounts, downloads, player and settings views |
| `apps/desktop/src/lib/components/` | Shared interface components |
| `apps/desktop/src/lib/controllers/` | Frontend request, queue and playback coordination |
| `apps/desktop/src/lib/api/` | Typed frontend/native boundary |
| `apps/desktop/src/lib/i18n/`, `apps/desktop/src/lib/styles/` | Four interface languages and shared theme roles |
| `apps/desktop/src-tauri/` | Native commands, events, windows and desktop integration |
| `apps/cli/` | `dldr` command-line adapter |
| `crates/library/` | Account/library services and download coordination |
| `crates/storage/` | SQLite, migrations, cached queries, playlists and backups |
| `crates/api/`, `crates/credentials/` | DLsite requests and local credentials |
| `crates/download/`, `crates/archive/` | Transfers, rate limiting and safe extraction |
| `crates/jobs/`, `crates/audit/` | Background jobs, events and sanitized diagnostics |
| `crates/player/` | libmpv, local track discovery, VTT and playback history |
| `apps/desktop/previews/` | Component/application previews using synthetic data |
| `resources/` | Canonical four-language genre catalogue shared by Rust and Svelte |
| `tools/dev/`, `tools/install/` | Local development and user installation helpers |
| `tools/qa/`, `tools/release/` | Regression checks and release tooling |

The repository root contains workspace manifests, toolchain pins and the main
project guides. Frontend dependencies and configuration belong to the desktop
app. Preview fixtures stay outside shipped `src/`; shared data must not depend
on either adapter. Cargo output and distributable archives belong under `target/`
(`target/dist/` for release artifacts), while Vite output stays inside the desktop
app. Editor preferences and personal development records are not part of the
repository.

Business logic belongs in the shared Rust crates. Tauri commands validate input,
call services and translate results/events. The CLI uses the same services;
Svelte handles user interaction and presentation.

## Library and downloads

The GUI and mutating CLI commands share a per-profile process lock. Cached CLI
queries use read-only storage and can coexist with the GUI. Shared profile resolution
uses the system config directory plus `dldr`: `~/.config/dldr` on Linux and
`%APPDATA%\dldr` on Windows. Browser storage and logs also use this root.
See [data compatibility](linux-install.md#data-compatibility-and-credentials).
Linux GTK windows and desktop entries use `io.github.ribeye0.dldr` so desktop
launchers associate running windows with the DLDR icon. The native executable is
`dldr-desktop`, and the command-line executable is `dldr`. Personal Linux builds
install the `dldr-gui` launcher and a matching desktop entry.

Downloads use a persistent per-work queue, bounded concurrency and one shared
rate limiter. Incomplete requests recover as paused after restart. Explicit
resume reuses their account, paths and extraction policy. Archive extraction
rejects traversal, and finalization records support recovery across restarts.

Library/Downloaded filtering, counts and pagination run in storage before rows
are returned. Downloaded requires a completed record with a local path; folder
verification is an explicit maintenance action. Selection is independent of the
current page and query.

## Names and search

Titles retain their localized source values; display order follows the saved
language preference. Circle and voice credits are separate from genres. Missing
production credits may be supplemented from public product metadata, with bounded,
deduplicated requests and cached results. Only supplied production fields appear
in work details. The original product introduction is loaded on demand from the
public DLsite page, with sanitized HTML and external links opened in the browser.
Custom-tag editing and filtering are no longer exposed; legacy database records
remain intact for backup compatibility. The GUI targets a single-account workflow.

Genre IDs identify entries in the offline four-language catalogue. All language
labels and explicit aliases participate in search. Update rules are next to
[`resources/genres.json`](../resources/README.md). Keyword and
`$circle:…$`, `$va:…$`, `$tag:…$`, `$-tag:…$` conditions are shared by GUI/CLI;
queries operate on locally cached metadata.
Typing `$` in the library search offers supported syntax and cached facet names,
including genre aliases across languages. Completion edits the query at the
caret; searching remains an explicit action.

## Local playback

`dldr-player` scans selected local works and plays through libmpv. Archive entries
are listed read-only using libarchive; mpv streams a selected entry directly,
without extracting audio to disk. VTT reads are bounded to 8 MiB. Archive duration
metadata is learned during playback, avoiding a full decompression for probing.
ZIP, RAR and 7z support depends on libarchive; protected or damaged archives report
a warning while other audio remains playable. GTK startup resets only LC_NUMERIC
to C before creating workers, as required by libmpv.

Native services own media paths, track probing and playback state; the frontend renders controls
and the synchronized VTT script. Collapsing the player preserves playback.
Subtitles can be paired automatically or selected manually.
The slider keeps its own drag preview so playback clock events cannot overwrite
a pending gesture. The controller serializes seeks and keeps the latest pending
destination. Native clock reconciliation accounts for playback speed and delayed
polling, preserving the requested timestamp while paused audio awaits decoding.
Track/subtitle IDs use `/` on every platform; filesystem boundary comparisons
use canonical roots, including Windows extended-length paths.

## Official DLsite Play

Official playback uses a separate, account-scoped WebView and browser profile.
Its cookies are independent of download credentials. Navigation and a restricted
origin/route-checked bridge can enqueue owned, cached works through the existing
download coordinator. Remote pages do not receive general native IPC access.
The local application WebView has its own Content Security Policy.

See [DLsite Play usage](dlsite-play.md), [CLI usage](dldr.md) and the
[release guide](releasing.md) for operation and verification.

## Rust module layout

The Cargo workspace groups reusable services under `crates/`. Each crate keeps a
small public facade in `src/lib.rs`; feature implementations and unit tests live
in child modules. Package names use the `dldr-` prefix; Rust imports use `dldr_`.

- `apps/desktop/src-tauri/src/app.rs` owns startup, application state registration and IPC routing.
  `state.rs` holds shared desktop state; `dto.rs` defines IPC data and conversions.
- `apps/desktop/src-tauri/src/commands/` groups adapters by accounts, library, downloads, jobs,
  settings, player, playlists and desktop actions. `errors.rs` translates typed
  failures, and `progress.rs` bridges service progress to desktop jobs.
- `crates/library/src/` separates account synchronization, product queries, single-work
  transfers, batch planning, local imports, metadata conversion and error reporting.
- `crates/storage/src/models.rs` contains stored/query models, `error.rs` contains typed
  diagnostics, `transaction.rs` owns writes, and accounts, products and settings
  have separate query modules. Migrations remain in the storage crate. Unit-test
  suites live in `src/tests/`.

Split modules along responsibilities, not arbitrary line counts. Avoid creating
new crates or layers solely to move files; GUI and CLI should continue sharing the
same tested services.

## Logs and troubleshooting

Settings → Logs shows a single chronological feed for jobs, playback and app
operations. It refreshes while open, supports searching work IDs and nested error
codes, and can read retained runs after a restart. Routine queries and operation
starts are hidden by default. Expand an event for its cause, suggested action,
structured fields and copyable text; open the complete operation to follow its
related stages and IDs.

Diagnostic files under the profile's `logs/diagnostics/` are bounded and retained
for up to seven days, subject to the 50 MiB store limit. An exported ZIP contains
`logs.txt` for reading, `events.jsonl` for tools, run metadata and a manifest that
reports omissions or incomplete writes. Export one operation when possible and
include the steps needed to reproduce the issue.

Persist typed facts such as phase, public RJ/BJ/VJ ID, duration, HTTP status,
SQLite code/constraint kind and OS error code. Human descriptions and suggested
actions come from a safe catalogue. Passwords, cookies, serials, response bodies,
signed download URLs and arbitrary exception strings must not enter diagnostics.
Private account identifiers and paths use per-run references. Task context must
also be carried into blocking workers so their causes remain linked to the
original operation.

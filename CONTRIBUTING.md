# Contributing

Keep DLDR small, readable and easy to maintain.

- Add files only when they serve the application, tests, build, distribution or
  essential documentation. Prefer extending an existing module or guide.
- Do not commit work diaries, conversation transcripts, session notes, temporary
  plans, personal machine reports or unused prototypes. Describe changes in
  commits and pull requests; release notes describe user-visible changes.
- Keep documentation current and consolidated. Remove obsolete material and fix
  references when moving or deleting files. Screenshots and generated artifacts
  belong outside the source tree unless needed by the product or tests.
- Use DLDR for product naming, `io.github.ribeye0.dldr` for desktop identity,
  `dldr-desktop` for the GUI executable and `dldr` for the CLI. Keep source
  attribution in the license and third-party notices.
- Use clear names and direct implementations. Avoid speculative abstractions,
  duplicated behavior and unrelated refactors.
- Keep Rust entry points focused on composition and public exports. Group commands
  and services by feature, place large unit-test suites in test modules, and keep
  existing public APIs stable during internal moves. Add diagnostics at meaningful
  stages with typed causes and actionable context, without account secrets or raw
  response bodies.
- Put shared business logic in the Rust crates; keep Tauri and CLI adapters thin
  and frontend components focused on interaction and presentation.
- Keep application code and configuration in `apps/desktop/` and `apps/cli/`,
  reusable Rust services in `crates/`, canonical shared data in `resources/`,
  tooling in `tools/` and guides in `docs/`. The root is for workspace manifests,
  toolchain settings and the README, contribution and license documents.
- Keep synthetic previews and their fixtures in `apps/desktop/previews/`, outside
  shipped source. Put release/QA artifacts under `target/dist/`; ignore generated
  output. Do not add personal editor settings such as `.vscode/` or `.idea/`.
- Use `dldr-` Rust package names and `dldr_` imports. Remove unused components,
  template scaffolding and redundant probes rather than carrying them forward.
- Focus the GUI on a single DLsite account. Simplifying account flows must preserve
  existing profile data and downloaded files.
- Prioritize Linux while core functionality is being completed; resume Windows
  packaging when it is ready or explicitly requested.
- Preserve useful regression tests, licenses, attribution and data-source notes.
  Validate the affected code before submitting changes.
- For changes intended for the installed daily Linux app, rebuild and install
  with `tools/dev/install-linux.sh`, then verify the installed executable.
  Distinguish source changes from an updated installed application.
- Never commit credentials, personal databases or purchased media. Do not rewrite
  published history. Change profile locations or move user data only when the
  user explicitly requests it; routine cleanup must preserve them.

See [README](README.md) for setup and checks,
[architecture](docs/architecture.md) for module boundaries and
[releases](docs/releasing.md) for distribution requirements.

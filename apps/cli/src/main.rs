mod args;
mod auth;
mod download;
mod output;
mod progress;

use args::{Cli, Command};
use clap::{CommandFactory, Parser};
use dldr_credentials::{InMemoryCredentialStore, LocalCredentialStore};
use dldr_library::{Library, LibraryInstanceGuard};
use dldr_storage::{ProductListQuery, Storage};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::{ExitCode, Stdio},
    sync::Arc,
};

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
struct Error {
    message: String,
    code: u8,
}
impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 1,
        }
    }
    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 2,
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self {
            code: if error.kind() == std::io::ErrorKind::BrokenPipe {
                0
            } else {
                1
            },
            message: error.to_string(),
        }
    }
}
impl From<dldr_storage::StorageError> for Error {
    fn from(error: dldr_storage::StorageError) -> Self {
        Self::new(error.to_string())
    }
}
impl From<dldr_library::LibraryError> for Error {
    fn from(error: dldr_library::LibraryError) -> Self {
        Self::new(error.support_message())
    }
}

struct Context {
    library: Library,
    data_dir: PathBuf,
    _instance: Option<LibraryInstanceGuard>,
}
impl Context {
    async fn open(data_dir: PathBuf, write: bool, initialize: bool) -> Result<Self> {
        let database = data_dir.join("dldr.sqlite");
        if !database.is_file() && !initialize {
            return Err(Error::new(
                "No library found. Run `dldr login` first, or use --data-dir DIR.",
            ));
        }
        let instance = if write {
            Some(LibraryInstanceGuard::acquire(&data_dir)?)
        } else {
            None
        };
        let storage = if write {
            let storage = Storage::open(database).await?;
            storage.run_migrations().await?;
            storage.recover_download_queue().await?;
            storage
        } else {
            Storage::open_read_only(database).await?
        };
        let credentials: Arc<dyn dldr_credentials::CredentialStore> = if write {
            Arc::new(
                LocalCredentialStore::open(data_dir.join("credentials/vault.json")).map_err(
                    |_| {
                        Error::new(
                            "Could not open saved credentials. Check the GUI account settings.",
                        )
                    },
                )?,
            )
        } else {
            Arc::new(InMemoryCredentialStore::new())
        };
        Ok(Self {
            library: Library::new(storage, credentials),
            data_dir,
            _instance: instance,
        })
    }
    fn storage(&self) -> &Storage {
        self.library.storage()
    }

    async fn account_id(&self, name: &str) -> Result<String> {
        let accounts = self.library.accounts().await?;
        let matches: Vec<_> = accounts
            .iter()
            .filter(|a| a.id == name || a.label == name)
            .collect();
        match matches.as_slice() {
            [account] if account.enabled => Ok(account.id.clone()),
            [..] if matches.len() > 1 => Err(Error::usage(
                "Account label is ambiguous; use its ID from `dldr accounts`.",
            )),
            [_] => Err(Error::new(
                "The selected account is disabled. Use `dldr login --account ID` or enable it in the GUI.",
            )),
            _ => Err(Error::usage(
                "Unknown account. Run `dldr accounts` to list IDs and labels.",
            )),
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = run(&cli).await;
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            if error.code != 0 {
                if cli.json {
                    eprintln!("{}", json!({"error": error.message}));
                } else {
                    eprintln!("dldr: {}", output::clean(&error.message));
                }
            }
            ExitCode::from(error.code)
        }
    }
}

async fn run(cli: &Cli) -> Result<u8> {
    if cli.command.is_some() && cli.download != args::Download::default() {
        return Err(Error::usage("Do not combine a subcommand with direct-download arguments. Put its options after the subcommand, or use `dldr RJ01760323 [OPTIONS]`."));
    }
    if cli.command.is_none() && cli.download == args::Download::default() {
        Cli::command().print_help()?;
        output::write("\n")?;
        return Ok(0);
    }
    let direct = Command::Download(cli.download.clone());
    let command = cli.command.as_ref().unwrap_or(&direct);
    if matches!(command, Command::Download(d) if d.work_ids.is_empty() && d.search.is_none()) {
        return Err(Error::usage(
            "Specify a work ID, e.g. `dldr RJ01760323`, or use `dldr download --search QUERY`.",
        ));
    }
    // Reject redirected credentials before creating a new profile or opening its vault.
    if matches!(command, Command::Login { .. }) {
        auth::require_terminal()?;
    }
    if let Command::Gui { path } = command {
        if cli.data_dir.is_some() {
            return Err(Error::usage("--data-dir cannot be used with gui; it always opens the installed desktop profile."));
        }
        launch_gui(path.as_deref())?;
        if cli.json {
            output::json(&json!({"launched":true}))?;
        }
        return Ok(0);
    }
    let data_dir = match cli.data_dir.clone() {
        Some(path) => absolute(path)?,
        None => dldr_library::profile_dir()?,
    };
    let write = matches!(
        command,
        Command::Sync { .. } | Command::Resume(_) | Command::Login { .. } | Command::Logout { .. }
    ) || matches!(command, Command::Download(d) if !d.dry_run);
    let context = Context::open(data_dir, write, matches!(command, Command::Login { .. })).await?;
    let ui_locale = context.storage().app_settings().await?.ui_locale;
    match command {
        Command::Login {
            account,
            name,
            no_sync,
        } => {
            return auth::login(&context, cli, account.as_deref(), name.as_deref(), *no_sync).await;
        }
        Command::Logout { account } => {
            return auth::logout(&context, cli, account.as_deref()).await
        }
        Command::Search(search) => {
            let offset = (search.page - 1)
                .checked_mul(search.limit)
                .ok_or_else(|| Error::usage("Page number is too large."))?;
            let query = search.query.join(" ");
            let page = context
                .library
                .list_products(&ProductListQuery {
                    search: (!query.trim().is_empty()).then_some(query),
                    downloaded_only: search.downloaded,
                    limit: search.limit,
                    offset,
                    ..Default::default()
                })
                .await?;
            let works: Vec<Value> = page
                .products
                .iter()
                .map(|work| output::work(work, ui_locale))
                .collect();
            if cli.json {
                output::json(
                    &json!({"total":page.total_count,"page":search.page,"limit":search.limit,"works":works}),
                )?;
            } else {
                output::works(&works)?;
                if !cli.quiet {
                    eprintln!("{} results · page {}", page.total_count, search.page);
                }
            }
        }
        Command::Show { work_id } => {
            let detail = context.library.product_detail(work_id).await?;
            let value = output::detail(&detail, ui_locale);
            if cli.json {
                output::json(&value)?;
            } else {
                output::show(&value)?;
            }
        }
        Command::Download(options) => return download::download(&context, cli, options).await,
        Command::Resume(options) => return download::resume(&context, cli, options).await,
        Command::Status => {
            let entries = context.storage().download_queue().await?;
            let items: Vec<_> = entries
                .iter()
                .map(|e| json!({"work_id":e.work_id,"title":e.title,"status":e.status}))
                .collect();
            if cli.json {
                output::json(&json!({"queue":items}))?;
            } else if entries.is_empty() {
                output::write("No queued downloads.\n")?;
            } else {
                output::write("ID           STATUS       TITLE\n")?;
                for e in entries {
                    output::write(&format!(
                        "{:<12} {:<12} {}\n",
                        e.work_id,
                        e.status,
                        output::clean(&e.title)
                    ))?;
                }
            }
        }
        Command::Accounts => {
            let accounts = context.library.accounts().await?;
            let accounts: Vec<_> = accounts.iter().map(|a| json!({"id":a.id,"label":a.label,"enabled":a.enabled,"last_sync":a.last_sync_at})).collect();
            if cli.json {
                output::json(&json!({"accounts":accounts}))?;
            } else {
                for a in accounts {
                    output::write(&format!(
                        "{}  {}  {}\n",
                        output::string(&a, "id"),
                        if a["enabled"] == true {
                            "enabled "
                        } else {
                            "disabled"
                        },
                        output::clean(output::string(&a, "label"))
                    ))?;
                }
            }
        }
        Command::Config => {
            let settings = context.storage().app_settings().await?;
            let value = json!({"auto_extract":settings.auto_extract,"ui_locale":settings.ui_locale,"data_dir":context.data_dir,"library":settings.library_root,"temp":settings.download_root,
                "jobs":context.storage().download_concurrency().await?,"limit_rate":context.storage().download_rate_limit().await?});
            if cli.json {
                output::json(&value)?;
            } else {
                for key in [
                    "data_dir",
                    "library",
                    "temp",
                    "jobs",
                    "limit_rate",
                    "ui_locale",
                    "auto_extract",
                ] {
                    output::write(&format!(
                        "{key:<12} {}\n",
                        match &value[key] {
                            Value::String(s) => output::clean(s),
                            Value::Null => "not set / unlimited".into(),
                            v => v.to_string(),
                        }
                    ))?;
                }
            }
        }
        Command::Open { work_id, print } => {
            let path = if let Some(id) = work_id {
                let detail = context.library.product_detail(id).await?;
                if detail.download.status != dldr_storage::WorkDownloadStatus::Downloaded {
                    return Err(Error::new(
                        "This work is not downloaded. Use `dldr ID` first.",
                    ));
                }
                detail.download.local_path
            } else {
                context.storage().app_settings().await?.library_root
            };
            let path = path.ok_or_else(|| {
                Error::new("No local folder configured. Set the library folder in the GUI.")
            })?;
            if !Path::new(&path).is_dir() {
                return Err(Error::new(
                    "The configured local folder does not exist. Verify the library in Settings.",
                ));
            }
            if !print {
                open_folder(Path::new(&path))?;
            }
            if cli.json {
                output::json(&json!({"path":path}))?;
            } else if *print {
                output::write(&format!("{path}\n"))?;
            }
        }
        Command::Sync { account } => {
            return download::sync(&context, cli, account.as_deref()).await
        }
        Command::Gui { .. } => unreachable!(),
    }
    Ok(0)
}

fn absolute(path: PathBuf) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
}

fn launch_gui(explicit: Option<&Path>) -> Result<()> {
    let executable = std::env::current_exe()?;
    let beside = if cfg!(windows) && executable.with_file_name("DLDR.Desktop.exe").is_file() {
        executable.with_file_name("DLDR.Desktop.exe")
    } else {
        executable.with_file_name(if cfg!(windows) {
            "dldr-desktop.exe"
        } else {
            "dldr-desktop"
        })
    };
    let configured = std::env::var_os("DLDR_GUI").map(PathBuf::from);
    let installed = if cfg!(windows) {
        dirs::data_local_dir().map(|p| p.join("DLDR/DLDR.Desktop.exe"))
    } else {
        dirs::data_dir().map(|p| p.join("dldr/DLDR.AppImage"))
    };
    let legacy = if cfg!(windows) {
        None
    } else {
        dirs::home_dir().map(|h| h.join(".local/bin/dldr-gui"))
    };
    let binary = gui_binary(
        explicit.or(configured.as_deref()),
        installed.as_deref(),
        &beside,
        legacy.as_deref(),
    )?;
    std::process::Command::new(binary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

fn gui_binary(
    explicit: Option<&Path>,
    installed: Option<&Path>,
    beside: &Path,
    legacy: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return absolute(path.into());
        }
        return Err(Error::usage("GUI executable not found. Use `dldr gui PATH` or set DLDR_GUI to an existing executable."));
    }
    installed.into_iter().chain(std::iter::once(beside)).chain(legacy)
        .find(|p| p.is_file()).map(Path::to_path_buf)
        .ok_or_else(|| Error::new("DLDR desktop application not found. Use `dldr gui PATH`, set DLDR_GUI, or install the desktop application."))
}

fn open_folder(path: &Path) -> Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer.exe"
    } else {
        "xdg-open"
    };
    let status = std::process::Command::new(opener)
        .arg(path)
        .stdout(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(Error::new(
            "Could not open the file manager; use --print to get the folder path.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod gui_launcher_tests {
    use super::*;

    #[test]
    fn explicit_then_managed_then_adjacent_then_legacy_and_missing_override_fails() {
        let temp = tempfile::tempdir().unwrap();
        let explicit = temp.path().join("中文 GUI.AppImage");
        let installed = temp.path().join("installed");
        let beside = temp.path().join("beside");
        let legacy = temp.path().join("legacy");
        for path in [&explicit, &installed, &beside, &legacy] {
            std::fs::write(path, "fixture").unwrap();
        }
        assert_eq!(
            gui_binary(Some(&explicit), Some(&installed), &beside, Some(&legacy)).unwrap(),
            explicit
        );
        assert_eq!(
            gui_binary(None, Some(&installed), &beside, Some(&legacy)).unwrap(),
            installed
        );
        std::fs::remove_file(installed).unwrap();
        assert_eq!(
            gui_binary(None, None, &beside, Some(&legacy)).unwrap(),
            beside
        );
        std::fs::remove_file(&beside).unwrap();
        assert_eq!(
            gui_binary(None, None, &beside, Some(&legacy)).unwrap(),
            legacy
        );
        assert!(gui_binary(
            Some(&temp.path().join("missing")),
            None,
            &beside,
            Some(&legacy)
        )
        .is_err());
        assert!(gui_binary(None, None, &beside, None).is_err());
    }
}

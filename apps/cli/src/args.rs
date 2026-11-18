use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "dldr",
    version,
    about = "DLDR — DLsite Downloader: search and download your DLsite library",
    subcommand_precedence_over_arg = true,
    subcommand_negates_reqs = true,
    override_usage = "dldr [OPTIONS] <WORK_ID>...\n       dldr <COMMAND> [OPTIONS]",
    after_help = "Examples:\n  dldr login\n  dldr RJ01760323\n  dldr RJ01760323 RJ01729125 -j 2 --limit-rate 5M\n  dldr RJ01760323 --dry-run\n  dldr search '$va:小花衣こっこ$'\n  dldr download --search '青梅竹馬' --dry-run\n  dldr resume\n\nShares the desktop library and accounts. Close the GUI before login, sync or download."
)]
pub struct Cli {
    /// Use another application profile (login can create it)
    #[arg(long, global = true, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,
    /// Print machine-readable JSON to stdout
    #[arg(long, global = true)]
    pub json: bool,
    /// Hide progress and informational messages on stderr
    #[arg(short, long, global = true)]
    pub quiet: bool,
    #[command(flatten)]
    pub download: Download,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Sign in interactively, save credentials and sync purchases
    Login {
        /// Update an existing account by ID or exact label
        #[arg(short, long, conflicts_with = "name")]
        account: Option<String>,
        /// Label for a new account (default: DLsite)
        #[arg(long, value_parser = nonempty)]
        name: Option<String>,
        /// Sign in without refreshing the purchase list
        #[arg(long)]
        no_sync: bool,
    },
    /// Forget saved credentials and disable an account; keep cached works/files
    Logout {
        /// Required when multiple accounts exist
        #[arg(short, long)]
        account: Option<String>,
    },
    /// Search cached works (no network); no query lists the library
    #[command(visible_aliases = ["s", "list", "ls"])]
    Search(Search),
    /// Show a work's cached details
    #[command(visible_alias = "info")]
    Show {
        #[arg(value_parser = work_id)]
        work_id: String,
    },
    /// Download selected works; automatically reuse partial downloads
    #[command(visible_alias = "dl", group(clap::ArgGroup::new("selection").required(true).args(["work_ids", "search"])))]
    Download(Download),
    /// Resume paused/failed works; optionally specify work IDs
    Resume(Resume),
    /// Show the persistent download queue (not a live speed monitor)
    #[command(visible_alias = "queue")]
    Status,
    /// Open the library folder, or an individual downloaded work
    Open {
        #[arg(value_parser = work_id)]
        work_id: Option<String>,
        /// Print the path without opening a file manager
        #[arg(long)]
        print: bool,
    },
    /// List configured accounts without exposing login names or credentials
    Accounts,
    /// Refresh purchases using saved credentials (requires network)
    Sync {
        /// Account ID or exact label; defaults to all enabled accounts
        #[arg(short, long)]
        account: Option<String>,
    },
    /// Show saved paths and download defaults
    Config,
    /// Open the desktop application
    Gui {
        /// Launch a specific GUI executable or AppImage
        path: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
pub struct Search {
    /// Keywords or quoted $circle:...$, $va:...$, $tag:...$ conditions
    pub query: Vec<String>,
    /// Show only completed local works
    #[arg(short = 'd', long)]
    pub downloaded: bool,
    /// Results per page
    #[arg(short = 'n', long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=500))]
    pub limit: u32,
    /// One-based page number
    #[arg(short = 'p', long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    pub page: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Args)]
pub struct TransferOptions {
    /// Simultaneous works (1-4); defaults to the saved GUI setting
    #[arg(short = 'j', long, value_parser = clap::value_parser!(u8).range(1..=4))]
    pub jobs: Option<u8>,
    /// Total transfer limit, e.g. 5M or 1.5MiB/s; 0 means unlimited
    #[arg(long, value_name = "RATE", value_parser = rate)]
    pub limit_rate: Option<u64>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Args)]
pub struct Download {
    /// One or more purchased RJ/BJ/VJ work IDs
    #[arg(value_parser = work_id, num_args = 1.., value_name = "WORK_ID", conflicts_with = "search")]
    pub work_ids: Vec<String>,
    /// Download matches from this cached search (confirmation required)
    #[arg(short = 's', long, value_name = "QUERY", value_parser = nonempty)]
    pub search: Option<String>,
    /// Account ID or exact label
    #[arg(short, long)]
    pub account: Option<String>,
    /// Preview selected works and cached capacity without network or writes
    #[arg(long)]
    pub dry_run: bool,
    /// Accept a search-based selection without prompting
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Library folder for this command only
    #[arg(short = 'o', long, value_name = "DIR")]
    pub output: Option<PathBuf>,
    /// Partial-download folder for this command only
    #[arg(long, value_name = "DIR")]
    pub temp: Option<PathBuf>,
    /// Keep archives instead of extracting them
    #[arg(long, conflicts_with = "extract")]
    pub keep_archives: bool,
    /// Extract recognized archives and delete them after success (overrides settings)
    #[arg(long, conflicts_with = "keep_archives")]
    pub extract: bool,
    #[command(flatten)]
    pub transfer: TransferOptions,
}

#[derive(Debug, Args)]
pub struct Resume {
    #[arg(value_parser = work_id)]
    pub work_ids: Vec<String>,
    #[command(flatten)]
    pub transfer: TransferOptions,
}

fn nonempty(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("the search query cannot be empty".into())
    } else {
        Ok(value.trim().into())
    }
}

pub fn work_id(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_uppercase();
    let prefix = value.get(..2).unwrap_or("");
    let digits = value.get(2..).unwrap_or("");
    if matches!(prefix, "RJ" | "BJ" | "VJ")
        && !digits.is_empty()
        && digits.len() <= 10
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        Ok(value)
    } else {
        Err("expected a work ID such as RJ01647600".into())
    }
}

pub fn rate(value: &str) -> Result<u64, String> {
    let value = value.trim().to_ascii_uppercase();
    let value = value.strip_suffix("/S").unwrap_or(&value);
    let split = value
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(value.len());
    let (number, suffix) = value.split_at(split);
    let multiplier = match suffix {
        "" | "B" => 1f64,
        "K" | "KB" | "KIB" => 1024f64,
        "M" | "MB" | "MIB" => 1024f64.powi(2),
        "G" | "GB" | "GIB" => 1024f64.powi(3),
        _ => return Err("use bytes/s or a K/M/G suffix, e.g. 5M; 0 means unlimited".into()),
    };
    let bytes = number.parse::<f64>().map_err(|_| "invalid transfer rate")? * multiplier;
    if !bytes.is_finite()
        || bytes < 0.0
        || (bytes > 0.0 && bytes < 1024.0)
        || bytes > 1024f64.powi(3)
    {
        return Err("rate must be 0 (unlimited), or between 1K and 1G".into());
    }
    Ok(bytes as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    #[test]
    fn direct_ids_flags_and_subcommands_have_unambiguous_boundaries() {
        let cli = Cli::try_parse_from([
            "dldr",
            "--json",
            "rj01760323",
            "RJ2",
            "-j",
            "2",
            "-o",
            "my works",
        ])
        .unwrap();
        assert!(cli.command.is_none());
        assert_eq!(cli.download.work_ids, ["RJ01760323", "RJ2"]);
        assert_eq!(cli.download.output, Some(PathBuf::from("my works")));

        assert!(Cli::try_parse_from(["dldr", "RJ1", "--search", "NTR"]).is_err());
        assert!(matches!(
            Cli::try_parse_from(["dldr", "--data-dir", "my profile", "login", "--no-sync"])
                .unwrap()
                .command,
            Some(Command::Login { no_sync: true, .. })
        ));
        assert!(Cli::try_parse_from(["dldr", "login", "--password", "secret"]).is_err());
        assert!(Cli::try_parse_from(["dldr", "login", "-a", "x", "--name", "y"]).is_err());
        Cli::command().debug_assert();
    }
    #[test]
    fn accepts_conventional_short_flags_and_precise_queries() {
        let args = Cli::try_parse_from([
            "dldr",
            "dl",
            "rj01647600",
            "-j",
            "2",
            "--limit-rate",
            "1.5M",
            "--json",
        ])
        .unwrap();
        let Command::Download(d) = args.command.unwrap() else {
            panic!()
        };
        assert_eq!(d.work_ids, ["RJ01647600"]);
        assert_eq!(d.transfer.limit_rate, Some(1_572_864));
        assert!(args.json);
        assert!(Cli::try_parse_from(["dldr", "dl"]).is_err());
        assert!(Cli::try_parse_from(["dldr", "dl", "RJ1", "--search", "NTR"]).is_err());
        assert!(Cli::try_parse_from(["dldr", "dl", "--search", " "]).is_err());
        assert!(Cli::try_parse_from(["dldr", "dl", "RJ1", "-j", "5"]).is_err());
    }
    #[test]
    fn archive_overrides_are_explicit_and_mutually_exclusive() {
        let default = Cli::try_parse_from(["dldr", "RJ1"]).unwrap();
        assert!(!default.download.extract && !default.download.keep_archives);
        assert!(
            Cli::try_parse_from(["dldr", "RJ1", "--extract"])
                .unwrap()
                .download
                .extract
        );
        assert!(
            Cli::try_parse_from(["dldr", "RJ1", "--keep-archives"])
                .unwrap()
                .download
                .keep_archives
        );
        for args in [
            vec!["dldr", "RJ1", "--keep-archives", "--extract"],
            vec!["dldr", "download", "RJ1", "--keep-archives", "--extract"],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
    }
    #[test]
    fn rejects_bad_ids_and_rates_before_opening_data() {
        for id in ["../RJ123", "RJ1/foo", "RJ", "こんにちは"] {
            assert!(work_id(id).is_err());
        }
        for r in ["NaN", "inf", "-1M", "0.001K", "2G", "123Q"] {
            assert!(rate(r).is_err(), "{r}");
        }
        assert_eq!(rate("0").unwrap(), 0);
        assert_eq!(rate("1KiB/s").unwrap(), 1024);
    }
}

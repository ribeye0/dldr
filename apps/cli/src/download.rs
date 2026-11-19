use crate::{
    absolute,
    args::{Cli, Download, Resume, TransferOptions},
    output,
    progress::{self, Progress, Prompt, WorkSink},
    Context, Error, Result,
};
use dldr_api::DlsiteClient;
use dldr_download::{DownloadRateLimiter, UnpackPolicy};
use dldr_library::{
    download_progress::DownloadWorkPlan, AccountSyncRequest, CancellationToken, DlsiteSource,
    WorkDownloadRequest,
};
use dldr_storage::{DownloadQueueEntry, ProductDetail, ProductListQuery};
use futures_util::{stream, StreamExt};
use serde_json::json;
use std::{
    collections::BTreeSet,
    io::{IsTerminal, Write},
    path::PathBuf,
};

struct Plan {
    works: Vec<ProductDetail>,
    skipped: usize,
}

async fn plan(context: &Context, options: &Download) -> Result<Plan> {
    let account = match &options.account {
        Some(name) => Some(context.account_id(name).await?),
        None => None,
    };
    let query = ProductListQuery {
        search: options.search.clone(),
        account_id: account.clone(),
        ..Default::default()
    };
    // Explicit IDs are strict: a typo or unavailable ownership must never silently disappear.
    for id in &options.work_ids {
        let detail = context.library.product_detail(id).await.map_err(|_| {
            Error::new(format!(
                "{id}: not in the cached library; sync purchases first."
            ))
        })?;
        if !detail.owners.iter().any(|a| {
            a.account_id != dldr_storage::LOCAL_PRODUCT_OWNER_ID
                && account.as_ref().is_none_or(|id| *id == a.account_id)
        }) {
            return Err(Error::new(format!(
                "{id}: not owned by an enabled matching account."
            )));
        }
    }
    let candidates = context
        .library
        .select_bulk_candidates(
            &query,
            true,
            (!options.work_ids.is_empty()).then_some(options.work_ids.as_slice()),
        )
        .await?;
    let mut works = Vec::new();
    for id in candidates.work_ids {
        works.push(context.library.product_detail(&id).await?);
    }
    if !options.work_ids.is_empty() {
        works.sort_by_key(|w| {
            options
                .work_ids
                .iter()
                .position(|id| *id == w.work_id)
                .unwrap_or(usize::MAX)
        });
    }
    Ok(Plan {
        works,
        skipped: candidates.skipped_downloaded_count,
    })
}

pub async fn download(context: &Context, cli: &Cli, options: &Download) -> Result<u8> {
    let plan = plan(context, options).await?;
    let known = plan
        .works
        .iter()
        .filter_map(|p| p.content_size_bytes)
        .fold(0u64, u64::saturating_add);
    let unknown = plan
        .works
        .iter()
        .filter(|p| p.content_size_bytes.is_none())
        .count();
    if options.dry_run {
        let locale = context.storage().app_settings().await?.ui_locale;
        let works: Vec<_> = plan
            .works
            .iter()
            .map(|work| output::detail(work, locale))
            .collect();
        if cli.json {
            output::json(
                &json!({"selected":works.len(),"skipped_downloaded":plan.skipped,"known_bytes":known,"unknown_sizes":unknown,"works":works}),
            )?;
        } else {
            output::works(&works)?;
            output::write(&format!(
                "{} selected · {} known · {} unknown sizes · {} already downloaded\n",
                works.len(),
                output::bytes(known),
                unknown,
                plan.skipped
            ))?;
        }
        return Ok(0);
    }
    if plan.works.is_empty() {
        if cli.json {
            output::json(
                &json!({"succeeded":[],"failed":[],"paused":0,"skipped_downloaded":plan.skipped}),
            )?;
        } else {
            output::write(&format!(
                "Nothing to download ({} already downloaded).\n",
                plan.skipped
            ))?;
        }
        return Ok(0);
    }
    if options.search.is_some() && !options.yes {
        if !std::io::stdin().is_terminal() || cli.json {
            return Err(Error::usage("Search-based downloads need confirmation. Inspect --dry-run first, then add --yes."));
        }
        eprint!(
            "Download {} matching works ({} known, {} unknown sizes)? [y/N] ",
            plan.works.len(),
            output::bytes(known),
            unknown
        );
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return Ok(0);
        }
    }
    let settings = context.storage().app_settings().await?;
    let library = root(options.output.clone(), settings.library_root, "library")?;
    let temp = root(
        options.temp.clone(),
        settings.download_root,
        "partial-download",
    )?;
    let account = match &options.account {
        Some(name) => Some(context.account_id(name).await?),
        None => None,
    };
    let entries = plan
        .works
        .iter()
        .map(|p| DownloadQueueEntry {
            work_id: p.work_id.clone(),
            title: p.title.clone(),
            thumbnail_url: p.thumbnail_url.clone(),
            batch_id: String::new(),
            position: 0,
            account_id: account.clone(),
            library_root: library.clone(),
            download_root: temp.clone(),
            unpack_policy: if options.keep_archives || (!options.extract && !settings.auto_extract)
            {
                "keep_archives"
            } else {
                "unpack_when_recognized"
            }
            .into(),
            replace_existing: false,
            status: "queued".into(),
            error_message: None,
        })
        .collect();
    execute(context, cli, &options.transfer, entries, plan.skipped).await
}

fn root(override_path: Option<PathBuf>, saved: Option<String>, label: &str) -> Result<String> {
    let path = override_path
        .or_else(|| saved.map(PathBuf::from))
        .ok_or_else(|| {
            Error::new(format!(
                "No {label} folder configured. Set folders in the GUI or pass --output / --temp."
            ))
        })?;
    if path.as_os_str().is_empty() {
        return Err(Error::usage(format!("The {label} folder cannot be empty.")));
    }
    absolute(path)?
        .into_os_string()
        .into_string()
        .map_err(|_| Error::new("Folder paths must be valid UTF-8."))
}

pub async fn resume(context: &Context, cli: &Cli, options: &Resume) -> Result<u8> {
    let queue = context.storage().download_queue().await?;
    for id in &options.work_ids {
        if !queue.iter().any(|e| {
            &e.work_id == id && matches!(e.status.as_str(), "paused" | "failed" | "cancelled")
        }) {
            return Err(Error::usage(format!(
                "{id}: not a paused, failed or cancelled queue entry. Check `dldr status`."
            )));
        }
    }
    let selected: BTreeSet<_> = options.work_ids.iter().collect();
    let entries = queue
        .into_iter()
        .filter(|e| {
            if selected.is_empty() {
                matches!(e.status.as_str(), "paused" | "failed")
            } else {
                selected.contains(&e.work_id)
            }
        })
        .collect();
    execute(context, cli, &options.transfer, entries, 0).await
}

async fn execute(
    context: &Context,
    cli: &Cli,
    options: &TransferOptions,
    entries: Vec<DownloadQueueEntry>,
    skipped: usize,
) -> Result<u8> {
    execute_with_factory(
        context,
        cli,
        options,
        entries,
        skipped,
        CancellationToken::new(),
        |limiter| {
            Ok(
                DlsiteSource::new(DlsiteClient::new(Default::default())?)
                    .with_rate_limiter(limiter),
            )
        },
    )
    .await
}

async fn execute_with_factory<S, F>(
    context: &Context,
    cli: &Cli,
    options: &TransferOptions,
    mut entries: Vec<DownloadQueueEntry>,
    skipped: usize,
    cancellation: CancellationToken,
    factory: F,
) -> Result<u8>
where
    S: dldr_library::WorkDownloadSource + Sync,
    F: Fn(DownloadRateLimiter) -> dldr_library::Result<S> + Sync,
{
    let jobs = match options.jobs {
        Some(j) => j as usize,
        None => context.storage().download_concurrency().await?,
    };
    context.library.transfers().set_limit(jobs)?;
    let rate = match options.limit_rate {
        Some(0) => None,
        Some(n) => Some(n),
        None => context.storage().download_rate_limit().await?,
    };
    let limiter = DownloadRateLimiter::default();
    limiter.set_limit(rate).map_err(Error::new)?;
    let batch = uuid::Uuid::new_v4().to_string();
    let mut plans = Vec::new();
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.batch_id = batch.clone();
        entry.position = index as i64;
        entry.status = "queued".into();
        entry.error_message = None;
        plans.push(DownloadWorkPlan {
            work_id: entry.work_id.clone(),
            title: entry.title.clone(),
            bytes_total: context
                .library
                .product_detail(&entry.work_id)
                .await
                .ok()
                .and_then(|p| p.content_size_bytes),
        });
    }
    let progress = Progress::new(plans, cli.quiet);
    let prompt = Prompt::new(cancellation.clone(), progress.clone());
    let signal = progress::signals(cancellation.clone());
    context.storage().enqueue_downloads(&entries).await?;
    let total = entries.len();
    let mut workers = stream::iter(entries)
        .map(|entry| {
            let limiter = limiter.clone();
            let progress = progress.clone();
            let cancellation = cancellation.clone();
            let prompt = &prompt;
            let factory = &factory;
            async move {
                if cancellation.is_cancelled() {
                    return Ok((entry.work_id, Err(dldr_library::LibraryError::Cancelled)));
                }
                progress.start(&entry.work_id);
                let sink = WorkSink {
                    progress: progress.clone(),
                    work_id: entry.work_id.clone(),
                };
                let result = async {
                    let source = factory(limiter)?;
                    let library = PathBuf::from(&entry.library_root);
                    let temp = PathBuf::from(&entry.download_root);
                    let mut request = WorkDownloadRequest::new(&entry.work_id, &library, &temp);
                    request.account_id = entry.account_id.as_deref();
                    request.queue_batch = Some(&entry.batch_id);
                    request.replace_existing = entry.replace_existing;
                    request.resume_partial = true;
                    request.unpack_policy = if entry.unpack_policy == "keep_archives" {
                        UnpackPolicy::KeepArchives
                    } else {
                        UnpackPolicy::UnpackWhenRecognized
                    };
                    request.cancellation_token = Some(&cancellation);
                    request.progress_sink = Some(&sink);
                    request.two_factor_prompt = Some(prompt);
                    context
                        .library
                        .download_work_with_source(request, &source)
                        .await
                }
                .await;
                // Client construction may fail before the core enters its queued-work wrapper.
                if let Err(error) = &result {
                    if context
                        .storage()
                        .queued_download(&entry.work_id)
                        .await?
                        .is_some_and(|q| q.batch_id == entry.batch_id && q.status == "queued")
                    {
                        context
                            .storage()
                            .set_queue_status(
                                &entry.work_id,
                                &entry.batch_id,
                                if cancellation.is_cancelled() {
                                    "paused"
                                } else {
                                    "failed"
                                },
                                Some(&error.support_message()),
                            )
                            .await?;
                    }
                }
                if result.as_ref().is_err_and(|e| {
                    e.is_cancelled() || matches!(e, dldr_library::LibraryError::QueueStopped)
                }) {
                    progress.defer(&entry.work_id);
                } else {
                    progress.finish(&entry.work_id, result.is_ok());
                }
                Ok::<_, dldr_library::LibraryError>((entry.work_id, result))
            }
        })
        .buffer_unordered(jobs);
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    let mut fatal = None;
    while let Some(result) = workers.next().await {
        let (id, result) = match result {
            Ok(result) => result,
            Err(error) => {
                fatal.get_or_insert(error);
                cancellation.cancel();
                continue;
            }
        };
        match result {
            Ok(report) => succeeded
                .push(json!({"work_id":id,"path":report.local_path,"warnings":report.warnings})),
            Err(error)
                if error.is_cancelled()
                    || matches!(error, dldr_library::LibraryError::QueueStopped) => {}
            Err(error) => failed.push(json!({"work_id":id,"error":error.support_message()})),
        }
    }
    let cleanup = context.storage().pause_queued_batch(&batch).await;
    drop(signal);
    progress.clear();
    cleanup?;
    if let Some(error) = fatal {
        return Err(error.into());
    }
    let paused = total.saturating_sub(succeeded.len() + failed.len());
    let code = if cancellation.is_cancelled() || paused > 0 {
        130
    } else if !failed.is_empty() {
        1
    } else {
        0
    };
    if cli.json {
        output::json(
            &json!({"succeeded":succeeded,"failed":failed,"paused":paused,"skipped_downloaded":skipped}),
        )?;
    } else {
        output::write(&format!(
            "{} completed · {} failed · {} paused · {} already downloaded\n",
            succeeded.len(),
            failed.len(),
            paused,
            skipped
        ))?;
        for failure in failed {
            output::write(&format!(
                "{}: {}\n",
                output::string(&failure, "work_id"),
                output::clean(output::string(&failure, "error"))
            ))?;
        }
        if paused > 0 {
            output::write("Partial files kept. Run `dldr resume` to continue.\n")?;
        }
    }
    Ok(code)
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;

pub async fn sync(context: &Context, cli: &Cli, account: Option<&str>) -> Result<u8> {
    let only = match account {
        Some(name) => Some(context.account_id(name).await?),
        None => None,
    };
    let accounts = context
        .library
        .accounts()
        .await?
        .into_iter()
        .filter(|a| a.enabled && only.as_ref().is_none_or(|id| *id == a.id))
        .collect::<Vec<_>>();
    if accounts.is_empty() {
        return Err(Error::new("No enabled accounts. Run `dldr login` first."));
    }
    let progress = Progress::new(Vec::new(), cli.quiet);
    let cancellation = CancellationToken::new();
    let prompt = Prompt::new(cancellation.clone(), progress);
    let signal = progress::signals(cancellation.clone());
    let mut reports = Vec::new();
    let mut failed = false;
    let mut interrupted = false;
    for account in accounts {
        if cancellation.is_cancelled() {
            break;
        }
        if !cli.quiet {
            eprintln!("Syncing {}…", output::clean(&account.label));
        }
        let source = DlsiteSource::new(
            DlsiteClient::new(Default::default())
                .map_err(|_| Error::new("Could not initialize the DLsite client."))?,
        );
        let mut request = AccountSyncRequest::new(&account.id);
        request.cancellation_token = Some(&cancellation);
        request.two_factor_prompt = Some(&prompt);
        match context
            .library
            .sync_account_with_source(request, &source)
            .await
        {
            Ok(report) => {
                reports.push(json!({"account_id":account.id,"purchased":report.purchased_count,"cached":report.cached_work_count}));
            }
            Err(error) if error.is_cancelled() => {
                interrupted = true;
                break;
            }
            Err(error) => {
                failed = true;
                reports.push(json!({"account_id":account.id,"error":error.support_message()}));
            }
        }
    }
    drop(signal);
    let interrupted = interrupted || cancellation.is_cancelled();
    if cli.json {
        output::json(&json!({"accounts":reports,"interrupted":interrupted}))?;
    } else {
        for report in reports {
            output::write(&format!(
                "{}: {}\n",
                output::string(&report, "account_id"),
                if report["error"].is_string() {
                    output::clean(output::string(&report, "error"))
                } else {
                    format!(
                        "{} purchased, {} cached",
                        report["purchased"], report["cached"]
                    )
                }
            ))?;
        }
    }
    Ok(if interrupted {
        130
    } else if failed {
        1
    } else {
        0
    })
}

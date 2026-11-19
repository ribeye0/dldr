use crate::{
    download, output,
    progress::{self, Progress, Prompt},
    Cli, Context, Error, Result,
};
use dldr_api::DlsiteClient;
use dldr_library::{CancellationToken, DlsiteSource, SaveAccountRequest, TwoFactorPromptResponse};
use dldr_storage::{Account, AppSettingsPatch};
use serde_json::json;
use std::io::IsTerminal;

pub fn require_terminal() -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(Error::usage("Login requires an interactive terminal. Run `dldr login` directly; passwords and verification codes cannot be passed as flags or piped input."));
    }
    Ok(())
}

async fn input(label: &str, cancellation: &CancellationToken) -> Result<String> {
    let token = cancellation.clone();
    let label = label.to_owned();
    let response = tokio::task::spawn_blocking(move || {
        progress::read_hidden(&label, &token, &Progress::new(Vec::new(), true))
    })
    .await
    .map_err(|_| Error::new("Login prompt failed."))??;
    match response {
        TwoFactorPromptResponse::Code(value) => Ok(value),
        // Enter with no characters can select an existing login name.
        TwoFactorPromptResponse::Cancelled if !cancellation.is_cancelled() => Ok(String::new()),
        _ => Err(Error {
            message: "Login cancelled.".into(),
            code: 130,
        }),
    }
}

async fn selected_account(context: &Context, selector: Option<&str>) -> Result<Account> {
    let accounts = context.library.accounts().await?;
    let mut matches = accounts
        .into_iter()
        .filter(|a| selector.is_none_or(|s| a.id == s || a.label == s));
    let Some(account) = matches.next() else {
        return Err(Error::usage(
            "No matching account. Use `dldr accounts` or `dldr login`.",
        ));
    };
    if matches.next().is_some() {
        return Err(Error::usage(
            "Multiple accounts match; use --account ID from `dldr accounts`.",
        ));
    }
    Ok(account)
}

pub async fn login(
    context: &Context,
    cli: &Cli,
    selector: Option<&str>,
    name: Option<&str>,
    no_sync: bool,
) -> Result<u8> {
    let cancellation = CancellationToken::new();
    let signal = progress::signals(cancellation.clone());
    let existing = if selector.is_some() {
        Some(selected_account(context, selector).await?)
    } else {
        None
    };
    let prompt = if existing
        .as_ref()
        .and_then(|a| a.login_name.as_ref())
        .is_some()
    {
        "DLsite login ID/email (input hidden; Enter keeps saved ID; Ctrl-C cancels):"
    } else {
        "DLsite login ID/email (input hidden; Ctrl-C cancels):"
    };
    let entered = input(prompt, &cancellation).await?;
    let login_name = if entered.trim().is_empty() {
        existing
            .as_ref()
            .and_then(|a| a.login_name.clone())
            .ok_or_else(|| Error::usage("Login ID cannot be empty."))?
    } else {
        entered.trim().to_owned()
    };
    // Reuse this identity when logging in again without requiring the user to copy an ID.
    let existing = match existing {
        Some(a) => Some(a),
        None => {
            let mut matches = context
                .library
                .accounts()
                .await?
                .into_iter()
                .filter(|a| a.login_name.as_deref() == Some(&login_name));
            let first = matches.next();
            if matches.next().is_some() {
                return Err(Error::usage(
                    "Multiple saved accounts use this login ID; use `dldr login --account ID`.",
                ));
            }
            first
        }
    };
    let password = input("Password (input hidden; Ctrl-C cancels):", &cancellation).await?;
    if password.is_empty() {
        return Err(Error::usage("Password cannot be empty."));
    }
    let request = SaveAccountRequest {
        id: existing.as_ref().map(|a| a.id.clone()),
        label: name
            .map(str::to_owned)
            .or_else(|| existing.as_ref().map(|a| a.label.clone()))
            .unwrap_or_else(|| "DLsite".into()),
        login_name: Some(login_name),
        password: Some(password),
        remember_password: true,
        enabled: true,
    };
    let source = DlsiteSource::new(
        DlsiteClient::new(Default::default())
            .map_err(|_| Error::new("Could not initialize the DLsite client."))?,
    );
    let prompt = Prompt::new(cancellation.clone(), Progress::new(Vec::new(), cli.quiet));
    let account = match context
        .library
        .login_account_with_source(request, Some(&prompt), Some(&cancellation), &source)
        .await
    {
        Err(e) if e.is_cancelled() => return Ok(130),
        result => result?,
    };
    // Establish defaults only for unset paths; preserve every existing GUI setting.
    let settings = context.storage().app_settings().await?;
    let library = settings
        .library_root
        .or_else(|| dirs::download_dir().map(|p| p.join("DLDR").to_string_lossy().into_owned()))
        .unwrap_or_else(|| {
            context
                .data_dir
                .join("library")
                .to_string_lossy()
                .into_owned()
        });
    let temp = settings.download_root.unwrap_or_else(|| {
        context
            .data_dir
            .join("downloads")
            .to_string_lossy()
            .into_owned()
    });
    context
        .storage()
        .save_app_settings_patch(&AppSettingsPatch {
            library_root: Some(library),
            download_root: Some(temp),
            ..Default::default()
        })
        .await?;
    if !cli.quiet {
        eprintln!(
            "Signed in as {}. Credentials saved locally.{}",
            output::clean(&account.label),
            if no_sync {
                " Run `dldr sync` before downloading uncached purchases."
            } else {
                " Refreshing purchases…"
            }
        );
    }
    if cancellation.is_cancelled() {
        return Ok(130);
    }
    drop(signal);
    if !no_sync {
        // The same saved session is reused; JSON remains one complete sync result.
        return download::sync(context, cli, Some(&account.id)).await;
    }
    if cli.json {
        output::json(
            &json!({"account_id":account.id,"label":account.label,"logged_in":true,"synced":false}),
        )?;
    }
    Ok(0)
}

pub async fn logout(context: &Context, cli: &Cli, selector: Option<&str>) -> Result<u8> {
    let account = selected_account(context, selector).await?;
    context.library.logout_account(&account.id).await?;
    if cli.json {
        output::json(&json!({"account_id":account.id,"logged_out":true}))?;
    } else {
        output::write(&format!(
            "Signed out of {}. Cached works and downloaded files kept.\n",
            output::clean(&account.label)
        ))?;
    }
    Ok(0)
}

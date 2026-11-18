use crate::output;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal,
};
use dldr_library::{
    download_progress::{DownloadProgressTracker, DownloadWorkPlan},
    CancellationToken, TwoFactorPrompt, TwoFactorPromptRequest, TwoFactorPromptResponse,
    WorkDownloadProgress, WorkDownloadProgressSink,
};
use std::{
    io::{IsTerminal, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub struct Progress {
    state: Mutex<(DownloadProgressTracker, Instant)>,
    pub prompting: AtomicBool,
    quiet: bool,
}
impl Progress {
    pub fn new(plans: Vec<DownloadWorkPlan>, quiet: bool) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new((DownloadProgressTracker::new(plans), Instant::now())),
            prompting: AtomicBool::new(false),
            quiet,
        })
    }
    pub fn start(&self, id: &str) {
        let mut state = self.state.lock().expect("progress");
        state.0.start(id);
        if !self.quiet && !std::io::stderr().is_terminal() {
            eprintln!("{id}: starting");
        }
        self.render(&mut state, true);
    }
    pub fn finish(&self, id: &str, success: bool) {
        let mut state = self.state.lock().expect("progress");
        state.0.finish(id, success);
        if !self.quiet && !std::io::stderr().is_terminal() {
            eprintln!(
                "{id}: {}",
                if success {
                    "completed"
                } else {
                    "stopped / failed"
                }
            );
        }
        self.render(&mut state, true);
    }
    fn render(&self, state: &mut (DownloadProgressTracker, Instant), force: bool) {
        if self.quiet
            || self.prompting.load(Ordering::Relaxed)
            || !std::io::stderr().is_terminal()
            || (!force && state.1.elapsed() < Duration::from_millis(250))
        {
            return;
        }
        state.1 = Instant::now();
        let snapshot = state.0.snapshot();
        let mut line = format!("{}/{}", snapshot["completedWorks"], snapshot["totalWorks"]);
        for work in snapshot["activeWorks"]
            .as_array()
            .into_iter()
            .flatten()
            .take(4)
        {
            let bytes = work["bytesReceived"].as_u64().unwrap_or(0);
            let amount = work["bytesTotal"]
                .as_u64()
                .filter(|n| *n > 0)
                .map(|n| {
                    format!(
                        "{}%",
                        bytes
                            .saturating_mul(100)
                            .checked_div(n)
                            .unwrap_or(0)
                            .min(100)
                    )
                })
                .unwrap_or_else(|| output::bytes(bytes));
            let speed = work["bytesPerSecond"]
                .as_u64()
                .map(|n| format!(" {}/s", output::bytes(n)))
                .unwrap_or_default();
            line.push_str(&format!(
                " | {} {} {amount}{speed}",
                output::string(work, "workId"),
                output::string(work, "phase")
            ));
        }
        let width = terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(100)
            .saturating_sub(1);
        let line: String = line.chars().take(width).collect();
        eprint!("\r\x1b[2K{line}");
        let _ = std::io::stderr().flush();
    }
    pub fn clear(&self) {
        if !self.quiet && std::io::stderr().is_terminal() {
            eprint!("\r\x1b[2K");
        }
    }
    pub fn defer(&self, id: &str) {
        let mut state = self.state.lock().expect("progress");
        state.0.defer(id);
        if !self.quiet && !std::io::stderr().is_terminal() {
            eprintln!("{id}: paused");
        }
        self.render(&mut state, true);
    }
}

pub struct WorkSink {
    pub progress: Arc<Progress>,
    pub work_id: String,
}
impl WorkDownloadProgressSink for WorkSink {
    fn emit(&self, event: WorkDownloadProgress) {
        let mut state = self.progress.state.lock().expect("progress");
        state.0.observe(&self.work_id, &event);
        if let WorkDownloadProgress::Warning(message) = &event {
            if !self.progress.quiet {
                self.progress.clear();
                eprintln!("{}: {}", self.work_id, output::clean(message));
            }
        }
        self.progress.render(&mut state, false);
    }
}

pub struct Prompt {
    gate: tokio::sync::Mutex<()>,
    pub cancellation: CancellationToken,
    pub progress: Arc<Progress>,
}
impl Prompt {
    pub fn new(cancellation: CancellationToken, progress: Arc<Progress>) -> Self {
        Self {
            gate: tokio::sync::Mutex::new(()),
            cancellation,
            progress,
        }
    }
}
#[async_trait::async_trait]
impl TwoFactorPrompt for Prompt {
    async fn request_code(
        &self,
        request: TwoFactorPromptRequest,
    ) -> dldr_library::Result<TwoFactorPromptResponse> {
        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            return Err(dldr_library::LibraryError::TwoFactorPromptUnavailable(
                request.account_id,
            ));
        }
        let _gate = self.gate.lock().await;
        if self.cancellation.is_cancelled() {
            return Ok(TwoFactorPromptResponse::Cancelled);
        }
        let cancellation = self.cancellation.clone();
        let progress = self.progress.clone();
        tokio::task::spawn_blocking(move || read_code(&request, &cancellation, &progress))
            .await
            .map_err(|_| {
                dldr_library::LibraryError::SyncSource("Verification prompt failed".into())
            })?
    }
}

struct PromptGuard<'a> {
    progress: &'a Progress,
    raw: bool,
}
impl Drop for PromptGuard<'_> {
    fn drop(&mut self) {
        if self.raw {
            let _ = terminal::disable_raw_mode();
        }
        self.progress.prompting.store(false, Ordering::Relaxed);
        eprintln!("\r");
    }
}
fn read_code(
    request: &TwoFactorPromptRequest,
    cancellation: &CancellationToken,
    progress: &Progress,
) -> dldr_library::Result<TwoFactorPromptResponse> {
    let label = format!(
        "Verification code for {}{} (input hidden; Esc cancels):",
        output::clean(&request.account_label),
        if request.previous_code_rejected {
            " — previous code rejected"
        } else {
            ""
        }
    );
    read_hidden(&label, cancellation, progress)
}

/// Credentials are read from the terminal only and never echoed to a pipe or log.
pub fn read_hidden(
    label: &str,
    cancellation: &CancellationToken,
    progress: &Progress,
) -> dldr_library::Result<TwoFactorPromptResponse> {
    progress.prompting.store(true, Ordering::Relaxed);
    progress.clear();
    let mut guard = PromptGuard {
        progress,
        raw: false,
    };
    terminal::enable_raw_mode()?;
    guard.raw = true;
    eprint!("{}\r\n", output::clean(label));
    std::io::stderr().flush()?;
    let mut code = String::new();
    loop {
        if cancellation.is_cancelled() {
            return Ok(TwoFactorPromptResponse::Cancelled);
        }
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key.kind == event::KeyEventKind::Release {
                continue;
            }
            match key.code {
                KeyCode::Enter => {
                    return Ok(if code.is_empty() {
                        TwoFactorPromptResponse::Cancelled
                    } else {
                        TwoFactorPromptResponse::Code(code)
                    })
                }
                KeyCode::Esc => {
                    cancellation.cancel();
                    return Ok(TwoFactorPromptResponse::Cancelled);
                }
                KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    cancellation.cancel();
                    return Ok(TwoFactorPromptResponse::Cancelled);
                }
                KeyCode::Backspace => {
                    code.pop();
                }
                KeyCode::Char(c) if !c.is_control() && code.len() < 4096 => code.push(c),
                _ => {}
            }
        }
    }
}

pub struct SignalHandler(tokio::task::JoinHandle<()>);
impl Drop for SignalHandler {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub fn signals(cancellation: CancellationToken) -> SignalHandler {
    SignalHandler(tokio::spawn(async move {
        #[cfg(unix)]
        {
            if let Ok(mut term) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
            } else {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        cancellation.cancel();
    }))
}

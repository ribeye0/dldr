//! DLDR desktop application. Commands are adapters over the shared Rust workspace.
mod app;
mod commands;
mod diagnostics;
mod dlsite_play;
mod download_progress;
mod dto;
mod errors;
mod opener;
mod progress;
mod state;
mod two_factor;
mod validation;
mod window_appearance;

pub use app::run;

/// The verified portable package supplies its own updater helper.
pub fn run_update_helper_if_requested() -> bool {
    #[cfg(windows)]
    return commands::updates::portable::run_if_requested();
    #[cfg(not(windows))]
    false
}
pub(crate) use state::AppState;

#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod tests;

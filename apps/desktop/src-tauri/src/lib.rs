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
pub(crate) use state::AppState;

#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod tests;

//! Realer Xpad: sticky notes with Markdown and drawing. Powered by Rust.

pub mod app;
pub mod canvas;
pub mod cli;
pub mod drawing;
pub mod editor;
pub mod hypr;
pub mod manager;
pub mod markdown;
pub mod note_window;
pub mod store;
pub mod theme;

pub const APP_ID: &str = "ch.giosuel.RealerXpad";
pub const APP_NAME: &str = "Realer Xpad";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

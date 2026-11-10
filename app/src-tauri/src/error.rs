//! One serialisable error type for every command. It crosses the IPC boundary as a plain
//! string, exactly like the `Result<_, String>` the first commands returned, so the frontend
//! keeps doing `String(e)`.

use serde::{Serialize, Serializer};

#[derive(Debug)]
pub struct AppError(String);

pub type AppResult<T> = Result<T, AppError>;

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// Explicit `From`s for the error types the commands meet today. For any other `Display` error
// use `.app()` (below), so a feature never needs to edit this file just to propagate one.
impl From<pm_formats::library::Error> for AppError {
    fn from(e: pm_formats::library::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        Self(e.to_string())
    }
}

/// `result.app()?` converts any `Result<T, impl Display>` into an `AppResult<T>`.
#[allow(dead_code)] // the escape hatch for feature areas; unused until one needs it
pub trait IntoApp<T> {
    fn app(self) -> AppResult<T>;
}

impl<T, E: std::fmt::Display> IntoApp<T> for Result<T, E> {
    fn app(self) -> AppResult<T> {
        self.map_err(|e| AppError(e.to_string()))
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

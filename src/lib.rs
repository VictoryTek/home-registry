//! Home Registry Library
//!
//! Core functionality for the Home Registry home inventory management system.

#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::too_many_lines)]

// Public modules
pub mod access_log;
pub mod api;
pub mod auth;
pub mod db;
pub mod models;
pub mod rate_limit;
pub mod startup;

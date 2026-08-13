#![forbid(unsafe_code)]

//! Shared application services and ports. HTTP and CLI adapters call these same use cases.

mod error;
mod ports;
mod service;

pub use casegraph_domain as domain;
pub use error::{AppError, ErrorKind};
pub use ports::*;
pub use service::*;

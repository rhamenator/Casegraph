#![forbid(unsafe_code)]

//! Shared application services and ports. HTTP and CLI adapters call these same use cases.

mod error;
mod extension;
mod pipeline;
mod ports;
mod reasoning;
mod rules;
mod service;

pub use casegraph_domain as domain;
pub use error::{AppError, ErrorKind};
pub use extension::*;
pub use pipeline::*;
pub use ports::*;
pub use reasoning::*;
pub use rules::*;
pub use service::*;

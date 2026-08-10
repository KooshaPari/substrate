//! Nexus error types

use std::fmt;
use std::error::Error as StdError;

#[derive(Debug)]
pub enum NexusError {
    NotFound(String),
    AlreadyExists(String),
    InvalidEndpoint(String),
    NetworkError(String),
    InternalError(String),
}

impl fmt::Display for NexusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NexusError::NotFound(s) => write!(f, "not found: {}", s),
            NexusError::AlreadyExists(s) => write!(f, "already exists: {}", s),
            NexusError::InvalidEndpoint(s) => write!(f, "invalid endpoint: {}", s),
            NexusError::NetworkError(s) => write!(f, "network error: {}", s),
            NexusError::InternalError(s) => write!(f, "internal error: {}", s),
        }
    }
}

impl StdError for NexusError {}

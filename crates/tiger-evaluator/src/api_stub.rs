//! The browser and WASI build's `api` stub.
//!
//! A sandboxed target has no sockets, so the module keeps its surface and every
//! request reports a clear error instead of linking a network stack. The pure
//! `parse_url` and `encode_query` helpers are unaffected and work on every
//! target, because they never open a connection.

use crate::apimod::{Request, Response};

/// The single message every request reports on a sandboxed target.
pub const UNAVAILABLE: &str =
    "the api module cannot open network connections in the browser or WASI build; use the native CLI or a standalone executable";

pub fn request(_request: &Request) -> Result<Response, String> {
    Err(UNAVAILABLE.to_string())
}

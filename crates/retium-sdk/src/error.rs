//! One error type for the whole client.
//!
//! Two of these matter when you submit a transaction:
//!
//! * [`Error::Refused`] means the node looked at it and said no. Sending the
//!   same bytes again will not help, so fix the transaction.
//! * [`Error::Busy`] means the node could not take it right now. The same
//!   transaction may well succeed a moment later.

/// Anything that can go wrong locally or at the node.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A key, signature or file was the wrong size.
    #[error("{what} must be {expected} bytes, got {got}")]
    BadLength {
        /// What was being read.
        what: &'static str,
        /// How many bytes it should have been.
        expected: usize,
        /// How many bytes it was.
        got: usize,
    },

    /// Bytes or text that could not be decoded.
    #[error("could not read {0}")]
    BadEncoding(&'static str),

    /// A bare 32-byte seed where a key file was expected.
    #[error(
        "a key file is 37 bytes (RTSK, version, seed), this file is 32. \
         Wrap a raw seed with `retium key import <file>`"
    )]
    RawSeedFile,

    /// The address is malformed or its checksum does not match.
    #[error("{0} is not a valid Retium address")]
    InvalidAddress(String),

    /// The amount could not be represented exactly.
    #[error("{0} is not a valid amount of RTM")]
    InvalidAmount(String),

    /// The node refused this transaction. A definitive answer.
    #[error("the node refused this transaction ({0})")]
    Refused(String),

    /// The node is busy or mid-handover. Try the same transaction again shortly.
    #[error("the node is busy, try again shortly ({0})")]
    Busy(String),

    /// Checked locally and not sent. The message says what to change.
    #[error("{0}")]
    Invalid(String),

    /// Any other answer from the node.
    #[error("node error HTTP {status} ({body})")]
    Node {
        /// HTTP status code.
        status: u16,
        /// What the node said.
        body: String,
    },

    /// The request never got an answer.
    #[error("could not reach the node ({0})")]
    Transport(String),

    /// The answer did not have the shape this client expects.
    #[error("unexpected answer from the node ({0})")]
    Unexpected(String),

    /// A local file problem.
    #[error("{0}")]
    Io(String),
}

impl Error {
    /// True when sending the same transaction again may work.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Error::Busy(_) | Error::Transport(_))
    }
}

/// `Result` with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_busy_and_transport_are_worth_retrying() {
        assert!(Error::Busy("no open blocks".into()).is_retryable());
        assert!(Error::Transport("timed out".into()).is_retryable());
        assert!(!Error::Refused("bad signature".into()).is_retryable());
        assert!(!Error::InvalidAddress("nope".into()).is_retryable());
    }

    #[test]
    fn a_raw_seed_file_says_what_to_do_about_it() {
        let message = Error::RawSeedFile.to_string();
        assert!(message.contains("key import"), "{message}");
    }
}

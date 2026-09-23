//! One error type for the whole client.
//!
//! Two of these matter when you submit a transaction:
//!
//! * [`Error::Refused`] — the node looked at it and said no. Sending the same
//!   bytes again will not help; fix the transaction.
//! * [`Error::Busy`] — the node could not take it right now. The same
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

    /// A key file from before the post-quantum migration.
    #[error(
        "this is a pre-2026 32-byte key file. Retium now uses ML-DSA-44 keys \
         (37-byte RTSK files); re-wrap the same seed with `retium key import --seed <hex>`"
    )]
    LegacyKeyFile,

    /// The address is malformed or its checksum does not match.
    #[error("{0} is not a valid Retium address")]
    InvalidAddress(String),

    /// The amount could not be represented exactly.
    #[error("{0} is not a valid amount of RTM")]
    InvalidAmount(String),

    /// The node refused this transaction. A definitive answer.
    #[error("the node refused this transaction: {0}")]
    Refused(String),

    /// The node is busy or mid-handover; try the same transaction again shortly.
    #[error("the node is busy, try again shortly: {0}")]
    Busy(String),

    /// Any other answer from the node.
    #[error("node error (HTTP {status}): {body}")]
    Node {
        /// HTTP status code.
        status: u16,
        /// What the node said.
        body: String,
    },

    /// The request never got an answer.
    #[error("could not reach the node: {0}")]
    Transport(String),

    /// The answer did not have the shape this client expects.
    #[error("unexpected answer from the node: {0}")]
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
    fn a_legacy_key_file_says_what_to_do_about_it() {
        let message = Error::LegacyKeyFile.to_string();
        assert!(message.contains("re-wrap"), "{message}");
    }
}

//! # Retium SDK
//!
//! A Rust client for the [Retium](https://retium.org) blockchain: addresses,
//! keys, transaction signing, and a typed client for the public RPC.
//!
//! Retium signs with ML-DSA-44 (FIPS 204), a post-quantum scheme. Keys and
//! signatures are larger than the elliptic-curve ones you may be used to (a
//! public key is 1,312 bytes and a signature 2,421), but the shape of the work
//! is the same: build a payload, hash it, sign it, send it.
//!
//! ```no_run
//! use retium_sdk::{key::SecretKey, rpc::Client, tx};
//!
//! # async fn example() -> retium_sdk::Result<()> {
//! let key = SecretKey::read_key_file("my-wallet.sk")?;
//! let client = Client::new("https://rpc.retium.org")?;
//!
//! let wallet = client.wallet(&key.address()).await?.expect("a funded account");
//! println!("balance: {} RTM", wallet.balance_rtm);
//!
//! let sent = tx::transfer(&client, &key, "rete…recipient…", "1.5").await?;
//! println!("sent, tx {}", sent.tx_id);
//! # Ok(())
//! # }
//! ```
//!
//! ## Where to start
//!
//! - [`key`]: create, load and back up an account
//! - [`address`]: checksummed addresses; validate anything a user typed
//! - [`amount`]: RTM and atomic units, without floating point
//! - [`canonical`]: exactly what a transaction signs
//! - [`contract`]: contract ids and RCP-1 metadata
//! - [`rpc`]: the client
//! - [`tx`]: build, sign and submit in one call
//!
//! ## Testnet
//!
//! The public test network is for building and breaking things. RTM there has
//! no value and no one should pay for it. Nothing in this SDK can be used to
//! sell it as if it did.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod address;
pub mod amount;
pub mod canonical;
pub mod contract;
pub mod error;
pub mod key;

#[cfg(feature = "rpc")]
pub mod rpc;
#[cfg(feature = "rpc")]
pub mod tx;

pub use error::{Error, Result};

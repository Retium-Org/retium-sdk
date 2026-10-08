//! `retium`, a command-line tool for the Retium blockchain.
//!
//! Everything it does, the library does too. This is a thin front end over
//! [`retium_sdk`], not a separate implementation.
//!
//! It is meant to be scriptable. Failures print to stderr and exit non-zero,
//! and `--json` prints one JSON object per command.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use retium_sdk::{
    address, amount,
    canonical::{ChainId, FEE_RTD, FEE_RTM},
    contract,
    key::SecretKey,
    rpc, tx, Error,
};

/// Command-line client for the Retium blockchain.
#[derive(Parser)]
#[command(name = "retium", version, about, long_about = None)]
struct Cli {
    /// RPC endpoint.
    #[arg(long, env = "RETIUM_RPC", default_value = rpc::PUBLIC_RPC, global = true)]
    rpc: String,

    /// API key for writes. Prefer the environment variable, because a key on the
    /// command line is visible to every process on the machine.
    #[arg(long, env = "RETIUM_API_KEY", hide_env_values = true, global = true)]
    api_key: Option<String>,

    /// Chain id to sign for (mainnet, testnet or devnet). Every network signs
    /// as devnet until mainnet, the public testnet included, so leave it alone.
    #[arg(long, env = "RETIUM_CHAIN", default_value = "devnet", global = true)]
    chain: String,

    /// Coin to pay gas in, RTM or RTD (Retium Dollar, 1.5x the RTM fee).
    #[arg(long, env = "RETIUM_FEE", default_value = "RTM", global = true)]
    fee: String,

    /// Key file to sign with.
    #[arg(long, env = "RETIUM_KEY", global = true)]
    key: Option<PathBuf>,

    /// Print machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create or inspect an account.
    #[command(subcommand)]
    Key(KeyCommand),
    /// Show an account's balance and next nonce.
    Balance {
        /// Address to look up. Defaults to your key's address.
        address: Option<String>,
    },
    /// Send RTM.
    Send {
        /// Recipient address.
        to: String,
        /// Amount in RTM, e.g. 1.5
        amount: String,
    },
    /// Deploy a contract.
    Deploy {
        /// Path to the .wasm file.
        wasm: PathBuf,
        /// Path to a JSON file of deploy metadata (name, symbol and so on).
        #[arg(long)]
        metadata: Option<PathBuf>,
        /// Print the contract id and stop.
        #[arg(long)]
        dry_run: bool,
    },
    /// Call a contract method (costs gas, changes state).
    Call {
        /// Contract id.
        contract_id: String,
        /// Method name.
        method: String,
        /// Arguments, in order.
        args: Vec<String>,
        /// RTM to attach to the call.
        #[arg(long)]
        value: Option<String>,
    },
    /// Read a contract with any argument text, sent as JSON (free, but the
    /// public endpoint wants an API key for it).
    View {
        /// Contract id.
        contract_id: String,
        /// Method name.
        method: String,
        /// Arguments, in order.
        args: Vec<String>,
    },
    /// Read a contract without sending a transaction (free, no API key).
    Query {
        /// Contract id.
        contract_id: String,
        /// Method name.
        method: String,
        /// Arguments, in order.
        args: Vec<String>,
    },
    /// Show a transaction's progress.
    Tx {
        /// Transaction id.
        tx_id: String,
        /// Wait until it settles.
        #[arg(long)]
        wait: bool,
    },
    /// Show a deployed contract, including its code hash.
    Contract {
        /// Contract id.
        contract_id: String,
    },
    /// Check the code hash of a local .wasm against what is deployed.
    Verify {
        /// Contract id on the chain.
        contract_id: String,
        /// The .wasm you built.
        wasm: PathBuf,
    },
    /// Show the network's status.
    Health,
}

#[derive(Subcommand)]
enum KeyCommand {
    /// Create a new account and write its key file.
    New {
        /// Where to write the key file.
        out: PathBuf,
        /// Also print the secret seed, so you can write it down. Anyone who
        /// sees it owns the account.
        #[arg(long)]
        show_seed: bool,
    },
    /// Show the address and public key of a key file.
    Show,
    /// Write a key file from a 64-character seed you already have.
    Import {
        /// Where to write the key file.
        out: PathBuf,
        /// The seed, as 64 hex characters.
        #[arg(long)]
        seed: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            if json {
                let message = serde_json::to_string(&e.to_string()).unwrap_or_default();
                eprintln!("{{\"ok\":false,\"error\":{message}}}");
            } else {
                eprintln!("error: {e}");
                if e.is_retryable() {
                    eprintln!("hint: this one is worth trying again in a moment");
                }
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> retium_sdk::Result<String> {
    let chain = ChainId::parse(&cli.chain)
        .ok_or_else(|| Error::Invalid(format!("unknown network '{}'", cli.chain)))?;
    let fee_currency = match cli.fee.trim().to_ascii_uppercase().as_str() {
        "RTM" => FEE_RTM,
        "RTD" => FEE_RTD,
        other => {
            return Err(Error::Invalid(format!(
                "unknown gas coin '{other}', use RTM or RTD"
            )))
        }
    };
    let opts = tx::Options {
        chain,
        fee_currency,
    };

    // Key commands need no network.
    if let Command::Key(cmd) = &cli.command {
        return key_command(cmd, cli.key.as_deref(), cli.json);
    }

    let client = {
        let mut b = rpc::Client::builder(&cli.rpc);
        if let Some(k) = &cli.api_key {
            b = b.api_key(k);
        }
        b.build()?
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Io(e.to_string()))?;

    runtime.block_on(async move {
        match cli.command {
            Command::Key(_) => unreachable!("handled above"),

            Command::Balance { address } => {
                let address = match address {
                    Some(a) => a,
                    None => load_key(cli.key.as_deref())?.address(),
                };
                match client.wallet(&address).await? {
                    None => Ok(out(
                        cli.json,
                        &serde_json::json!({"address": address, "known": false}),
                        &format!("{address}\n  the chain has no record of this account yet"),
                    )),
                    Some(w) => Ok(out(
                        cli.json,
                        &serde_json::json!({
                            "address": w.address,
                            "balance_rtm": w.balance_rtm,
                            "staked_pie": w.balance_pie,
                            "next_nonce": w.nonce,
                        }),
                        &format!(
                            "{}\n  balance    {} RTM\n  staked     {} PIE\n  next nonce {}",
                            w.address, w.balance_rtm, w.balance_pie, w.nonce
                        ),
                    )),
                }
            }

            Command::Send { to, amount: rtm } => {
                let key = load_key(cli.key.as_deref())?;
                let sent = tx::transfer_with(&client, &key, &to, &rtm, opts).await?;
                Ok(out(
                    cli.json,
                    &serde_json::json!({"tx_id": sent.tx_id, "to": to, "amount_rtm": rtm}),
                    &format!("sent {rtm} RTM to {to}\n  tx {}", sent.tx_id),
                ))
            }

            Command::Deploy { wasm, metadata, dry_run } => {
                let key = load_key(cli.key.as_deref())?;
                let bytes = std::fs::read(&wasm)
                    .map_err(|e| Error::Io(format!("{}: {e}", wasm.display())))?;
                let meta = match &metadata {
                    Some(path) => {
                        let text = std::fs::read_to_string(path)
                            .map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
                        Some(serde_json::from_str::<serde_json::Value>(&text).map_err(|e| {
                            Error::Invalid(format!("{} is not valid JSON ({e})", path.display()))
                        })?)
                    }
                    None => None,
                };
                let checked = contract::preflight(&bytes, meta.as_ref())?;
                let id = contract::contract_id(&bytes, &key.address(), contract::DEPLOY_WEIGHT, meta.as_ref());
                if dry_run {
                    let m = &checked.metadata;
                    let traits = if m.traits.is_empty() {
                        "none".to_string()
                    } else {
                        m.traits.join(", ")
                    };
                    return Ok(out(
                        cli.json,
                        &serde_json::json!({
                            "contract_id": id,
                            "code_hash": checked.code_hash,
                            "size_bytes": bytes.len(),
                            "simulated_weight": checked.simulated_weight,
                            "embedded_metadata": checked.embedded,
                            "metadata": m,
                            "submitted": false,
                        }),
                        &format!(
                            "would deploy {} ({} bytes)\n  contract id {id}\n  code hash   {}\n  type        {}\n  version     {}\n  weight      {} (the chain scores the code at {})\n  traits      {traits}\n  metadata    {}",
                            wasm.display(),
                            bytes.len(),
                            checked.code_hash,
                            m.rcp_type,
                            m.version,
                            m.weight,
                            checked.simulated_weight,
                            if checked.embedded { "embedded in the wasm" } else { "none embedded" },
                        ),
                    ));
                }
                let sent = tx::deploy_with(&client, &key, &bytes, meta.as_ref(), opts).await?;
                let id = sent.contract_id.clone().unwrap_or(id);
                Ok(out(
                    cli.json,
                    &serde_json::json!({
                        "tx_id": sent.tx_id,
                        "contract_id": id,
                        "code_hash": contract::code_hash(&bytes),
                    }),
                    &format!(
                        "deployed {}\n  contract id {id}\n  code hash   {}\n  tx          {}",
                        wasm.display(),
                        contract::code_hash(&bytes),
                        sent.tx_id
                    ),
                ))
            }

            Command::Call { contract_id, method, args, value } => {
                let key = load_key(cli.key.as_deref())?;
                let sent =
                    tx::call_with(&client, &key, &contract_id, &method, &args, value.as_deref(), opts).await?;
                Ok(out(
                    cli.json,
                    &serde_json::json!({"tx_id": sent.tx_id, "contract_id": contract_id, "method": method}),
                    &format!("called {contract_id}.{method}\n  tx {}", sent.tx_id),
                ))
            }

            Command::Query { contract_id, method, args } => {
                let answer = client.query(&contract_id, &method, &args).await?;
                Ok(out(cli.json, &answer, &answer.to_string()))
            }

            Command::View { contract_id, method, args } => {
                let answer = client.view(&contract_id, &method, &args).await?;
                Ok(out(cli.json, &answer, &answer.to_string()))
            }

            Command::Tx { tx_id, wait } => {
                let finality = if wait {
                    client
                        .wait_for_final(&tx_id, std::time::Duration::from_secs(600))
                        .await?
                } else {
                    client.finality(&tx_id).await?
                };
                Ok(out(
                    cli.json,
                    &serde_json::json!({"tx_id": tx_id, "finality": format!("{finality:?}")}),
                    &format!("{tx_id} is {finality:?}"),
                ))
            }

            Command::Contract { contract_id } => match client.contract(&contract_id).await? {
                None => Err(Error::Invalid(format!("no contract {contract_id}"))),
                Some(info) => Ok(out(
                    cli.json,
                    &serde_json::json!({
                        "contract_id": info.contract_id,
                        "code_hash": info.code_hash,
                        "rcp_metadata": info.rcp_metadata,
                    }),
                    &format!(
                        "{}\n  code hash {}\n  metadata  {}",
                        info.contract_id,
                        info.code_hash.clone().unwrap_or_else(|| "unknown".into()),
                        info.rcp_metadata
                            .as_ref()
                            .map(|m| m.to_string())
                            .unwrap_or_else(|| "none".into())
                    ),
                )),
            },

            Command::Verify { contract_id, wasm } => {
                let bytes = std::fs::read(&wasm)
                    .map_err(|e| Error::Io(format!("{}: {e}", wasm.display())))?;
                let local = contract::code_hash(&bytes);
                let deployed = client
                    .contract(&contract_id)
                    .await?
                    .and_then(|c| c.code_hash)
                    .ok_or_else(|| Error::Invalid(format!("no code hash for {contract_id}")))?;
                let same = local == deployed;
                let text = if same {
                    format!("match\n  {contract_id}\n  runs exactly the code in {}", wasm.display())
                } else {
                    format!(
                        "DIFFERENT\n  deployed {deployed}\n  local    {local}\n  the chain is not running this file"
                    )
                };
                if same {
                    Ok(out(
                        cli.json,
                        &serde_json::json!({"match": true, "code_hash": local}),
                        &text,
                    ))
                } else {
                    Err(Error::Invalid(text))
                }
            }

            Command::Health => {
                let h = client.health().await?;
                Ok(out(
                    cli.json,
                    &serde_json::json!({"status": h.status, "tick": h.tick, "version": h.version}),
                    &format!("{}, tick {}, node {}", h.status, h.tick, h.version),
                ))
            }
        }
    })
}

fn key_command(
    cmd: &KeyCommand,
    key_path: Option<&std::path::Path>,
    json: bool,
) -> retium_sdk::Result<String> {
    match cmd {
        KeyCommand::New {
            out: path,
            show_seed,
        } => {
            let key = SecretKey::generate();
            key.write_key_file(path)?;
            let mut text = format!(
                "new account\n  address  {}\n  key file {}",
                key.address(),
                path.display()
            );
            if *show_seed {
                text.push_str(&format!(
                    "\n  seed     {}\n\nWrite the seed down and keep it offline. It is the account.\nAnyone who sees it can spend everything. It is not shown again.",
                    key.seed_hex()
                ));
            } else {
                text.push_str("\n\nBack up the key file. Without it the account is gone.\nRun with --show-seed if you want the seed to write down.");
            }
            Ok(out(
                json,
                &serde_json::json!({"address": key.address(), "key_file": path.display().to_string()}),
                &text,
            ))
        }
        KeyCommand::Import { out: path, seed } => {
            let key = SecretKey::from_hex_seed(seed)?;
            key.write_key_file(path)?;
            Ok(out(
                json,
                &serde_json::json!({"address": key.address(), "key_file": path.display().to_string()}),
                &format!(
                    "imported\n  address  {}\n  key file {}",
                    key.address(),
                    path.display()
                ),
            ))
        }
        KeyCommand::Show => {
            let key = load_key(key_path)?;
            let pk = key.public_key().to_hex();
            Ok(out(
                json,
                &serde_json::json!({"address": key.address(), "public_key": pk}),
                &format!("{}\n  public key {}", key.address(), pk),
            ))
        }
    }
}

fn load_key(path: Option<&std::path::Path>) -> retium_sdk::Result<SecretKey> {
    let path = path
        .ok_or_else(|| Error::Io("no key file, pass --key <file> or set RETIUM_KEY".to_string()))?;
    SecretKey::read_key_file(path)
}

fn out(json: bool, value: &serde_json::Value, text: &str) -> String {
    if json {
        value.to_string()
    } else {
        text.to_string()
    }
}

#[allow(dead_code)]
fn unused_reexports() {
    // Keeps the lints honest about what this binary is expected to use.
    let _ = (address::is_valid("x"), amount::parse_rtm("1").is_ok());
}

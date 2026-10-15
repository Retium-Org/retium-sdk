# Building with an AI agent

You can hand Retium to an AI coding assistant and let it do the work: make an account, deploy a contract, call it, send coins, read the chain. This page is for the person running the agent. It says what to give the agent, what the agent will ask you for, and how to keep your money and keys safe while it works.

## How it works

The SDK repository has a file called [`AGENTS.md`](../AGENTS.md). It is written for the agent, not for you. It explains Retium in five lines, lists the rules the agent must follow with your key, and gives every common task with the exact `retium` command and what a correct result looks like. An agent that has read it needs nothing else.

Everything runs on your machine. The agent installs the `retium` tool from the SDK, and that tool signs with ML-DSA-44 locally. The key never goes to any server, not to Retium's and not to the agent's. There is no hosted service to connect to and no account to create with us. Reads go straight to the public endpoint and are free. Writes need an API key, which you create once.

## 1. Set up

Clone the SDK where the agent works.

```bash
git clone https://github.com/Retium-Org/retium-sdk
cd retium-sdk
```

Most coding assistants read a file named `AGENTS.md` from the repository root without being told. If yours does not, tell it once:

> Read AGENTS.md in this repository and follow it. Then install the retium tool and check the network is up.

The agent runs `cargo install --locked --path crates/retium-sdk` and `retium health`. It needs Rust 1.85 or newer and git on the machine, nothing else.

## 2. What the agent will ask you for

Two things need a person, and the file tells the agent to stop and ask.

**Money.** A new account has nothing. The agent gives you its address and asks you to fund it. Claim RTD at [retium.money](https://retium.money) after linking your Discord account, buy some RTM with it in the Safehouse there, and send a little to the agent's address from your wallet. Testnet coins have no value. 5 RTM covers a deploy and about eight calls.

**The first API key.** Every write needs one, and the first key can only be created from a wallet in a browser. Open [retium.net/api-keys](https://retium.net/api-keys), connect your wallet, create a free key and give it to the agent as the `RETIUM_API_KEY` environment variable, not in the chat. The agent can make further keys itself from then on.

Once it has both, the agent works on its own.

## 3. What to tell it to do

Plain requests work. These are the tasks the file covers.

| You say | The agent does |
|---|---|
| "Deploy the hello_counter example and make me the owner" | builds, dry-runs, deploys, waits for final, sends `init`, checks `owner`, verifies the code hash |
| "Send 1.5 RTM to rete..." | checks the address, sends, waits for `HardFinal` |
| "What is the balance of rete..." | one free read, no transaction |
| "Call bump 3 on contract ..." | sends the call, waits, reports the contract's reply or its refusal reason |
| "Write me a token contract and deploy it" | reads [Writing contracts](04-writing-contracts.md) and [Tokens](09-tokens.md), builds, tests, dry-runs, deploys |

For a contract you wrote, tell the agent the method names, or put them in your own project's `AGENTS.md`. Retium does not publish a contract's methods on chain, and the file tells the agent never to guess one, because a call to an unknown method still costs gas.

## 4. Rules the agent follows

These are in the file. Knowing them tells you what to expect.

* It never prints, logs or sends the seed or the key file. If you want the seed backed up, you ask, and it shows it only to you.
* It never puts the API key on a command line. The key lives in the environment.
* Before signing a transfer or a call you did not spell out exactly, it tells you who receives, how much, and which contract and method.
* It calls a transaction done only when `retium tx <id> --wait` says `HardFinal`, and for a call or deploy also `result ok`. "Submitted" is not done.
* It retries only when the network is busy or unreachable. A refusal is not retried.
* It sends one transaction at a time from one account, so nonces do not collide.
* When something needs a person, it stops and says exactly what to do and where.

## 5. Keep it on a budget

An agent can make mistakes, and the simplest protection costs nothing.

* **Give the agent its own account.** Let it create a key file with `retium key new` and fund that, instead of handing it your main wallet. Whatever happens, it can only spend what you sent.
* **Give it its own API key.** Create one key for the agent and revoke it when the job is done with `retium api-key revoke`. Making a key is a transaction that costs a cent of gas, so a second key is not an expense worth avoiding.
* **Keep key files out of repositories.** This repo ignores `*.sk` files. If the agent works elsewhere, check its project does too.
* **Check its work in the explorer.** Every transaction id the agent reports opens at [retium.net](https://retium.net). A deploy shows the contract id and the code hash, a call shows the result, a refusal shows the reason.

A transfer costs 0.01 RTM, a deploy 0.5 RTM and a call to a weight-5 contract 0.5 RTM, so a 10 RTM account is enough for an afternoon of building and cannot lose more than that.

## 6. The facts in one file

Next to `AGENTS.md` sit two more files. [`retium-agent.json`](../retium-agent.json) holds the network's facts as data: the endpoint, chain id, signature scheme, coins, fee table, API key rules and limits. An agent reads numbers from there instead of from prose. [`llms.txt`](../llms.txt) is an index of every page in this documentation, for agents that prefer to start from a list.

## What is not there yet

There is no hosted service an agent can connect to without installing the SDK, and no way for an agent to get testnet coins or its first API key without you. Agents also see the same error messages people do, as text, listed in [Errors](11-errors.md). Those are the next steps, and the file will say so when they change.

Read on with [Security](07-security.md) for how keys work, or go back to the [Quickstart](02-quickstart.md) to do the same tasks by hand.

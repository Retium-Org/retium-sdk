//! The signed byte layout, spelled out by hand.
//!
//! `docs/08-wire-reference.md` tells other languages how to build the bytes a
//! Retium signature covers. This test builds them by hand from that description
//! and checks the SDK agrees, so the page cannot drift away from the code.

use retium_sdk::canonical::{CanonicalPayload, ChainId};

fn string(s: &str) -> Vec<u8> {
    let mut out = (s.len() as u64).to_le_bytes().to_vec();
    out.extend_from_slice(s.as_bytes());
    out
}

fn list(items: &[&str]) -> Vec<u8> {
    let mut out = (items.len() as u64).to_le_bytes().to_vec();
    for item in items {
        out.extend_from_slice(&string(item));
    }
    out
}

const ALICE: &str = "rete0000000000000000000000000000000000000000000000000000000000000000dead";
const BOB: &str = "rete1111111111111111111111111111111111111111111111111111111111111111beef";

#[test]
fn a_transfer_matches_the_layout_in_the_wire_reference() {
    let amount: u128 = 1_500_000_000_000_000_000;
    let claim: u128 = 25_000_000_000_000_000_000;
    let nonce: u64 = 7;

    let payload =
        CanonicalPayload::transfer(ALICE, BOB, amount, nonce, claim).on_chain(ChainId::Testnet);

    // Built by hand, exactly as docs/08-wire-reference.md §3 describes.
    let mut expected = Vec::new();
    expected.push(5u8); // version
    expected.extend_from_slice(&2u32.to_le_bytes()); // chain_id, testnet
    expected.extend_from_slice(&string("transfer"));
    expected.extend_from_slice(&string(ALICE));
    expected.extend_from_slice(&string(BOB));
    expected.extend_from_slice(&amount.to_le_bytes());
    expected.extend_from_slice(&nonce.to_le_bytes());
    expected.extend_from_slice(&claim.to_le_bytes());
    expected.extend_from_slice(&string("")); // method
    expected.extend_from_slice(&[0u8; 32]); // args_hash
                                            // declared, sorted by byte order, de-duplicated
    expected.extend_from_slice(&list(&[ALICE, BOB, "rete_treasury"]));
    expected.push(0u8); // fee_currency, RTM

    let actual = bincode::serialize(&payload).expect("serialises");
    assert_eq!(
        actual, expected,
        "the signed bytes no longer match docs/08-wire-reference.md"
    );

    // And the signed message is the hash of exactly those bytes.
    assert_eq!(payload.signing_hash(), *blake3::hash(&expected).as_bytes());
}

#[test]
fn a_call_hashes_its_arguments_joined_with_commas() {
    let payload =
        CanonicalPayload::call(ALICE, "contract-id", "transfer", "rete...bob,10", 0, 1, 0);
    assert_eq!(
        payload.args_hash,
        *blake3::hash(b"rete...bob,10").as_bytes(),
        "args_hash is blake3 of the comma-joined argument string"
    );
    assert!(
        payload.declared.is_empty(),
        "a call declares nothing, the chain derives the write set"
    );
}

#[test]
fn declared_lists_are_sorted_and_deduplicated() {
    let payload = CanonicalPayload::deploy(ALICE, "some-contract-id", 0, 0);
    let mut sorted = payload.declared.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(payload.declared, sorted);
    assert!(payload.declared.contains(&"rete_treasury".to_string()));
}

#[test]
fn every_chain_id_changes_the_signed_bytes() {
    let base = CanonicalPayload::transfer(ALICE, BOB, 1, 0, 1);
    for chain in [ChainId::Mainnet, ChainId::Testnet] {
        assert_ne!(
            base.signing_hash(),
            base.clone().on_chain(chain).signing_hash(),
            "a transaction signed for one network must not be valid on another"
        );
    }
}

/// Test vectors published with the node for canonical v5. The
/// same transfer with gas in RTM and in RTD, on the development chain id.
#[test]
fn transfers_match_the_node_v5_vectors() {
    let sender = format!("rete72bf{}a5ad", "00".repeat(30));
    let to = format!("rete11aa{}b6be", "00".repeat(30));
    let p = CanonicalPayload::transfer(
        &sender,
        &to,
        1_000_000_000_000_000_000,
        7,
        500_000_000_000_000_000_000,
    );
    let hex = |h: [u8; 32]| h.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    assert_eq!(
        hex(p.signing_hash()),
        "7d87a786c4d7dc516561dacd26af822b72bdf14f2b03d89b5f2297c7a8c87c15"
    );
    let p = p.with_fee_currency(retium_sdk::canonical::FEE_RTD);
    assert_eq!(
        hex(p.signing_hash()),
        "1266cbfaca9639e4ae6e37782ec8f2684bbd1eab8cf7e587f532d90bbc22db19"
    );
}

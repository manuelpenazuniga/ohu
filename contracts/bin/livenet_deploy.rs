//! Deploy OhuVault + MutualPool contracts to Casper Testnet via Odra Livenet.
//!
//! # Usage
//!
//! ```bash
//! # 1. Ensure .env (gitignored) has the required env vars:
//! #    ODRA_CASPER_LIVENET_NODE_ADDRESS=<node url>
//! #    ODRA_CASPER_LIVENET_CHAIN_NAME=casper-test
//! #    ODRA_CASPER_LIVENET_SECRET_KEY_PATH=<path to deployer secret_key.pem>
//! #    ODRA_CASPER_LIVENET_KEY_1=<path to admin secret_key.pem>  (for setters)
//! #    OHUVAULT_ADMIN_ACCOUNT_HASH=<account-hash-...>
//! #    OHUVAULT_OPERATOR_ACCOUNT_HASH=<account-hash-...>
//! #    OHUVAULT_APPROVER_ACCOUNT_HASHES=<account-hash-... account-hash-...>
//! #    OHUVAULT_REQUIRED_APPROVALS=<u8>
//! #    OHUVAULT_MICROPAYMENT_CAP_MOTES=<motes>
//! #    OHUVAULT_EPOCH_CAP_MOTES=<motes>
//! #    OHUVAULT_EPOCH_WINDOW_MS=<u64>
//! #    OHUVAULT_CHAIN_ID=<u64>
//! #    OHUVAULT_QUORUM_FAIL_BPS=<u64>
//! #    OHUVAULT_ATTESTATION_WINDOW_MS=<u64>
//! #    OHUVAULT_PREMIUM_BPS=<u64>
//! #    OHUVAULT_INDEMNITY_TARGET_BPS=<u64>
//! #
//! # 2. Source the .env:  set -a && source .env && set +a
//! #
//! # 3. Build + lower + deploy en un paso (RECOMENDADO):
//! #    bash infra/scripts/deploy_testnet.sh
//! #
//! # NOTA: el WASM crudo de `cargo odra build` trae bulk-memory/sign-ext que la
//! # VM de Casper RECHAZA. Hay que bajarlos a MVP con `wasm-opt` antes de deployar
//! # (lo hace el script de arriba). Correr `cargo run` con un WASM sin lowering
//! # falla on-chain con "Wasm preprocessing error: Bulk memory operations...".
//! ```
//!
//! # Gas
//!
//! Deploy gas is set to 600_000_000_000 motes (600 CSPR). This is a conservative
//! estimate for a ~500 KB WASM (1 CSPR ~ 1000 motes per typical recommendation).
//! Adjust `DEPLOY_GAS_MOTES` if the compiled WASM size differs significantly.

use odra::casper_types::U512;
use odra::host::Deployer;
use odra::prelude::*;
use std::str::FromStr;

use ohu_contracts::mutual_pool::{MutualPool, MutualPoolHostRef, MutualPoolInitArgs};
use ohu_contracts::ohu_vault::{OhuVault, OhuVaultHostRef, OhuVaultInitArgs};

// ── Gas ──────────────────────────────────────────────────────────────────────
// 600 CSPR = 600_000_000_000 motes. Se estimó para ~500 KB de WASM.
// 1 CSPR = 1_000_000_000 motes.
const DEPLOY_GAS_MOTES: u64 = 600_000_000_000;
// 10 CSPR por cada setter call.
const SETTER_GAS_MOTES: u64 = 10_000_000_000;

fn main() {
    // ── Livenet environment ───────────────────────────────────────────────────
    // odra_casper_livenet_env::env() reads ODRA_CASPER_LIVENET_NODE_ADDRESS,
    // ODRA_CASPER_LIVENET_CHAIN_NAME, and ODRA_CASPER_LIVENET_SECRET_KEY_PATH
    // from the environment / .env file automatically.
    let env = odra_casper_livenet_env::env();

    // ── Read OhuVault init params from environment ────────────────────────────

    // Account hashes (formatted as "account-hash-<hex>").
    let admin_hash =
        std::env::var("OHUVAULT_ADMIN_ACCOUNT_HASH").expect("Missing OHUVAULT_ADMIN_ACCOUNT_HASH");
    let operator_hash =
        std::env::var("OHUVAULT_OPERATOR_ACCOUNT_HASH").expect("Missing OHUVAULT_OPERATOR_ACCOUNT_HASH");
    let approver_hashes_raw =
        std::env::var("OHUVAULT_APPROVER_ACCOUNT_HASHES").expect("Missing OHUVAULT_APPROVER_ACCOUNT_HASHES");

    // Scalar params.
    let required_approvals: u8 = std::env::var("OHUVAULT_REQUIRED_APPROVALS")
        .expect("Missing OHUVAULT_REQUIRED_APPROVALS")
        .parse()
        .expect("OHUVAULT_REQUIRED_APPROVALS must be u8");

    let micropayment_cap = U512::from_dec_str(
        &std::env::var("OHUVAULT_MICROPAYMENT_CAP_MOTES")
            .expect("Missing OHUVAULT_MICROPAYMENT_CAP_MOTES"),
    )
    .expect("OHUVAULT_MICROPAYMENT_CAP_MOTES invalid (must be decimal motes)");

    let epoch_cap = U512::from_dec_str(
        &std::env::var("OHUVAULT_EPOCH_CAP_MOTES")
            .expect("Missing OHUVAULT_EPOCH_CAP_MOTES"),
    )
    .expect("OHUVAULT_EPOCH_CAP_MOTES invalid (must be decimal motes)");

    let epoch_window_ms: u64 = std::env::var("OHUVAULT_EPOCH_WINDOW_MS")
        .expect("Missing OHUVAULT_EPOCH_WINDOW_MS")
        .parse()
        .expect("OHUVAULT_EPOCH_WINDOW_MS must be u64");

    let chain_id: u64 = std::env::var("OHUVAULT_CHAIN_ID")
        .expect("Missing OHUVAULT_CHAIN_ID")
        .parse()
        .expect("OHUVAULT_CHAIN_ID must be u64");

    let quorum_fail_bps: u64 = std::env::var("OHUVAULT_QUORUM_FAIL_BPS")
        .expect("Missing OHUVAULT_QUORUM_FAIL_BPS")
        .parse()
        .expect("OHUVAULT_QUORUM_FAIL_BPS must be u64");

    let attestation_window_ms: u64 = std::env::var("OHUVAULT_ATTESTATION_WINDOW_MS")
        .expect("Missing OHUVAULT_ATTESTATION_WINDOW_MS")
        .parse()
        .expect("OHUVAULT_ATTESTATION_WINDOW_MS must be u64");

    // ── Read MutualPool / setter params ──────────────────────────────────────

    let premium_bps: u64 = std::env::var("OHUVAULT_PREMIUM_BPS")
        .expect("Missing OHUVAULT_PREMIUM_BPS")
        .parse()
        .expect("OHUVAULT_PREMIUM_BPS must be u64");

    let indemnity_target_bps: u64 = std::env::var("OHUVAULT_INDEMNITY_TARGET_BPS")
        .expect("Missing OHUVAULT_INDEMNITY_TARGET_BPS")
        .parse()
        .expect("OHUVAULT_INDEMNITY_TARGET_BPS must be u64");

    // ── Parse addresses ───────────────────────────────────────────────────────
    // TODO(verify): confirm Address::from_str properly parses "account-hash-<hex>"
    // format. The Odra example uses it for "hash-<hex>"; account hashes follow
    // the same Display/FromStr convention in casper-types.
    let admin: Address =
        Address::from_str(&admin_hash).expect("Invalid OHUVAULT_ADMIN_ACCOUNT_HASH");
    let operator: Address =
        Address::from_str(&operator_hash).expect("Invalid OHUVAULT_OPERATOR_ACCOUNT_HASH");
    let approvers: Vec<Address> = approver_hashes_raw
        .split_whitespace()
        .map(|s| Address::from_str(s).expect("Invalid approver account hash"))
        .collect();

    // ── Build init args (EXACT order matching OhuVault::init signature) ──────
    let vault_init_args = OhuVaultInitArgs {
        admin,
        operator,
        approvers,
        required_approvals,
        micropayment_cap,
        epoch_cap,
        epoch_window_ms,
        chain_id,
        quorum_fail_bps,
        attestation_window_ms,
    };

    // ── Deploy OhuVault ──────────────────────────────────────────────────────
    env.set_gas(DEPLOY_GAS_MOTES);

    println!("Deploying OhuVault to Casper Testnet...");
    println!("  Gas budget: {} motes (~{} CSPR)", DEPLOY_GAS_MOTES, DEPLOY_GAS_MOTES / 1_000_000_000);
    // TODO(verify): confirm deploy signature — Odra example shows
    // ModuleName::deploy(&env, init_args) returning HostRef.
    let mut vault: OhuVaultHostRef = OhuVault::deploy(&env, vault_init_args);

    let vault_addr = vault.address();
    println!("OhuVault deployed successfully!");
    println!("  Contract address: {}", vault_addr.to_string());

    // ── Deploy MutualPool ────────────────────────────────────────────────────
    env.set_gas(DEPLOY_GAS_MOTES);

    println!("Deploying MutualPool to Casper Testnet...");
    println!("  Gas budget: {} motes (~{} CSPR)", DEPLOY_GAS_MOTES, DEPLOY_GAS_MOTES / 1_000_000_000);
    // TODO(verify): confirm MutualPool::deploy takes &env + MutualPoolInitArgs.
    let pool_init_args = MutualPoolInitArgs {
        admin,
        authorized_vault: vault_addr,
    };
    let pool: MutualPoolHostRef = MutualPool::deploy(&env, pool_init_args);

    let pool_addr = pool.address();
    println!("MutualPool deployed successfully!");
    println!("  Contract address: {}", pool_addr.to_string());

    // ── Setters (caller == admin) ────────────────────────────────────────────
    // El admin debe tener su clave cargada como ODRA_CASPER_LIVENET_KEY_1
    // (u otro índice) y estar fondeado con gas.
    // TODO(verify): confirm env.set_caller() with an Address works across
    // accounts in livenet env (the key for that account must be loaded).
    env.set_caller(admin);
    env.set_gas(SETTER_GAS_MOTES);

    println!("Calling setters as admin...");

    vault.set_mutual_pool(pool_addr);
    println!("  set_mutual_pool({})  ok", pool_addr.to_string());

    vault.set_premium_bps(premium_bps);
    println!("  set_premium_bps({})   ok", premium_bps);

    vault.set_indemnity_target_bps(indemnity_target_bps);
    println!("  set_indemnity_target_bps({})  ok", indemnity_target_bps);

    // ── Output ────────────────────────────────────────────────────────────────
    println!();
    println!("=== Deploy complete ===");
    println!("OhuVault package hash:     {}", vault_addr.to_string());
    println!("MutualPool package hash:   {}", pool_addr.to_string());
    println!();
    println!("  View on-chain:");
    println!("    https://testnet.cspr.live/contract-package/{}", vault_addr.to_string());
    println!("    https://testnet.cspr.live/contract-package/{}", pool_addr.to_string());
    println!("  (replace the hash segment with the contract-package hash after deployment)");
}

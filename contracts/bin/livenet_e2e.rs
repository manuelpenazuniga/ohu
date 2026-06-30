//! W1-3 Fase 2 — Lote feliz E2E de OhuVault contra Casper Testnet (Odra Livenet).
//!
//! Corre el ciclo completo de un lote, cada paso firmado por la cuenta del rol
//! correspondiente (multi-key via env.set_caller + get_account):
//!   open_lote(admin) -> deposit_to_lote(buyer,$) -> post_bond(producer,$) [FUNDED]
//!   -> propose_release(approver0) -> approve_release x2 -> release_to_producer(admin) [SETTLED_OK]
//!
//! Cada llamada que no revierte PRUEBA la transición de estado (p.ej. post_bond
//! solo pasa en OPEN; release solo en FUNDED con approvals>=required). La prueba
//! final: el balance del producer sube en (share + bond).
//!
//! Claves cargadas por índice (ver .env):
//!   0 = ODRA_CASPER_LIVENET_SECRET_KEY_PATH (deployer, actúa de BUYER)
//!   1 = ODRA_CASPER_LIVENET_KEY_1 (admin)      2 = KEY_2 (approver0)
//!   3 = ODRA_CASPER_LIVENET_KEY_3 (approver1)  4 = KEY_4 (producer, no privilegiado)
//!
//! Uso (tras fondear las cuentas con gas):
//!   set -a && source ../.env && set +a
//!   cargo run --bin ohu_livenet_e2e --features livenet
//!
//! El package hash del contrato desplegado se pasa por env OHUVAULT_PACKAGE_HASH
//! (ver infra/deployments/testnet.md).

use odra::casper_types::U512;
use odra::host::{HostEnv, HostRef, HostRefLoader};
use odra::prelude::*;
use std::str::FromStr;

use ohu_contracts::ohu_vault::{OhuVault, OhuVaultHostRef};

const LOTE_ID: u64 = 1;
const SHARE_MOTES: u64 = 10_000_000_000; // 10 CSPR — share del comprador
const BOND_MOTES: u64 = 5_000_000_000; //  5 CSPR — bono del productor
const CALL_GAS: u64 = 10_000_000_000; // 10 CSPR techo de gas por llamada

fn step(env: &HostEnv, caller: Address, label: &str) {
    env.set_caller(caller);
    env.set_gas(CALL_GAS);
    println!("→ {label}  (caller {caller:?})");
}

fn main() {
    let env = odra_casper_livenet_env::env();

    let buyer = env.get_account(0);
    let admin = env.get_account(1);
    let approver0 = env.get_account(2);
    let approver1 = env.get_account(3);
    let producer = env.get_account(4);

    let pkg = std::env::var("OHUVAULT_PACKAGE_HASH").expect("Missing OHUVAULT_PACKAGE_HASH");
    let address = Address::from_str(&pkg).expect("OHUVAULT_PACKAGE_HASH inválido");
    let mut contract: OhuVaultHostRef = OhuVault::load(&env, address);

    println!("== Lote E2E (id={LOTE_ID}) sobre {pkg} ==");
    let bal_before = env.balance_of(&producer);
    println!("producer balance inicial: {bal_before}");

    // 1. open_lote — admin registra el lote y su productor.
    step(&env, admin, "open_lote");
    contract.open_lote(LOTE_ID, producer);

    // 2. deposit_to_lote — el comprador deposita su share (payable, earmarked INV-7).
    step(&env, buyer, "deposit_to_lote (share)");
    contract.with_tokens(U512::from(SHARE_MOTES)).deposit_to_lote(LOTE_ID);

    // 3. post_bond — el productor deposita el bono (payable) -> transición a FUNDED.
    step(&env, producer, "post_bond (bono) -> FUNDED");
    contract.with_tokens(U512::from(BOND_MOTES)).post_bond(LOTE_ID);

    // 4. propose_release — un approver propone la liberación (lote en FUNDED).
    step(&env, approver0, "propose_release");
    contract.propose_release(LOTE_ID);

    // 5. approve_release ×2 — M-de-N on-chain (required_approvals = 2).
    step(&env, approver0, "approve_release #1");
    contract.approve_release(LOTE_ID);
    step(&env, approver1, "approve_release #2");
    contract.approve_release(LOTE_ID);

    // 6. release_to_producer — admin ejecuta (caller==admin ∧ approvals>=2) -> SETTLED_OK.
    step(&env, admin, "release_to_producer -> SETTLED_OK");
    contract.release_to_producer(LOTE_ID);

    let bal_after = env.balance_of(&producer);
    let expected = U512::from(SHARE_MOTES + BOND_MOTES);
    println!("\nproducer balance final: {bal_after}");
    println!("delta esperado (share+bond): {expected}");
    println!("\n✅ E2E COMPLETO: lote {LOTE_ID} liquidado (SETTLED_OK), escrow liberado al producer.");
}

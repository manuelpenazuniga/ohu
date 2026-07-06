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

const LOTE_ID: u64 = 2; // el lote 1 quedó SETTLED_FAIL en el E2E del fallo
const SHARE_MOTES: u64 = 10_000_000_000; // 10 CSPR — share del comprador
const BOND_MOTES: u64 = 10_000_000_000; // 10 CSPR — bono >= target (8 CSPR con indemnity_bps=8000)
const CALL_GAS: u64 = 10_000_000_000; // 10 CSPR techo de gas por llamada

fn step(env: &HostEnv, caller: Address, label: &str) {
    env.set_caller(caller);
    env.set_gas(CALL_GAS);
    println!("→ {label}  (caller {caller:?})");
}

fn main() {
    let env = odra_casper_livenet_env::env();

    // Camino feliz vía tally (W2-4): NO usa approvers (el M-de-N quedó vestigial).
    let buyer = env.get_account(0);
    let admin = env.get_account(1);
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

    // 3. post_bond — el productor deposita el bono (payable). Sigue en OPEN.
    step(&env, producer, "post_bond (bono) -> sigue OPEN");
    contract.with_tokens(U512::from(BOND_MOTES)).post_bond(LOTE_ID);

    // 4. lock_lote — admin cierra la ventana de fondeo -> FUNDED (W2-4).
    step(&env, admin, "lock_lote -> FUNDED");
    contract.lock_lote(LOTE_ID);
    println!("   lote_state = {} (FUNDED)", contract.lote_state(LOTE_ID));

    // 5. Esperar el cierre de la ventana de atestación. CAMINO FELIZ: NADIE atesta
    //    negativo -> silencio = recibido -> tally negativo = 0 -> EVAL_OK.
    let window_ms: u64 = std::env::var("OHUVAULT_ATTESTATION_WINDOW_MS")
        .expect("Missing OHUVAULT_ATTESTATION_WINDOW_MS")
        .parse()
        .expect("OHUVAULT_ATTESTATION_WINDOW_MS must be u64");
    let sleep_ms = window_ms + 10_000;
    println!("\nSin atestaciones negativas (silencio=recibido). Esperando {sleep_ms} ms a que cierre la ventana...");
    std::thread::sleep(std::time::Duration::from_millis(sleep_ms));

    // 6. evaluate_lote — disparador PARAMÉTRICO: silencio -> EVAL_OK (INV-2: lo autoriza
    //    el tally, no un humano ni M-de-N).
    step(&env, admin, "evaluate_lote -> EVAL_OK");
    contract.evaluate_lote(LOTE_ID);
    println!("   lote_state = {} (esperado 4 = EVAL_OK)", contract.lote_state(LOTE_ID));

    // 7. release_to_producer — admin ejecuta (state==EVAL_OK) -> SETTLED_OK (+ prima al pool).
    step(&env, admin, "release_to_producer -> SETTLED_OK");
    contract.release_to_producer(LOTE_ID);

    let bal_after = env.balance_of(&producer);
    println!("\nproducer balance final: {bal_after}");
    println!("delta bruto esperado: share+bond - prima = {} - {} (premium_bps) motes",
        SHARE_MOTES + BOND_MOTES, "0.5%·funded");
    println!("\n✅ E2E FELIZ (vía tally) COMPLETO: lote {LOTE_ID} liquidado (SETTLED_OK), escrow al producer, prima al MutualPool.");
}

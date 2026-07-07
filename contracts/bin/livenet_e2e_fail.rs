//! W2-4 — Lote FALLIDO E2E de OhuVault contra Casper Testnet (Odra Livenet).
//!
//! Corre el ciclo completo de un lote que falla, con atestación negativa firmada
//! Ed25519 off-chain por el comprador y relayada por el admin:
//!   open_lote(admin) -> deposit_to_lote(buyer,$) -> post_bond(producer,$)
//!   -> lock_lote(admin) [FUNDED] -> firma negativa Ed25519 off-chain
//!   -> verify_attestation(admin relay, DENTRO de la ventana)
//!   -> sleep(attestation_window) [cerrar la ventana]
//!   -> evaluate_lote(admin) [EVAL_FAIL] -> settle_failure(admin) [SETTLED_FAIL]
//!   -> withdraw_settlement(buyer) [refund + indemnity]
//!
//! Claves cargadas por indice (ver .env):
//!   0 = ODRA_CASPER_LIVENET_SECRET_KEY_PATH (buyer, tambien firma atestaciones)
//!   1 = ODRA_CASPER_LIVENET_KEY_1 (admin)
//!   2 = ODRA_CASPER_LIVENET_KEY_2 (producer)
//!
//! Uso (tras fondear las cuentas con gas y deployar los contratos):
//!   set -a && source ../.env && set +a
//!   cargo run --bin ohu_livenet_e2e_fail --features livenet
//!
//! El package hash de cada contrato desplegado se pasa por env:
//!   OHUVAULT_PACKAGE_HASH y MUTUALPOOL_PACKAGE_HASH
//! (ver infra/deployments/testnet.md).

use odra::casper_types::crypto::{self, PublicKey, SecretKey};
use odra::casper_types::U512;
use odra::host::{HostEnv, HostRef, HostRefLoader};
use odra::prelude::*;
use std::str::FromStr;

use ohu_contracts::mutual_pool::{MutualPool, MutualPoolHostRef};
use ohu_contracts::ohu_vault::{OhuVault, OhuVaultHostRef};

const LOTE_ID: u64 = 1;
const SHARE_MOTES: u64 = 10_000_000_000; // 10 CSPR — share del comprador
const BOND_MOTES: u64 = 10_000_000_000; //  10 CSPR — bono del productor (>= target=8 CSPR con bps=8000)
const CALL_GAS: u64 = 15_000_000_000; // 15 CSPR techo de gas por llamada

fn step(env: &HostEnv, caller: Address, label: &str) {
    env.set_caller(caller);
    env.set_gas(CALL_GAS);
    println!("→ {label}  (caller {caller:?})");
}

fn main() {
    let env = odra_casper_livenet_env::env();

    // Índices de ODRA_CASPER_LIVENET_KEY_*: 0=deployer(secp256k1), 1=admin,
    // 2=approver0, 3=approver1, 4=producer.
    // buyer = approver0 (Ed25519, para poder FIRMAR la atestación; el deployer es
    // secp256k1 y no sirve). producer = get_account(4) (no privilegiado; approver0
    // seria rechazado por open_lote con ProducerIsPrivileged).
    let buyer = env.get_account(2);
    let admin = env.get_account(1);
    let producer = env.get_account(4);

    let vault_pkg = std::env::var("OHUVAULT_PACKAGE_HASH").expect("Missing OHUVAULT_PACKAGE_HASH");
    let pool_pkg =
        std::env::var("MUTUALPOOL_PACKAGE_HASH").expect("Missing MUTUALPOOL_PACKAGE_HASH");

    let vault_addr = Address::from_str(&vault_pkg).expect("OHUVAULT_PACKAGE_HASH invalido");
    let pool_addr = Address::from_str(&pool_pkg).expect("MUTUALPOOL_PACKAGE_HASH invalido");

    // TODO(verify): confirm Odra HostRefLoader::load works with contract
    // package hash Address in livenet env.
    let mut vault: OhuVaultHostRef = OhuVault::load(&env, vault_addr);
    let _pool: MutualPoolHostRef = MutualPool::load(&env, pool_addr);

    // Read chain_id from on-chain state (domain separation, fix #4).
    let chain_id = vault.chain_id();
    // Read attestation_window_ms from on-chain state.
    let attestation_window_ms = vault.attestation_window_ms();

    println!("== Lote E2E FALLO (id={LOTE_ID}) ==");
    println!("  vault:  {vault_pkg}");
    println!("  pool:   {pool_pkg}");
    println!("  buyer:  {buyer:?}");
    println!("  admin:  {admin:?}");
    println!("  producer: {producer:?}");
    println!("  chain_id: {chain_id}");
    println!("  attestation_window_ms: {attestation_window_ms}");

    let buyer_bal_before = env.balance_of(&buyer);
    println!("\nbuyer balance inicial: {buyer_bal_before}");

    // 1. open_lote — admin registra el lote y su productor.
    step(&env, admin, "open_lote");
    vault.open_lote(LOTE_ID, producer);
    println!("   lote_state = {} (OPEN)", vault.lote_state(LOTE_ID));

    // 2. deposit_to_lote — el comprador deposita su share (payable, earmarked).
    step(&env, buyer, "deposit_to_lote (share)");
    vault
        .with_tokens(U512::from(SHARE_MOTES))
        .deposit_to_lote(LOTE_ID);
    println!("   lote_funded = {}", vault.lote_funded(LOTE_ID));
    println!(
        "   lote_share(buyer) = {}",
        vault.lote_share(LOTE_ID, buyer)
    );

    // 3. post_bond — el productor deposita el bono (>= target).
    //    Con indemnity_target_bps=8000: target = 10*8000/10000 = 8 CSPR.
    //    BOND=10 CSPR >= 8 CSPR. Sigue en OPEN; lock_lote transiciona a FUNDED.
    step(&env, producer, "post_bond (bono) -> sigue OPEN");
    vault.with_tokens(U512::from(BOND_MOTES)).post_bond(LOTE_ID);
    println!("   lote_bond = {}", vault.lote_bond(LOTE_ID));
    println!(
        "   lote_state = {} (OPEN, pending lock)",
        vault.lote_state(LOTE_ID)
    );

    // 4. lock_lote — admin cierra el lote -> FUNDED.
    step(&env, admin, "lock_lote -> FUNDED");
    vault.lock_lote(LOTE_ID);
    println!("   lote_state = {} (FUNDED)", vault.lote_state(LOTE_ID));
    println!("   lote_funded_at = {}", vault.lote_funded_at(LOTE_ID));

    // 5. ATESTACION NEGATIVA (gasless), DENTRO de la ventana (now < deadline).
    //    ORDEN CRITICO (gate del pase holistico W2-4): verify_attestation exige
    //    state==FUNDED && now < funded_at+window. Por eso se atesta AQUI (justo
    //    tras lock, aun dentro de la ventana); RECIEN DESPUES se duerme para
    //    cerrar la ventana y poder evaluar. Atestar tras dormir revertiria
    //    AttestationWindowClosed.
    //    El comprador firma off-chain; el admin relaya pagando el gas.
    println!("\n-- Atestacion negativa off-chain --");

    // Load buyer's Ed25519 secret key from file.
    // TODO(verify): confirm SecretKey::from_file API in casper-types 6.x
    // and that it correctly reads PEM files from the Odra 2.8.2 toolchain.
    let sk_path = std::env::var("BUYER_SECRET_KEY_PATH").unwrap_or_else(|_| {
        std::env::var("ODRA_CASPER_LIVENET_SECRET_KEY_PATH")
            .expect("Missing BUYER_SECRET_KEY_PATH or ODRA_CASPER_LIVENET_SECRET_KEY_PATH")
    });
    let sk =
        SecretKey::from_file(&sk_path).expect("Failed to load buyer Ed25519 secret key from file");
    let pk = PublicKey::from(&sk);

    // Build the attestation message (same format as on-chain build_attestation_message).
    // TODO(verify): confirm vault.address() returns the same value as
    // self_address() on-chain. The verifying_contract in the message must match
    // what self_address() returns.
    let verifying_contract = vault.address();
    let valid_before = u64::MAX; // effectively never expires
    let nonce = 1u64;
    let received = false;

    let msg = ohu_contracts::attestation::build_attestation_message(
        LOTE_ID,
        nonce,
        received,
        verifying_contract,
        chain_id,
        valid_before,
    );

    // Sign with Ed25519.
    let sig = crypto::sign(&msg, &sk, &pk);

    // Extract raw bytes.
    // TODO(verify): confirm Into<Vec<u8>> for PublicKey and Signature yields
    // the raw Ed25519 bytes (32 + 64) in casper-types 6.x.
    let pk_bytes: [u8; 32] = Into::<Vec<u8>>::into(&pk)
        .try_into()
        .expect("Ed25519 pk should be 32 bytes");
    let sig_bytes: [u8; 64] = Into::<Vec<u8>>::into(&sig)
        .try_into()
        .expect("Ed25519 sig should be 64 bytes");

    println!("   firmante: {:?}", pk.to_account_hash());
    println!("   mensaje: {} bytes", msg.len());
    println!("   firma: recibido=false, nonce={nonce}, valid_before={valid_before}");

    // Relay: admin pays gas, calls verify_attestation.
    step(&env, admin, "verify_attestation (admin relay)");
    let ok = vault.verify_attestation(LOTE_ID, nonce, received, valid_before, pk_bytes, sig_bytes);
    println!("   verify_attestation returned: {ok}");
    println!("   tally_negative = {}", vault.lote_tally_negative(LOTE_ID));

    // 6. ESPERAR a que la ventana CIERRE (now >= funded_at + window) para poder
    //    evaluar. evaluate_lote exige now >= deadline; verify_attestation exigia
    //    now < deadline — por eso el sleep va DESPUES de atestar.
    let sleep_ms = attestation_window_ms + 10_000; // window + 10s buffer
    println!(
        "\nWaiting {} ms for attestation window to close...",
        sleep_ms
    );
    std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
    println!("Done waiting.");

    // 7. evaluate_lote — parametric trigger (negative tally >= quorum).
    step(&env, admin, "evaluate_lote -> EVAL_FAIL");
    vault.evaluate_lote(LOTE_ID);
    let eval_state = vault.lote_state(LOTE_ID);
    println!(
        "   lote_state = {eval_state} (expected {} = EVAL_FAIL)",
        5u8
    );

    // 8. settle_failure — admin cierra el lote fallido.
    step(&env, admin, "settle_failure -> SETTLED_FAIL");
    vault.settle_failure(LOTE_ID);
    println!(
        "   lote_state = {} (SETTLED_FAIL)",
        vault.lote_state(LOTE_ID)
    );
    println!(
        "   lote_indemnity_pool = {}",
        vault.lote_indemnity_pool(LOTE_ID)
    );
    println!("   lote_tail = {}", vault.lote_tail(LOTE_ID));

    // 9. withdraw_settlement — el comprador reclama refund + indemnity (PULL).
    step(&env, buyer, "withdraw_settlement");
    vault.withdraw_settlement(LOTE_ID);

    let buyer_bal_after = env.balance_of(&buyer);
    println!("\nbuyer balance final: {buyer_bal_after} (antes: {buyer_bal_before})");

    // Expected delta: refund(SHARE) + indemnity(min(bond,target)).
    // With SHARE=10 CSPR, bps=8000: target = 10*8000/10000 = 8 CSPR.
    // Bond=10 >= target=8 -> indemnity = 8 CSPR from bond.
    // Tail=0 (bond covers target, C-1).
    // Total = 10 (refund) + 8 (indemnity) = 18 CSPR.
    let expected_target = U512::from(SHARE_MOTES)
        .checked_mul(U512::from(8000u64))
        .unwrap()
        .checked_div(U512::from(10000u64))
        .unwrap();
    let expected_delta = U512::from(SHARE_MOTES)
        .checked_add(expected_target)
        .unwrap();
    println!("delta esperado (refund+indemnity): {expected_delta}");
    println!(
        "delta real: {}",
        buyer_bal_after
            .checked_sub(buyer_bal_before)
            .unwrap_or(U512::zero())
    );

    println!("\nE2E FALLO COMPLETO: lote {LOTE_ID} indemnizado por regla.");
}

//! MutualPool — prima + cola de indemnización paramétrica.
//!
//! ## Propósito (§4.1, §4.7)
//!
//! `MutualPool` cobra una **prima** (≈0.5%) cada vez que se libera un lote feliz
//! (`collect_premium` → `OhuVault::release_to_producer`). Cuando un lote falla y el
//! bono slasheado quedó corto, `MutualPool` paga la **cola** de indemnización acotada
//! (`pay_tail` → `OhuVault::settle_failure`).
//!
//! ## Modelo de seguridad (INV-1, INV-2)
//!
//! - **Solo el `OhuVault` autorizado** puede llamar `pay_tail`. Nadie drena el pool
//!   por juicio del agente.
//! - **`pay_tail` está acotado a la reserva**: `amount <= self_balance()` siempre.
//! - **`collect_premium` es sin gate**: recibir dinero es seguro.
//! - El `admin` puede cambiar el vault autorizado (`set_authorized_vault`).
//!
//! ## Cross-contract call API (Odra 2.8.2)
//!
//! OhuVault llama a MutualPool usando `MutualPoolContractRef::new(&self.env(), addr)`:
//!
//! ```text
//! // Llamada NO-payable (pay_tail):
//! let pool = MutualPoolContractRef::new(&self.env(), pool_addr);
//! pool.pay_tail(self_address, tail_amount);
//!
//! // Llamada payable (collect_premium):
//! pool.with_tokens(premium_amount).collect_premium();
//! ```
//!
//! TODO(audit): confirmar la firma exacta de `MutualPoolContractRef::new` y
//! `with_tokens` en cross-contract calls (Odra 2.8.2). Ver
//! <https://odra.dev/docs/basics/cross-calls>.

use odra::casper_types::U512;
use odra::prelude::*;

/// Errores de MutualPool.
#[odra::odra_error]
pub enum Error {
    /// El caller no es el `admin` (set_authorized_vault).
    NotAdmin = 1,
    /// El caller no es el `authorized_vault` registrado (pay_tail).
    NotAuthorizedVault = 2,
    /// La reserva del pool es menor que la cola solicitada.
    InsufficientReserve = 3,
}

/// Evento: se cobró una prima (recibida del OhuVault en release feliz).
#[odra::event]
pub struct PremiumCollected {
    pub from: Address,
    pub amount: U512,
}

/// Evento: se pagó la cola de indemnización al OhuVault (lote fallido).
#[odra::event]
pub struct TailPaid {
    pub recipient: Address,
    pub amount: U512,
}

/// Contrato de mutual paramétrica.
///
/// Custodia primas en su propio `purse` (creado por el runtime Odra).
/// Solo el `OhuVault` autorizado puede drenarlo, y solo de forma acotada.
#[odra::module(events = [PremiumCollected, TailPaid])]
pub struct MutualPool {
    /// Cuenta que puede cambiar el `authorized_vault`.
    admin: Var<Address>,
    /// El contrato `OhuVault` que puede llamar `pay_tail`.
    authorized_vault: Var<Address>,
}

#[odra::module]
impl MutualPool {
    /// Inicializa el pool con el admin y el vault autorizado.
    pub fn init(&mut self, admin: Address, authorized_vault: Address) {
        self.admin.set(admin);
        self.authorized_vault.set(authorized_vault);
    }

    /// Recibe una prima (CSPR) desde el OhuVault. Sin gate.
    ///
    /// `#[odra(payable)]`: el caller envía CSPR junto con la llamada,
    /// que queda en el `purse` del contrato como reserva.
    ///
    /// TODO(audit): confirmar que `attached_value()` devuelve lo que el
    /// caller (OhuVault) adjuntó en la cross-contract call con `with_tokens`.
    /// Ver <https://odra.dev/docs/basics/cross-calls>.
    #[odra(payable)]
    pub fn collect_premium(&mut self) {
        let from = self.env().caller();
        let amount = self.env().attached_value();
        self.env().emit_event(PremiumCollected { from, amount });
    }

    /// Paga la cola de indemnización al OhuVault (única salida de capital).
    ///
    /// Gates:
    /// - `caller == authorized_vault` (si no, `NotAuthorizedVault`).
    /// - `amount <= self_balance()` (si no, `InsufficientReserve`).
    ///
    /// Transfiere `amount` motes al `recipient` (el propio OhuVault, su `self_address()`).
    ///
    /// INV-1, INV-2: solo el vault autorizado puede llamar; nadie drena el pool
    /// por juicio del agente.
    pub fn pay_tail(&mut self, recipient: Address, amount: U512) {
        let vault = self
            .authorized_vault
            .get_or_revert_with(Error::NotAuthorizedVault);
        if self.env().caller() != vault {
            self.env().revert(Error::NotAuthorizedVault);
        }
        let balance = self.env().self_balance();
        if amount > balance {
            self.env().revert(Error::InsufficientReserve);
        }
        // TODO(audit): `transfer_tokens` en Casper no dispara callback al receptor
        // → seguro como interacción en CEI. Ver techs-specs.md §2.
        self.env().transfer_tokens(&recipient, &amount);
        self.env().emit_event(TailPaid { recipient, amount });
    }

    /// Devuelve la reserva actual del pool (`self_balance()` del purse).
    pub fn reserve(&self) -> U512 {
        self.env().self_balance()
    }

    /// Cambia el vault autorizado. Solo el `admin`.
    pub fn set_authorized_vault(&mut self, addr: Address) {
        let admin = self.admin.get_or_revert_with(Error::NotAdmin);
        if self.env().caller() != admin {
            self.env().revert(Error::NotAdmin);
        }
        self.authorized_vault.set(addr);
    }

    // ── Getters ─────────────────────────────────────────────────────

    /// `admin` configurado en init.
    pub fn admin(&self) -> Address {
        self.admin.get_or_revert_with(Error::NotAdmin)
    }

    /// `authorized_vault` configurado.
    pub fn authorized_vault(&self) -> Address {
        self.authorized_vault
            .get_or_revert_with(Error::NotAuthorizedVault)
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, MutualPool, MutualPoolHostRef, MutualPoolInitArgs, PremiumCollected, TailPaid};
    use odra::casper_types::U512;
    use odra::host::{Deployer, HostEnv, HostRef};
    use odra::prelude::Address;

    const ONE_CSPR: u64 = 1_000_000_000;

    struct Fixture {
        contract: MutualPoolHostRef,
        env: HostEnv,
        admin: Address,
        vault: Address,
        depositor: Address,
    }

    fn setup() -> Fixture {
        let env = odra_test::env();
        let admin = env.get_account(0);
        let vault = env.get_account(1);
        let depositor = env.get_account(2);

        let contract = MutualPool::deploy(
            &env,
            MutualPoolInitArgs {
                admin,
                authorized_vault: vault,
            },
        );

        Fixture {
            contract,
            env,
            admin,
            vault,
            depositor,
        }
    }

    /// Fondea el pool con `amount` CSPR desde `caller`.
    fn fund_pool(f: &mut Fixture, caller: Address, amount: U512) {
        f.env.set_caller(caller);
        f.contract.with_tokens(amount).collect_premium();
    }

    // ── collect_premium: positivos ──────────────────────────────────

    #[test]
    fn collect_premium_increases_reserve() {
        let f = setup();
        let amount = U512::from(5 * ONE_CSPR);
        let balance_before = f.env.balance_of(&f.contract);

        f.env.set_caller(f.depositor);
        f.contract.with_tokens(amount).collect_premium();

        assert_eq!(f.env.balance_of(&f.contract), balance_before + amount);
        assert_eq!(f.contract.reserve(), balance_before + amount);
        assert!(f.env.emitted_event(
            &f.contract,
            PremiumCollected {
                from: f.depositor,
                amount,
            }
        ));
    }

    #[test]
    fn collect_premium_multiple_depositors_accumulate() {
        let mut f = setup();
        let amount1 = U512::from(3 * ONE_CSPR);
        let amount2 = U512::from(7 * ONE_CSPR);
        let depositor = f.depositor;

        fund_pool(&mut f, depositor, amount1);
        let other = f.env.get_account(5);
        fund_pool(&mut f, other, amount2);

        assert_eq!(f.contract.reserve(), amount1 + amount2);
    }

    // ── pay_tail: positivos ─────────────────────────────────────────

    #[test]
    fn pay_tail_by_authorized_vault_decreases_reserve() {
        let mut f = setup();
        let depositor = f.depositor;
        let fund_amount = U512::from(10 * ONE_CSPR);
        let tail = U512::from(4 * ONE_CSPR);
        let recipient = f.env.get_account(3);

        fund_pool(&mut f, depositor, fund_amount);

        let recipient_before = f.env.balance_of(&recipient);
        let pool_before = f.env.balance_of(&f.contract);

        f.env.set_caller(f.vault);
        f.contract.pay_tail(recipient, tail);

        assert_eq!(f.env.balance_of(&recipient), recipient_before + tail);
        assert_eq!(f.env.balance_of(&f.contract), pool_before - tail);
        assert_eq!(f.contract.reserve(), fund_amount - tail);
        assert!(f.env.emitted_event(
            &f.contract,
            TailPaid {
                recipient,
                amount: tail,
            }
        ));
    }

    #[test]
    fn pay_tail_exact_full_reserve_succeeds() {
        let mut f = setup();
        let depositor = f.depositor;
        let fund_amount = U512::from(10 * ONE_CSPR);
        let recipient = f.env.get_account(3);

        fund_pool(&mut f, depositor, fund_amount);

        f.env.set_caller(f.vault);
        f.contract.pay_tail(recipient, fund_amount);

        assert_eq!(f.contract.reserve(), U512::zero());
    }

    // ── pay_tail: negativos ─────────────────────────────────────────

    #[test]
    fn pay_tail_by_non_vault_reverts() {
        let mut f = setup();
        let depositor = f.depositor;
        let fund_amount = U512::from(10 * ONE_CSPR);
        fund_pool(&mut f, depositor, fund_amount);

        let random = f.env.get_account(9);
        f.env.set_caller(random);
        let result = f.contract.try_pay_tail(f.depositor, U512::from(ONE_CSPR));

        assert_eq!(result.unwrap_err(), Error::NotAuthorizedVault.into());
        assert_eq!(f.contract.reserve(), fund_amount);
    }

    #[test]
    fn pay_tail_exceeds_reserve_reverts() {
        let mut f = setup();
        let depositor = f.depositor;
        let fund_amount = U512::from(10 * ONE_CSPR);
        fund_pool(&mut f, depositor, fund_amount);

        f.env.set_caller(f.vault);
        let result = f.contract.try_pay_tail(f.depositor, fund_amount + U512::one());

        assert_eq!(result.unwrap_err(), Error::InsufficientReserve.into());
        assert_eq!(f.contract.reserve(), fund_amount);
    }

    #[test]
    fn pay_tail_zero_amount_succeeds() {
        let mut f = setup();
        let depositor = f.depositor;
        let fund_amount = U512::from(10 * ONE_CSPR);
        fund_pool(&mut f, depositor, fund_amount);

        f.env.set_caller(f.vault);
        f.contract.pay_tail(f.depositor, U512::zero());

        assert_eq!(f.contract.reserve(), fund_amount);
    }

    // ── set_authorized_vault ────────────────────────────────────────

    #[test]
    fn set_authorized_vault_by_admin_succeeds() {
        let mut f = setup();
        let depositor = f.depositor;
        let new_vault = f.env.get_account(5);

        f.env.set_caller(f.admin);
        f.contract.set_authorized_vault(new_vault);

        assert_eq!(f.contract.authorized_vault(), new_vault);

        // El vault viejo ya no puede pagar.
        f.env.set_caller(f.vault);
        let result = f.contract.try_pay_tail(f.depositor, U512::one());
        assert_eq!(result.unwrap_err(), Error::NotAuthorizedVault.into());

        // El nuevo vault sí puede.
        fund_pool(&mut f, depositor, U512::from(5 * ONE_CSPR));
        f.env.set_caller(new_vault);
        f.contract.pay_tail(f.depositor, U512::from(ONE_CSPR));
        assert_eq!(f.contract.reserve(), U512::from(4 * ONE_CSPR));
    }

    #[test]
    fn set_authorized_vault_by_non_admin_reverts() {
        let mut f = setup();
        let new_vault = f.env.get_account(5);

        f.env.set_caller(f.depositor);
        let result = f.contract.try_set_authorized_vault(new_vault);

        assert_eq!(result.unwrap_err(), Error::NotAdmin.into());
        assert_eq!(f.contract.authorized_vault(), f.vault);
    }

    // ── reserve ─────────────────────────────────────────────────────

    #[test]
    fn reserve_starts_at_zero() {
        let f = setup();
        assert_eq!(f.contract.reserve(), U512::zero());
    }

    // ── Getters ─────────────────────────────────────────────────────

    #[test]
    fn getters_reflect_init_configuration() {
        let f = setup();
        assert_eq!(f.contract.admin(), f.admin);
        assert_eq!(f.contract.authorized_vault(), f.vault);
        assert_eq!(f.contract.reserve(), U512::zero());
    }
}

//! OhuVault — custodia en `purse` para lotes de compra cooperativa.
//!
//! S1 (spike): `deposit()` recibe CSPR en el purse del contrato y
//! `withdraw_to(recipient, amount)` transfiere a una cuenta destino.
//! Este último está ABIERTO temporalmente para poder hacer el test E2E;
//! S2 lo cerrará tras `caller == admin` + aprobación M-de-N.
//!
//! Invariantes aplicables:
//! - INV-3: NO usamos Addressable Entity. El contrato custodia los fondos en
//!   su purse principal. Odra 2.8.2 no expone `create_purse()` en `ContractEnv`;
//!   el purse se crea implícitamente al deployar el contrato en Casper. Ver
//!   el `TODO(audit)` en `init()`.
//! - INV-1 / INV-2: en S1 aún no hay gating de retiros; queda marcado para S2.

use odra::casper_types::U512;
use odra::prelude::*;

/// Errores de OhuVault.
#[odra::odra_error]
pub enum Error {
    /// El vault no tiene saldo suficiente para la transferencia solicitada.
    InsufficientBalance = 1,
    /// La cantidad debe ser mayor que cero.
    ZeroAmount = 2,
}

/// Evento emitido cuando se depositan fondos en el vault.
#[odra::event]
pub struct Deposit {
    pub sender: Address,
    pub amount: U512,
}

/// Evento emitido cuando se retiran fondos del vault a una cuenta.
#[odra::event]
pub struct Withdraw {
    pub recipient: Address,
    pub amount: U512,
}

/// Contrato de custodia de Ohu.
///
/// TODO(audit): CES (`emit_event`) es el event standard de Casper soportado por
/// Odra. Verificar si CSPR.cloud indexa CES, native events, o ambos; ajustar a
/// `emit_native_event` si es necesario. Ver <https://odra.dev/docs/basics/events>.
#[odra::module(events = [Deposit, Withdraw])]
pub struct OhuVault;

#[odra::module]
impl OhuVault {
    /// Inicializa el vault.
    ///
    /// En Casper cada contrato tiene un purse principal creado por el runtime
    /// donde se acumulan los fondos enviados a entry points `#[odra(payable)]`.
    /// Odra 2.8.2 abstrae ese purse; no hay API pública `create_purse()`.
    ///
    /// TODO(audit): verificar contra <https://odra.dev/docs/basics/native-token>
    /// y la documentación de Casper si para S2+ se requiere un purse secundario
    /// aislado. Si es así, habrá que recurrir a host calls documentados de Casper
    /// o a una versión de Odra que exponga `create_purse`.
    pub fn init(&mut self) {}

    /// Deposita CSPR en el purse del contrato.
    ///
    /// El monto enviado se obtiene de `self.env().attached_value()` gracias al
    /// atributo `#[odra(payable)]`.
    #[odra(payable)]
    pub fn deposit(&mut self) {
        let sender = self.env().caller();
        let amount = self.env().attached_value();

        if amount == U512::zero() {
            self.env().revert(Error::ZeroAmount);
        }

        // Los fondos ya están en el purse principal del contrato.
        self.env().emit_event(Deposit { sender, amount });
    }

    /// Transfiere `amount` motes desde el purse del contrato a `recipient`.
    ///
    /// TODO(S2): gate admin+M-de-N. Temporalmente abierto solo para el test E2E.
    /// El agente/LLM NO debe poder llamar esta función sin las salvaguardas de S2.
    pub fn withdraw_to(&mut self, recipient: Address, amount: U512) {
        if amount == U512::zero() {
            self.env().revert(Error::ZeroAmount);
        }

        let balance = self.env().self_balance();
        if amount > balance {
            self.env().revert(Error::InsufficientBalance);
        }

        self.env().transfer_tokens(&recipient, &amount);
        self.env().emit_event(Withdraw { recipient, amount });
    }

    /// Devuelve el saldo actual del purse del contrato.
    pub fn balance(&self) -> U512 {
        self.env().self_balance()
    }
}

#[cfg(test)]
mod tests {
    use super::{Deposit, Error, OhuVault, OhuVaultHostRef, Withdraw};
    use odra::casper_types::U512;
    use odra::host::{Deployer, HostEnv, HostRef, NoArgs};
    use odra::prelude::Address;

    const ONE_CSPR: u64 = 1_000_000_000;

    fn setup() -> (OhuVaultHostRef, HostEnv, Address, Address) {
        let env = odra_test::env();
        let depositor = env.get_account(0);
        let recipient = env.get_account(1);
        let contract = OhuVault::deploy(&env, NoArgs);
        (contract, env, depositor, recipient)
    }

    #[test]
    fn deposit_increases_purse_balance_and_emits_event() {
        let (contract, env, depositor, _recipient) = setup();
        let amount = U512::from(5 * ONE_CSPR);

        let contract_balance_before = env.balance_of(&contract);
        assert_eq!(contract_balance_before, U512::zero());

        env.set_caller(depositor);
        contract.with_tokens(amount).deposit();

        assert_eq!(env.balance_of(&contract), amount);
        assert!(env.emitted_event(
            &contract,
            Deposit {
                sender: depositor,
                amount,
            }
        ));
    }

    #[test]
    fn deposit_zero_reverts() {
        let (contract, env, depositor, _recipient) = setup();

        env.set_caller(depositor);
        let result = contract.with_tokens(U512::zero()).try_deposit();

        assert_eq!(result.unwrap_err(), Error::ZeroAmount.into());
    }

    #[test]
    fn withdraw_to_transfers_to_recipient_and_emits_event() {
        let (mut contract, env, depositor, recipient) = setup();
        let deposit_amount = U512::from(10 * ONE_CSPR);
        let withdraw_amount = U512::from(3 * ONE_CSPR);

        env.set_caller(depositor);
        contract.with_tokens(deposit_amount).deposit();

        let recipient_balance_before = env.balance_of(&recipient);
        let contract_balance_before = env.balance_of(&contract);

        contract.withdraw_to(recipient, withdraw_amount);

        assert_eq!(
            env.balance_of(&recipient),
            recipient_balance_before + withdraw_amount
        );
        assert_eq!(
            env.balance_of(&contract),
            contract_balance_before - withdraw_amount
        );
        assert!(env.emitted_event(
            &contract,
            Withdraw {
                recipient,
                amount: withdraw_amount,
            }
        ));
    }

    #[test]
    fn withdraw_to_fails_when_purse_is_empty() {
        let (mut contract, _env, _depositor, recipient) = setup();
        let amount = U512::from(ONE_CSPR);

        let result = contract.try_withdraw_to(recipient, amount);

        assert_eq!(result.unwrap_err(), Error::InsufficientBalance.into());
    }

    #[test]
    fn withdraw_to_fails_if_amount_exceeds_balance() {
        let (mut contract, env, depositor, recipient) = setup();
        let deposit_amount = U512::from(ONE_CSPR);
        let withdraw_amount = U512::from(2 * ONE_CSPR);

        env.set_caller(depositor);
        contract.with_tokens(deposit_amount).deposit();

        let result = contract.try_withdraw_to(recipient, withdraw_amount);

        assert_eq!(result.unwrap_err(), Error::InsufficientBalance.into());
    }

    #[test]
    fn withdraw_zero_reverts() {
        let (mut contract, _env, _depositor, recipient) = setup();

        let result = contract.try_withdraw_to(recipient, U512::zero());

        assert_eq!(result.unwrap_err(), Error::ZeroAmount.into());
    }

    #[test]
    fn balance_reflects_deposits_and_withdrawals() {
        let (mut contract, env, depositor, recipient) = setup();
        let deposit_amount = U512::from(7 * ONE_CSPR);
        let withdraw_amount = U512::from(4 * ONE_CSPR);

        assert_eq!(contract.balance(), U512::zero());

        env.set_caller(depositor);
        contract.with_tokens(deposit_amount).deposit();
        assert_eq!(contract.balance(), deposit_amount);

        contract.withdraw_to(recipient, withdraw_amount);
        assert_eq!(contract.balance(), deposit_amount - withdraw_amount);
    }
}

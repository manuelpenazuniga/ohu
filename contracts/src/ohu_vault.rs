//! OhuVault — custodia en `purse` para lotes de compra cooperativa.
//!
//! ## S2: modelo de seguridad "el agente no drena" (INV-1)
//!
//! Tres roles on-chain, todos **cuentas** (no Addressable Entity, INV-3):
//!
//! - **`admin`** — cuenta que **ejecuta** los releases grandes (`execute`). En
//!   Testnet es además una cuenta con *associated keys* ponderadas + *deployment
//!   threshold* alto (multisig nativo fuera del contrato; ver
//!   `infra/scripts/setup_admin_account.sh`). Así, para que un `execute` siquiera
//!   se someta on-chain, el deploy del admin necesita co-firma off-chain.
//! - **`operator`** — la cuenta del **agente** (LLM). Solo puede llamar
//!   `route_micropayment`, con **tope por llamada**. No puede proponer, aprobar
//!   ni ejecutar releases.
//! - **`approvers`** — conjunto de M-de-N firmantes que `approve(id)` un release
//!   propuesto. `execute(id)` exige `caller == admin` **+** `approval_count(id)
//!   >= required_approvals` con aprobaciones de firmantes **distintos**.
//!
//! Flujo de release grande (Rail A, transferencia del contrato — NO x402, INV-4):
//! `propose_withdraw` (admin o approver) → `approve` × M approvers distintos →
//! `execute` (admin). Ningún paso mueve capital hasta `execute`, y `execute`
//! está doblemente gateado (caller admin + M-de-N on-chain).
//!
//! ### Defensa en profundidad (dos capas de co-firma)
//! 1. **Capa nativa (off-chain):** la cuenta `admin` es un multisig Casper
//!    (associated keys + threshold). Firmar el deploy de `execute` requiere
//!    co-firma off-chain. Configurada en `infra/scripts/setup_admin_account.sh`.
//! 2. **Capa on-chain (este contrato):** `execute` exige M aprobaciones
//!    **distintas** registradas en el contrato, independientes del threshold
//!    nativo. Sobrevive aunque el admin sea una clave única.
//!
//! Invariantes aplicables: INV-1, INV-2 (la aritmética de aprobaciones es la
//! condición on-chain), INV-3, INV-4. INV-5/INV-6 entran en S3.

use odra::casper_types::U512;
use odra::prelude::*;

/// Errores de OhuVault.
///
/// Los códigos 1–2 vienen de S1; 3+ son de S2.
#[odra::odra_error]
pub enum Error {
    /// El vault no tiene saldo suficiente para la transferencia solicitada.
    InsufficientBalance = 1,
    /// La cantidad debe ser mayor que cero.
    ZeroAmount = 2,
    /// El caller no es el `admin` (entrypoint reservado a admin).
    NotAdmin = 3,
    /// El caller no es el `operator` (entrypoint reservado al agente).
    NotOperator = 4,
    /// El caller no pertenece al conjunto de `approvers`.
    NotApprover = 5,
    /// El monto del micropago supera el tope por llamada (`micropayment_cap`).
    CapExceeded = 6,
    /// No existe una solicitud de retiro con ese `request_id`.
    RequestNotFound = 7,
    /// Este approver ya aprobó esta solicitud (no se cuenta dos veces).
    AlreadyApproved = 8,
    /// La solicitud no reúne las M aprobaciones requeridas.
    InsufficientApprovals = 9,
    /// La solicitud ya fue ejecutada.
    AlreadyExecuted = 10,
    /// Parámetros de inicialización inválidos.
    InvalidSetup = 11,
    /// Estado no inicializado (no debería ocurrir tras `init`).
    NotInitialized = 12,
    /// Una dirección de rol/destino debe ser cuenta, no contrato (INV-3).
    NotAnAccount = 13,
    /// La lista de approvers contiene duplicados.
    DuplicateApprover = 14,
}

/// Evento: fondos depositados en el vault.
#[odra::event]
pub struct Deposit {
    pub sender: Address,
    pub amount: U512,
}

/// Evento: el agente enruta un micropago acotado (INV-1, lado permitido).
#[odra::event]
pub struct MicropaymentRouted {
    pub operator: Address,
    pub recipient: Address,
    pub amount: U512,
}

/// Evento: se propone un release grande (aún no mueve capital).
#[odra::event]
pub struct WithdrawProposed {
    pub id: u64,
    pub proposer: Address,
    pub recipient: Address,
    pub amount: U512,
}

/// Evento: un approver aprueba un release.
#[odra::event]
pub struct WithdrawApproved {
    pub id: u64,
    pub approver: Address,
    pub count: u8,
}

/// Evento: un release grande se ejecuta (transfer on-chain, INV-4 no-x402).
#[odra::event]
pub struct WithdrawExecuted {
    pub id: u64,
    pub recipient: Address,
    pub amount: U512,
}

/// Contrato de custodia de Ohu.
///
/// TODO(audit): CES (`emit_event`) es el event standard soportado por Odra.
/// Verificar si CSPR.cloud indexa CES, native events, o ambos; ajustar a
/// `emit_native_event` si es necesario. Ver <https://odra.dev/docs/basics/events>.
#[odra::module(events = [Deposit, MicropaymentRouted, WithdrawProposed, WithdrawApproved, WithdrawExecuted])]
pub struct OhuVault {
    /// Cuenta que ejecuta releases grandes (`caller == admin` en `execute`).
    admin: Var<Address>,
    /// Cuenta del agente; única que puede llamar `route_micropayment`.
    operator: Var<Address>,
    /// Tope de motes por llamada a `route_micropayment` (INV-1).
    micropayment_cap: Var<U512>,
    /// M: número de aprobaciones **distintas** requeridas para `execute`.
    required_approvals: Var<u8>,
    /// Miembros del conjunto de firmantes M-de-N.
    is_approver: Mapping<Address, bool>,
    /// Contador de solicitudes de retiro (siguiente `request_id`).
    next_request_id: Var<u64>,
    /// Destino de cada solicitud de retiro.
    request_recipient: Mapping<u64, Address>,
    /// Monto de cada solicitud de retiro.
    request_amount: Mapping<u64, U512>,
    /// `true` si la solicitud ya fue ejecutada (anti doble-ejecución).
    request_executed: Mapping<u64, bool>,
    /// Número de aprobaciones **distintas** acumuladas por solicitud.
    approval_count: Mapping<u64, u8>,
    /// `(request_id, approver) -> true`: registro anti doble-aprobación.
    has_approved: Mapping<(u64, Address), bool>,
}

#[odra::module]
impl OhuVault {
    /// Inicializa el vault con el modelo de seguridad de S2.
    ///
    /// Valida (revert `InvalidSetup`/`NotAnAccount`/`DuplicateApprover`):
    /// - `admin` y `operator` son cuentas (no contratos) y distintos entre sí.
    /// - `operator` no está en `approvers` (separación de roles del agente).
    /// - `approvers` no vacío, sin duplicados, todos cuentas.
    /// - `required_approvals` en `[1, approvers.len()]`.
    /// - `micropayment_cap > 0`.
    ///
    /// TODO(audit): verificar contra <https://odra.dev/docs/basics/native-token>
    /// si para S2+ se requiere un purse secundario aislado. El purse principal
    /// (creado por el runtime) basta para este spike.
    pub fn init(
        &mut self,
        admin: Address,
        operator: Address,
        approvers: Vec<Address>,
        required_approvals: u8,
        micropayment_cap: U512,
    ) {
        if admin.is_contract() || operator.is_contract() {
            self.env().revert(Error::NotAnAccount);
        }
        if admin == operator {
            self.env().revert(Error::InvalidSetup);
        }
        if approvers.is_empty() {
            self.env().revert(Error::InvalidSetup);
        }
        if required_approvals == 0 || (required_approvals as usize) > approvers.len() {
            self.env().revert(Error::InvalidSetup);
        }
        if micropayment_cap == U512::zero() {
            self.env().revert(Error::InvalidSetup);
        }

        for i in 0..approvers.len() {
            let a = approvers[i];
            if a.is_contract() {
                self.env().revert(Error::NotAnAccount);
            }
            if a == operator {
                self.env().revert(Error::InvalidSetup);
            }
            for j in (i + 1)..approvers.len() {
                if approvers[i] == approvers[j] {
                    self.env().revert(Error::DuplicateApprover);
                }
            }
            self.is_approver.set(&a, true);
        }

        self.admin.set(admin);
        self.operator.set(operator);
        self.micropayment_cap.set(micropayment_cap);
        self.required_approvals.set(required_approvals);
        self.next_request_id.set(0u64);
    }

    /// Deposita CSPR en el purse del contrato.
    ///
    /// Abierto (cualquiera puede fondear el vault); el control de salida está
    /// en `route_micropayment` (capado) y `propose/approve/execute` (M-de-N).
    #[odra(payable)]
    pub fn deposit(&mut self) {
        let sender = self.env().caller();
        let amount = self.env().attached_value();

        if amount == U512::zero() {
            self.env().revert(Error::ZeroAmount);
        }

        self.env().emit_event(Deposit { sender, amount });
    }

    /// **Entrypoint capado del agente (INV-1).** Enruta un micropago desde el
    /// purse del contrato a `recipient`.
    ///
    /// Gates:
    /// - `caller == operator`.
    /// - `0 < amount <= micropayment_cap` (tope **por llamada**).
    /// - `amount <= self_balance`.
    ///
    /// No hay estado mutable post-transfer que un hipotético callback podría
    /// corromper, y `caller` se re-chequea en cada entrada (un reentrante
    /// tendría como caller al contrato receptor, no al `operator`). Ver nota de
    /// reentrancia en `execute`.
    ///
    /// El tope es **por llamada** (definición de INV-1): no existe un path que
    /// mueva más que `micropayment_cap` en una sola invocación. Un operator
    /// comprometido podría emitir muchas llamadas acotadas; acotar el daño
    /// acumulado es responsabilidad off-chain (cap pequeño + monitoring de
    /// `MicropaymentRouted` en CSPR.cloud), no un invariante de contrato.
    ///
    /// TODO(audit): confirmar contra los docs de Casper que
    /// `transfer_tokens` es un *balance move* sin callback al receptor (sí lo
    /// es en el runtime de Casper). Mientras tanto, `execute` aplica CEI por
    /// defensa en profundidad.
    pub fn route_micropayment(&mut self, recipient: Address, amount: U512) {
        let caller = self.env().caller();
        let operator = self.operator.get_or_revert_with(Error::NotInitialized);
        if caller != operator {
            self.env().revert(Error::NotOperator);
        }
        if amount == U512::zero() {
            self.env().revert(Error::ZeroAmount);
        }
        let cap = self
            .micropayment_cap
            .get_or_revert_with(Error::NotInitialized);
        if amount > cap {
            self.env().revert(Error::CapExceeded);
        }
        let balance = self.env().self_balance();
        if amount > balance {
            self.env().revert(Error::InsufficientBalance);
        }

        self.env().transfer_tokens(&recipient, &amount);
        self.env().emit_event(MicropaymentRouted {
            operator: caller,
            recipient,
            amount,
        });
    }

    /// Propone un release grande. No mueve capital.
    ///
    /// Lo puede llamar `admin` o cualquier `approver` (los roles que gobiernan
    /// los releases), pero **no** el `operator`: el agente no propone retiros.
    /// Devuelve el `request_id` nuevo.
    pub fn propose_withdraw(&mut self, recipient: Address, amount: U512) -> u64 {
        let caller = self.env().caller();
        let admin = self.admin.get_or_revert_with(Error::NotInitialized);
        let caller_is_approver = self.is_approver.get_or_default(&caller);
        if caller != admin && !caller_is_approver {
            self.env().revert(Error::NotApprover);
        }
        if amount == U512::zero() {
            self.env().revert(Error::ZeroAmount);
        }
        if recipient.is_contract() {
            self.env().revert(Error::NotAnAccount);
        }

        let id = self
            .next_request_id
            .get_or_revert_with(Error::NotInitialized);
        self.next_request_id.set(id + 1);

        self.request_recipient.set(&id, recipient);
        self.request_amount.set(&id, amount);
        self.request_executed.set(&id, false);
        self.approval_count.set(&id, 0u8);

        self.env().emit_event(WithdrawProposed {
            id,
            proposer: caller,
            recipient,
            amount,
        });
        id
    }

    /// Aprueba una solicitud de retiro. Solo `approvers`; un mismo approver no
    /// puede aprobar dos veces la misma solicitud (`AlreadyApproved`), lo que
    /// garantiza que `approval_count` cuente **firmantes distintos**.
    pub fn approve(&mut self, request_id: u64) {
        let caller = self.env().caller();
        if !self.is_approver.get_or_default(&caller) {
            self.env().revert(Error::NotApprover);
        }
        if self.request_recipient.get(&request_id).is_none() {
            self.env().revert(Error::RequestNotFound);
        }
        if self.request_executed.get_or_default(&request_id) {
            self.env().revert(Error::AlreadyExecuted);
        }

        let key = (request_id, caller);
        if self.has_approved.get_or_default(&key) {
            self.env().revert(Error::AlreadyApproved);
        }
        self.has_approved.set(&key, true);

        let count = self.approval_count.get_or_default(&request_id) + 1;
        self.approval_count.set(&request_id, count);

        self.env().emit_event(WithdrawApproved {
            id: request_id,
            approver: caller,
            count,
        });
    }

    /// Ejecuta un release grande. **Doble gate (INV-1):**
    /// - `caller == admin`.
    /// - `approval_count(request_id) >= required_approvals` (M aprobaciones
    ///   **distintas**, garantizadas por `has_approved` en `approve`).
    ///
    /// Aplica **checks-effects-interactions**: marca `request_executed = true`
    /// **antes** de la transferencia, de modo que un hipotético reentrante
    /// (callback desde el receptor) encontraría la solicitud ya ejecutada y
    /// revertiría con `AlreadyExecuted`. En Casper `transfer_tokens` no
    /// dispara código en el receptor, pero CEI es defensa en profundidad.
    pub fn execute(&mut self, request_id: u64) {
        let caller = self.env().caller();
        let admin = self.admin.get_or_revert_with(Error::NotInitialized);
        if caller != admin {
            self.env().revert(Error::NotAdmin);
        }

        let recipient = match self.request_recipient.get(&request_id) {
            Some(r) => r,
            None => self.env().revert(Error::RequestNotFound),
        };
        if self.request_executed.get_or_default(&request_id) {
            self.env().revert(Error::AlreadyExecuted);
        }

        let count = self.approval_count.get_or_default(&request_id);
        let required = self
            .required_approvals
            .get_or_revert_with(Error::NotInitialized);
        if count < required {
            self.env().revert(Error::InsufficientApprovals);
        }

        let amount = match self.request_amount.get(&request_id) {
            Some(a) => a,
            None => self.env().revert(Error::NotInitialized),
        };
        let balance = self.env().self_balance();
        if amount > balance {
            self.env().revert(Error::InsufficientBalance);
        }

        // Effects antes que interactions (CEI).
        self.request_executed.set(&request_id, true);
        self.env().transfer_tokens(&recipient, &amount);
        self.env().emit_event(WithdrawExecuted {
            id: request_id,
            recipient,
            amount,
        });
    }

    // -----------------------------------------------------------------
    // Getters de observabilidad (read-only).
    // -----------------------------------------------------------------

    /// Saldo actual del purse del contrato.
    pub fn balance(&self) -> U512 {
        self.env().self_balance()
    }

    /// `admin` configurado.
    pub fn admin(&self) -> Address {
        self.admin.get_or_revert_with(Error::NotInitialized)
    }

    /// `operator` configurado.
    pub fn operator(&self) -> Address {
        self.operator.get_or_revert_with(Error::NotInitialized)
    }

    /// Tope por llamada de `route_micropayment`.
    pub fn micropayment_cap(&self) -> U512 {
        self.micropayment_cap
            .get_or_revert_with(Error::NotInitialized)
    }

    /// M (aprobaciones distintas requeridas).
    pub fn required_approvals(&self) -> u8 {
        self.required_approvals
            .get_or_revert_with(Error::NotInitialized)
    }

    /// ¿`addr` es un approver?
    pub fn is_approver(&self, addr: Address) -> bool {
        self.is_approver.get_or_default(&addr)
    }

    /// Aprobaciones distintas acumuladas para `request_id`.
    pub fn approval_count(&self, request_id: u64) -> u8 {
        self.approval_count.get_or_default(&request_id)
    }

    /// ¿La solicitud fue ejecutada?
    pub fn request_executed(&self, request_id: u64) -> bool {
        self.request_executed.get_or_default(&request_id)
    }

    /// Destino de la solicitud (revert si no existe).
    pub fn request_recipient(&self, request_id: u64) -> Address {
        match self.request_recipient.get(&request_id) {
            Some(r) => r,
            None => self.env().revert(Error::RequestNotFound),
        }
    }

    /// Monto de la solicitud (revert si no existe).
    pub fn request_amount(&self, request_id: u64) -> U512 {
        match self.request_amount.get(&request_id) {
            Some(a) => a,
            None => self.env().revert(Error::RequestNotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Deposit, Error, MicropaymentRouted, OhuVault, OhuVaultHostRef, OhuVaultInitArgs,
        WithdrawApproved, WithdrawExecuted, WithdrawProposed,
    };
    use odra::casper_types::U512;
    use odra::host::{Deployer, HostEnv, HostRef};
    use odra::prelude::Address;

    const ONE_CSPR: u64 = 1_000_000_000;

    /// Fixture: admin=acct0, operator=acct1, approvers=acct2..4 (M=2),
    /// cap=1 CSPR/llamada, depositor=acct5. Vault fondeado con 100 CSPR.
    struct Fixture {
        contract: OhuVaultHostRef,
        env: HostEnv,
        admin: Address,
        operator: Address,
        approver0: Address,
        approver1: Address,
        approver2: Address,
        depositor: Address,
        recipient: Address,
        cap: U512,
    }

    fn setup() -> Fixture {
        setup_with(U512::from(ONE_CSPR), 2)
    }

    fn setup_with(cap: U512, required: u8) -> Fixture {
        let env = odra_test::env();
        let admin = env.get_account(0);
        let operator = env.get_account(1);
        let approver0 = env.get_account(2);
        let approver1 = env.get_account(3);
        let approver2 = env.get_account(4);
        let depositor = env.get_account(5);
        let recipient = env.get_account(6);

        let contract = OhuVault::deploy(
            &env,
            OhuVaultInitArgs {
                admin,
                operator,
                approvers: vec![approver0, approver1, approver2],
                required_approvals: required,
                micropayment_cap: cap,
            },
        );

        // Fondea el vault con 100 CSPR desde el depositor.
        env.set_caller(depositor);
        contract.with_tokens(U512::from(100 * ONE_CSPR)).deposit();

        Fixture {
            contract,
            env,
            admin,
            operator,
            approver0,
            approver1,
            approver2,
            depositor,
            recipient,
            cap,
        }
    }

    // ===============================================================
    // (a) El agente ejecuta un micropago dentro del tope. ✔
    // ===============================================================

    #[test]
    fn operator_routes_micropayment_within_cap_succeeds() {
        let mut f = setup();
        let amount = f.cap; // exactamente el tope -> permitido
        let recipient_before = f.env.balance_of(&f.recipient);
        let vault_before = f.env.balance_of(&f.contract);

        f.env.set_caller(f.operator);
        f.contract.route_micropayment(f.recipient, amount);

        assert_eq!(f.env.balance_of(&f.recipient), recipient_before + amount);
        assert_eq!(f.env.balance_of(&f.contract), vault_before - amount);
        assert!(f.env.emitted_event(
            &f.contract,
            MicropaymentRouted {
                operator: f.operator,
                recipient: f.recipient,
                amount,
            }
        ));
    }

    #[test]
    fn operator_routes_small_micropayment_under_cap_succeeds() {
        let mut f = setup();
        let amount = U512::from(100_000_000); // 0.1 CSPR < 1 CSPR cap
        let recipient_before = f.env.balance_of(&f.recipient);

        f.env.set_caller(f.operator);
        f.contract.route_micropayment(f.recipient, amount);

        assert_eq!(f.env.balance_of(&f.recipient), recipient_before + amount);
    }

    // ===============================================================
    // (b) El agente intentando retirar capital -> revierte. ✔
    // ===============================================================

    #[test]
    fn operator_micropayment_above_cap_reverts() {
        let mut f = setup();
        let too_much = f.cap + U512::one();

        f.env.set_caller(f.operator);
        let result = f.contract.try_route_micropayment(f.recipient, too_much);

        assert_eq!(result.unwrap_err(), Error::CapExceeded.into());
        // El vault no se movió.
        assert_eq!(f.env.balance_of(&f.contract), U512::from(100 * ONE_CSPR));
    }

    #[test]
    fn operator_cannot_call_execute_reverts_not_admin() {
        let mut f = setup();
        // Propone y aprueba como roles válidos para dejar la solicitud lista.
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(5 * ONE_CSPR));
        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);

        // El agente intenta ejecutar el release grande.
        f.env.set_caller(f.operator);
        let result = f.contract.try_execute(id);

        assert_eq!(result.unwrap_err(), Error::NotAdmin.into());
        assert!(!f.contract.request_executed(id));
    }

    #[test]
    fn operator_cannot_propose_withdraw_reverts_not_approver() {
        let mut f = setup();

        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_propose_withdraw(f.recipient, U512::from(5 * ONE_CSPR));

        assert_eq!(result.unwrap_err(), Error::NotApprover.into());
    }

    #[test]
    fn operator_cannot_approve_reverts_not_approver() {
        let mut f = setup();
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(5 * ONE_CSPR));

        f.env.set_caller(f.operator);
        let result = f.contract.try_approve(id);

        assert_eq!(result.unwrap_err(), Error::NotApprover.into());
        assert_eq!(f.contract.approval_count(id), 0);
    }

    #[test]
    fn non_operator_non_admin_cannot_route_micropayment() {
        let mut f = setup();
        // Un approver (no operator) intenta enrutar micropago.
        f.env.set_caller(f.approver0);
        let result = f.contract.try_route_micropayment(f.recipient, U512::one());

        assert_eq!(result.unwrap_err(), Error::NotOperator.into());
    }

    // ===============================================================
    // (c) Release grande SOLO con M aprobaciones distintas + admin. ✔
    // ===============================================================

    #[test]
    fn execute_without_approvals_reverts() {
        let mut f = setup();
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(5 * ONE_CSPR));

        let result = f.contract.try_execute(id);

        assert_eq!(result.unwrap_err(), Error::InsufficientApprovals.into());
        assert!(!f.contract.request_executed(id));
    }

    #[test]
    fn execute_with_one_approval_below_threshold_reverts() {
        let mut f = setup(); // required = 2
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(5 * ONE_CSPR));

        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        assert_eq!(f.contract.approval_count(id), 1);

        f.env.set_caller(f.admin);
        let result = f.contract.try_execute(id);

        assert_eq!(result.unwrap_err(), Error::InsufficientApprovals.into());
        assert!(!f.contract.request_executed(id));
    }

    #[test]
    fn execute_with_m_distinct_approvals_and_admin_succeeds() {
        let mut f = setup(); // required = 2
        let amount = U512::from(5 * ONE_CSPR);
        f.env.set_caller(f.admin);
        let id = f.contract.propose_withdraw(f.recipient, amount);

        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);
        assert_eq!(f.contract.approval_count(id), 2);

        let recipient_before = f.env.balance_of(&f.recipient);
        let vault_before = f.env.balance_of(&f.contract);

        f.env.set_caller(f.admin);
        f.contract.execute(id);

        assert_eq!(f.env.balance_of(&f.recipient), recipient_before + amount);
        assert_eq!(f.env.balance_of(&f.contract), vault_before - amount);
        assert!(f.contract.request_executed(id));
        assert!(f.env.emitted_event(
            &f.contract,
            WithdrawExecuted {
                id,
                recipient: f.recipient,
                amount,
            }
        ));
    }

    #[test]
    fn execute_with_three_distinct_approvals_still_succeeds() {
        // M=2 pero 3 aprobaciones (>= M) también debe pasar.
        let mut f = setup();
        let amount = U512::from(3 * ONE_CSPR);
        f.env.set_caller(f.approver2); // un approver puede proponer
        let id = f.contract.propose_withdraw(f.recipient, amount);

        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);
        f.env.set_caller(f.approver2);
        f.contract.approve(id);
        assert_eq!(f.contract.approval_count(id), 3);

        f.env.set_caller(f.admin);
        f.contract.execute(id);
        assert!(f.contract.request_executed(id));
    }

    // ---- Aprobaciones deben ser DISTINTAS (anti same-signer doubling) ----

    #[test]
    fn approver_cannot_approve_twice_reverts() {
        let mut f = setup();
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(5 * ONE_CSPR));

        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        let result = f.contract.try_approve(id); // mismo approver, 2da vez

        assert_eq!(result.unwrap_err(), Error::AlreadyApproved.into());
        // El conteo sigue siendo 1 (no se duplicó).
        assert_eq!(f.contract.approval_count(id), 1);
    }

    #[test]
    fn two_approvals_from_same_signer_do_not_meet_threshold() {
        // Defensa activa: aunque same-signer se bloquea en `approve`, cubrimos
        // el escenario "M aprobaciones del mismo firmante" reventando antes.
        let mut f = setup(); // required = 2
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(5 * ONE_CSPR));

        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver0);
        assert!(f.contract.try_approve(id).is_err());

        f.env.set_caller(f.admin);
        let result = f.contract.try_execute(id);

        assert_eq!(result.unwrap_err(), Error::InsufficientApprovals.into());
    }

    // ---- Vigencia / no-doble-ejecución ----

    #[test]
    fn execute_twice_reverts_already_executed() {
        let mut f = setup();
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(5 * ONE_CSPR));
        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);

        f.env.set_caller(f.admin);
        f.contract.execute(id);

        let result = f.contract.try_execute(id);
        assert_eq!(result.unwrap_err(), Error::AlreadyExecuted.into());
    }

    #[test]
    fn approve_after_execution_reverts() {
        let mut f = setup();
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(5 * ONE_CSPR));
        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);
        f.env.set_caller(f.admin);
        f.contract.execute(id);

        // Un approver tardío no puede aprobar lo ya ejecutado.
        f.env.set_caller(f.approver2);
        let result = f.contract.try_approve(id);
        assert_eq!(result.unwrap_err(), Error::AlreadyExecuted.into());
    }

    #[test]
    fn approve_unknown_request_reverts() {
        let mut f = setup();
        f.env.set_caller(f.approver0);
        let result = f.contract.try_approve(999);
        assert_eq!(result.unwrap_err(), Error::RequestNotFound.into());
    }

    #[test]
    fn execute_unknown_request_reverts() {
        let mut f = setup();
        f.env.set_caller(f.admin);
        let result = f.contract.try_execute(999);
        assert_eq!(result.unwrap_err(), Error::RequestNotFound.into());
    }

    #[test]
    fn execute_insufficient_balance_reverts() {
        let mut f = setup();
        // Propone retirar más de lo que el vault tiene (100 CSPR).
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(1_000 * ONE_CSPR));
        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);

        f.env.set_caller(f.admin);
        let result = f.contract.try_execute(id);
        assert_eq!(result.unwrap_err(), Error::InsufficientBalance.into());
        assert!(!f.contract.request_executed(id));
    }

    // ---- Proposed events y flujo de eventos ----

    #[test]
    fn propose_and_approve_emit_expected_events() {
        let mut f = setup();
        let amount = U512::from(5 * ONE_CSPR);
        f.env.set_caller(f.admin);
        let id = f.contract.propose_withdraw(f.recipient, amount);

        assert!(f.env.emitted_event(
            &f.contract,
            WithdrawProposed {
                id,
                proposer: f.admin,
                recipient: f.recipient,
                amount,
            }
        ));

        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        assert!(f.env.emitted_event(
            &f.contract,
            WithdrawApproved {
                id,
                approver: f.approver0,
                count: 1,
            }
        ));
    }

    // ===============================================================
    // Init: validaciones de setup (negativas).
    // ===============================================================

    #[test]
    fn init_reverts_when_admin_equals_operator() {
        let env = odra_test::env();
        let admin = env.get_account(0);
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin,
                operator: admin,
                approvers: vec![env.get_account(2), env.get_account(3)],
                required_approvals: 1,
                micropayment_cap: U512::from(ONE_CSPR),
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    #[test]
    fn init_reverts_when_operator_is_also_approver() {
        let env = odra_test::env();
        let admin = env.get_account(0);
        let operator = env.get_account(1);
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin,
                operator,
                approvers: vec![operator, env.get_account(3)],
                required_approvals: 1,
                micropayment_cap: U512::from(ONE_CSPR),
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    #[test]
    fn init_reverts_when_duplicate_approvers() {
        let env = odra_test::env();
        let admin = env.get_account(0);
        let operator = env.get_account(1);
        let approver = env.get_account(2);
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin,
                operator,
                approvers: vec![approver, approver],
                required_approvals: 1,
                micropayment_cap: U512::from(ONE_CSPR),
            },
        );
        assert_eq!(result.err().unwrap(), Error::DuplicateApprover.into());
    }

    #[test]
    fn init_reverts_when_required_approvals_zero() {
        let env = odra_test::env();
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin: env.get_account(0),
                operator: env.get_account(1),
                approvers: vec![env.get_account(2), env.get_account(3)],
                required_approvals: 0,
                micropayment_cap: U512::from(ONE_CSPR),
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    #[test]
    fn init_reverts_when_required_approvals_exceeds_approvers() {
        let env = odra_test::env();
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin: env.get_account(0),
                operator: env.get_account(1),
                approvers: vec![env.get_account(2), env.get_account(3)],
                required_approvals: 3,
                micropayment_cap: U512::from(ONE_CSPR),
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    #[test]
    fn init_reverts_when_empty_approvers() {
        let env = odra_test::env();
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin: env.get_account(0),
                operator: env.get_account(1),
                approvers: vec![],
                required_approvals: 1,
                micropayment_cap: U512::from(ONE_CSPR),
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    #[test]
    fn init_reverts_when_zero_cap() {
        let env = odra_test::env();
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin: env.get_account(0),
                operator: env.get_account(1),
                approvers: vec![env.get_account(2), env.get_account(3)],
                required_approvals: 1,
                micropayment_cap: U512::zero(),
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    // ===============================================================
    // S1 (regresión): depósito sigue funcionando.
    // ===============================================================

    #[test]
    fn deposit_increases_purse_balance_and_emits_event() {
        let f = setup();
        let amount = U512::from(7 * ONE_CSPR);
        let before = f.env.balance_of(&f.contract); // 100 CSPR ya fondeados

        f.env.set_caller(f.depositor);
        f.contract.with_tokens(amount).deposit();

        assert_eq!(f.env.balance_of(&f.contract), before + amount);
        assert!(f.env.emitted_event(
            &f.contract,
            Deposit {
                sender: f.depositor,
                amount,
            }
        ));
    }

    #[test]
    fn deposit_zero_reverts() {
        let f = setup();
        f.env.set_caller(f.depositor);
        let result = f.contract.with_tokens(U512::zero()).try_deposit();
        assert_eq!(result.unwrap_err(), Error::ZeroAmount.into());
    }

    #[test]
    fn getters_reflect_init_configuration() {
        let f = setup();
        assert_eq!(f.contract.admin(), f.admin);
        assert_eq!(f.contract.operator(), f.operator);
        assert_eq!(f.contract.micropayment_cap(), f.cap);
        assert_eq!(f.contract.required_approvals(), 2);
        assert!(f.contract.is_approver(f.approver0));
        assert!(f.contract.is_approver(f.approver1));
        assert!(f.contract.is_approver(f.approver2));
        assert!(!f.contract.is_approver(f.operator));
        assert!(!f.contract.is_approver(f.admin));
    }
}

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
//! ## S3: atestaciones gasless (INV-5)
//!
//! Un firmante (comprador) produce una firma Ed25519 off-chain sobre el payload
//! `"OhuAttestation:" || lote_id || nonce || received`. El agente retransmite
//! la firma on-chain vía `verify_attestation`. La verificación on-chain usa
//! `casper_types::crypto::verify` (Ed25519 pura) y deriva la identidad del
//! firmante de `PublicKey → AccountHash`. Anti-replay: cada par `(signer, nonce)`
//! se consume una única vez; nonces estrictamente crecientes por signer.
//!
//! Ruta target (no implementada aún): EIP-712 con recuperación ECDSA/Secp256k1
//! vía `casper-eip-712`. Ver `attestation.rs` para los typehash y el diseño.
//!
//! ### Defensa en profundidad
//! 1. **Capa on-chain (este contrato, activa):** `execute` exige M aprobaciones
//!    **distintas** registradas en el contrato. Adicionalmente, `route_micropayment`
//!    tiene dos topes: por llamada (`micropayment_cap`) y acumulado por ventana de
//!    epoch (`epoch_cap`) — el acumulado es lo que materializa INV-1 en el contrato.
//! 2. **Capa nativa (off-chain): TODO(audit)** — la cuenta `admin` debería ser
//!    un multisig Casper (associated keys + deployment threshold > peso individual)
//!    para forzar co-firma en cada deploy de `execute`. Ver
//!    `infra/scripts/setup_admin_account.sh`. Actualmente el script no opera
//!    porque falta `KEYS_MANAGER_WASM`; la capa nativa está en pausa hasta
//!    resolver ese TODO. Mientras tanto, la capa on-chain (este contrato) es la
//!    única defensa activa.
//!
//! Invariantes aplicables: INV-1, INV-2 (la aritmética de aprobaciones es la
//! condición on-chain), INV-3, INV-4, INV-5, INV-6.
//!
//! ## TODO(audit) — diferidos de la auditoría
//! - Rotación/remoción de approvers post-init (hoy son inmutables; riesgo si
//!   una clave se compromete).
//! - Expiry de proposals (un request aprobado pero sin fondos del vault queda
//!   abierto indefinidamente).
//! - Limpieza de storage post-`execute` (`request_recipient`, `request_amount`,
//!   `has_approved`, `approval_count` permanecen sin liberar).
//! - Binding de `approve` a `(recipient, amount)` (hoy solo recibe `request_id`;
//!   el approver no firma criptográficamente el destino y monto).
//! - Migrar `verify_attestation` a EIP-712 con `recover_secp256k1` cuando
//!   `casper-eip-712` sea compatible con Odra 2.8.2. Ver `attestation.rs`.

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
    /// El total acumulado en la ventana de epoch supera el `epoch_cap` (INV-1).
    EpochCapExceeded = 15,
    /// El parámetro `epoch_window_ms` debe ser > 0.
    InvalidEpochWindow = 16,
    /// `admin` no puede ser approver (separación de roles).
    AdminIsApprover = 17,
    /// Demasiados approvers (máx 255, por `approval_count: u8`).
    ApproversTooMany = 18,
    // ── S3: atestación (INV-5) ──
    /// Clave pública inválida (no se pudo decodificar como Ed25519).
    AttestationInvalidPublicKey = 30,
    /// Firma inválida (no se pudo decodificar como Ed25519).
    AttestationInvalidSignatureBytes = 31,
    /// La firma Ed25519 no es válida para este mensaje y clave pública.
    AttestationInvalidSignature = 32,
    /// Este nonce ya fue usado por este firmante (anti-replay).
    AttestationNonceAlreadyUsed = 33,
    /// El nonce es menor o igual al último usado (debe ser estrictamente creciente).
    AttestationInvalidNonce = 34,
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

/// Evento: atestación registrada on-chain (INV-5, S3).
#[odra::event]
pub struct AttestationRecorded {
    pub lote_id: u64,
    pub signer: Address,
    pub received: bool,
    pub nonce: u64,
}

/// Contrato de custodia de Ohu.
///
/// TODO(audit): CES (`emit_event`) es el event standard soportado por Odra.
/// Verificar si CSPR.cloud indexa CES, native events, o ambos; ajustar a
/// `emit_native_event` si es necesario. Ver <https://odra.dev/docs/basics/events>.
#[odra::module(events = [Deposit, MicropaymentRouted, WithdrawProposed, WithdrawApproved, WithdrawExecuted, AttestationRecorded])]
pub struct OhuVault {
    /// Cuenta que ejecuta releases grandes (`caller == admin` en `execute`).
    admin: Var<Address>,
    /// Cuenta del agente; única que puede llamar `route_micropayment`.
    operator: Var<Address>,
    /// Tope de motes por llamada a `route_micropayment` (INV-1).
    micropayment_cap: Var<U512>,
    /// Tope acumulado de motes en la ventana de epoch (INV-1, capa on-chain).
    epoch_cap: Var<U512>,
    /// Ventana del epoch en milisegundos (`get_block_time()`).
    epoch_window_ms: Var<u64>,
    /// Marca de tiempo de inicio de la ventana actual.
    window_start: Var<u64>,
    /// Total acumulado de motes enrutados en la ventana actual.
    accumulated: Var<U512>,
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
    /// S3: último nonce usado por este firmante (anti-replay, INV-5).
    attestation_nonce: Mapping<Address, u64>,
    /// S3: atestación registrada para `(lote_id, signer)`.
    attestation_recorded: Mapping<(u64, Address), bool>,
}

#[odra::module]
impl OhuVault {
    /// Inicializa el vault con el modelo de seguridad de S2.
    ///
    /// Valida (revert con error específico):
    /// - `admin` y `operator` son cuentas (no contratos) y distintos entre sí.
    /// - `admin` no está en `approvers` (separación: quien ejecuta no vota).
    /// - `operator` no está en `approvers` (separación de roles del agente).
    /// - `approvers` no vacío, sin duplicados, todos cuentas, ≤ 255.
    /// - `required_approvals` en `[1, approvers.len()]`.
    /// - `micropayment_cap > 0`, `epoch_cap > 0`, `epoch_window_ms > 0`.
    /// - El contador de epoch arranca en el bloque actual.
    ///
    /// TODO(audit): verificar contra <https://odra.dev/docs/basics/native-token>
    /// si para S2+ se requiere un purse secundario aislado. El purse principal
    /// (creado por el runtime) basta para este spike.
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        &mut self,
        admin: Address,
        operator: Address,
        approvers: Vec<Address>,
        required_approvals: u8,
        micropayment_cap: U512,
        epoch_cap: U512,
        epoch_window_ms: u64,
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
        if approvers.len() > u8::MAX as usize {
            self.env().revert(Error::ApproversTooMany);
        }
        if required_approvals == 0 || (required_approvals as usize) > approvers.len() {
            self.env().revert(Error::InvalidSetup);
        }
        if micropayment_cap == U512::zero() {
            self.env().revert(Error::InvalidSetup);
        }
        if epoch_cap == U512::zero() {
            self.env().revert(Error::InvalidSetup);
        }
        if epoch_window_ms == 0 {
            self.env().revert(Error::InvalidEpochWindow);
        }

        for i in 0..approvers.len() {
            let a = approvers[i];
            if a.is_contract() {
                self.env().revert(Error::NotAnAccount);
            }
            if a == operator {
                self.env().revert(Error::InvalidSetup);
            }
            if a == admin {
                self.env().revert(Error::AdminIsApprover);
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
        self.epoch_cap.set(epoch_cap);
        self.epoch_window_ms.set(epoch_window_ms);
        // TODO(audit): confirmar que `get_block_time()` funciona durante el
        // deploy (está documentado en Odra 2.8.2). Ver <https://odra.dev/docs/>.
        self.window_start.set(self.env().get_block_time());
        self.accumulated.set(U512::zero());
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
    /// - `recipient` debe ser cuenta, no contrato (consistencia con release).
    /// - `accumulated + amount <= epoch_cap` (tope **acumulado on-chain** por
    ///   ventana de epoch — esto es lo que materializa INV-1 en el contrato).
    /// - `amount <= self_balance`.
    ///
    /// La ventana de epoch se mide con `get_block_time()` (ms): cuando el bloque
    /// actual ≥ `window_start + epoch_window_ms`, la ventana se resetea y el
    /// contador acumulado vuelve a cero. Esto acota el daño incluso si el
    /// operator se comporta maliciosamente y emite muchas llamadas.
    ///
    /// TODO(audit): confirmar contra los docs de Casper que
    /// `transfer_tokens` es un *balance move* sin callback al receptor (sí lo
    /// es en el runtime de Casper). Mientras tanto, `execute` aplica CEI por
    /// defensa en profundidad.
    /// TODO(audit): `get_block_time()` está documentado en Odra 2.8.2; confirmar
    /// que la resolución en Testnet es suficiente para ventanas de epoch de
    /// ~minutos/horas. Ver <https://odra.dev/docs/>.
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
        if recipient.is_contract() {
            self.env().revert(Error::NotAnAccount);
        }

        // --- Epoch window: cap acumulado on-chain (INV-1) ---
        let epoch_cap = self.epoch_cap.get_or_revert_with(Error::NotInitialized);
        let epoch_window_ms = self
            .epoch_window_ms
            .get_or_revert_with(Error::NotInitialized);
        let mut window_start = self.window_start.get_or_revert_with(Error::NotInitialized);
        let mut accumulated = self.accumulated.get_or_revert_with(Error::NotInitialized);
        let now = self.env().get_block_time();

        if now >= window_start + epoch_window_ms {
            window_start = now;
            accumulated = U512::zero();
            self.window_start.set(window_start);
        }

        let new_accumulated = accumulated + amount;
        if new_accumulated > epoch_cap {
            self.env().revert(Error::EpochCapExceeded);
        }

        let balance = self.env().self_balance();
        if amount > balance {
            self.env().revert(Error::InsufficientBalance);
        }

        self.accumulated.set(new_accumulated);
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

    // ─────────────────────────────────────────────────────────────────
    // S3 — Atestaciones gasless (INV-5)
    // ─────────────────────────────────────────────────────────────────

    /// Verifica una atestación Ed25519 firmada off-chain y la registra on-chain.
    ///
    /// El firmante (comprador) firma el mensaje `"OhuAttestation:" || lote_id ||
    /// nonce || received` con su clave Ed25519. El agente retransmite la firma
    /// pagando el gas (gasless para el firmante).
    ///
    /// # Verificación
    /// 1. Decodifica `public_key_bytes` (32 bytes) y `signature_bytes` (64 bytes).
    /// 2. Reconstruye el mensaje y verifica la firma Ed25519.
    /// 3. Deriva `AccountHash` de la clave pública → identidad del firmante.
    /// 4. Anti-replay: `nonce` debe ser estrictamente mayor que el último usado
    ///    por este firmante.
    /// 5. Registra la atestación en `attestation_recorded[(lote_id, signer)]`.
    ///
    /// # Retorna
    /// `true` si la atestación es válida y se registró; revierte en caso
    /// contrario (firma inválida, replay, etc.).
    ///
    /// TODO(audit): migrar a EIP-712 cuando `casper-eip-712` (v1.2.0+) sea
    /// compatible con Odra 2.8.2. El mensaje sería el digest EIP-712 en lugar
    /// del mensaje plano Ed25519. Ver `attestation.rs`.
    pub fn verify_attestation(
        &mut self,
        lote_id: u64,
        nonce: u64,
        received: bool,
        public_key_bytes: [u8; 32],
        signature_bytes: [u8; 64],
    ) -> bool {
        use crate::attestation::verify_attestation_signature;

        let signer = verify_attestation_signature(
            lote_id,
            nonce,
            received,
            public_key_bytes,
            signature_bytes,
        );

        let signer = match signer {
            Ok(s) => s,
            Err(e) => match e {
                // TODO(audit): mapear errores del módulo a errores del contrato.
                // Por ahora mapeamos genéricamente.
                crate::attestation::AttestationError::InvalidPublicKey => {
                    self.env().revert(Error::AttestationInvalidPublicKey)
                }
                crate::attestation::AttestationError::InvalidSignatureBytes => {
                    self.env().revert(Error::AttestationInvalidSignatureBytes)
                }
                crate::attestation::AttestationError::InvalidSignature => {
                    self.env().revert(Error::AttestationInvalidSignature)
                }
                _ => self.env().revert(Error::AttestationInvalidSignature),
            },
        };

        // Anti-replay: nonce debe ser estrictamente creciente por signer.
        let last_nonce = self.attestation_nonce.get_or_default(&signer);
        if nonce <= last_nonce {
            self.env().revert(Error::AttestationInvalidNonce);
        }

        // Anti-replay: este nonce no debe haber sido usado ya.
        let replay_key = (lote_id, signer);
        if self.attestation_recorded.get_or_default(&replay_key) {
            self.env().revert(Error::AttestationNonceAlreadyUsed);
        }

        // Registrar.
        self.attestation_nonce.set(&signer, nonce);
        self.attestation_recorded.set(&replay_key, true);

        self.env().emit_event(AttestationRecorded {
            lote_id,
            signer,
            received,
            nonce,
        });

        true
    }

    // ── Getters de atestación (S3) ──

    /// Último nonce usado por `signer`.
    pub fn attestation_nonce(&self, signer: Address) -> u64 {
        self.attestation_nonce.get_or_default(&signer)
    }

    /// ¿La atestación para `(lote_id, signer)` ya fue registrada?
    pub fn attestation_recorded(&self, lote_id: u64, signer: Address) -> bool {
        self.attestation_recorded.get_or_default(&(lote_id, signer))
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

    /// Tope acumulado por ventana de epoch para `route_micropayment`.
    pub fn epoch_cap(&self) -> U512 {
        self.epoch_cap.get_or_revert_with(Error::NotInitialized)
    }

    /// Ventana del epoch en milisegundos.
    pub fn epoch_window_ms(&self) -> u64 {
        self.epoch_window_ms
            .get_or_revert_with(Error::NotInitialized)
    }

    /// Total acumulado en la ventana de epoch actual.
    pub fn accumulated(&self) -> U512 {
        self.accumulated.get_or_revert_with(Error::NotInitialized)
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
        AttestationRecorded, Deposit, Error, MicropaymentRouted, OhuVault, OhuVaultHostRef,
        OhuVaultInitArgs, WithdrawApproved, WithdrawExecuted, WithdrawProposed,
    };
    use odra::casper_types::crypto::{self, PublicKey, SecretKey};
    use odra::casper_types::U512;
    use odra::host::{Deployer, HostEnv, HostRef};
    use odra::prelude::Address;

    const ONE_CSPR: u64 = 1_000_000_000;

    /// Fixture: admin=acct0, operator=acct1, approvers=acct2..4 (M=2),
    /// cap=1 CSPR/llamada, epoch_cap=5 CSPR/ventana, depositor=acct5.
    /// Vault fondeado con 100 CSPR.
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
        epoch_cap: U512,
        epoch_window_ms: u64,
    }

    fn setup() -> Fixture {
        setup_with(U512::from(ONE_CSPR), 2, U512::from(5 * ONE_CSPR), 3_600_000)
    }

    fn setup_with(cap: U512, required: u8, epoch_cap: U512, epoch_window_ms: u64) -> Fixture {
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
                epoch_cap,
                epoch_window_ms,
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
            epoch_cap,
            epoch_window_ms,
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

    // ---- Micropayment: validaciones nuevas ----

    #[test]
    fn route_micropayment_zero_amount_reverts() {
        let mut f = setup();
        f.env.set_caller(f.operator);
        let result = f.contract.try_route_micropayment(f.recipient, U512::zero());
        assert_eq!(result.unwrap_err(), Error::ZeroAmount.into());
        assert_eq!(f.env.balance_of(&f.contract), U512::from(100 * ONE_CSPR));
    }

    #[test]
    fn route_micropayment_insufficient_balance_reverts() {
        let mut f = setup_with(
            U512::from(ONE_CSPR),
            2,
            U512::from(10 * ONE_CSPR), // epoch_cap holgado
            3_600_000,
        );
        // Vacía el vault con una transferencia grande vía M-de-N
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(99 * ONE_CSPR));
        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);
        f.env.set_caller(f.admin);
        f.contract.execute(id);
        // Queda ~1 CSPR. Usamos el cap (1 CSPR) → ok.
        f.env.set_caller(f.operator);
        f.contract
            .route_micropayment(f.recipient, U512::from(ONE_CSPR));
        // Ahora el vault está prácticamente vacío; el siguiente revierte.
        let result = f
            .contract
            .try_route_micropayment(f.recipient, U512::from(ONE_CSPR));
        assert_eq!(result.unwrap_err(), Error::InsufficientBalance.into());
    }

    #[test]
    fn route_micropayment_to_contract_reverts() {
        let mut f = setup();
        // El propio vault es un contrato: `is_contract()` devuelve true.
        let vault_addr = f.contract.contract_address();
        f.env.set_caller(f.operator);
        let result = f.contract.try_route_micropayment(vault_addr, U512::one());
        assert_eq!(result.unwrap_err(), Error::NotAnAccount.into());
    }

    // ---- Epoch cap acumulado (INV-1 on-chain) ----

    #[test]
    fn operator_routes_two_micropayments_within_epoch_cap_succeeds() {
        let mut f = setup(); // cap=1 CSPR/llamada, epoch_cap=5 CSPR
        let amount = U512::from(ONE_CSPR);
        let recipient_before = f.env.balance_of(&f.recipient);

        f.env.set_caller(f.operator);
        f.contract.route_micropayment(f.recipient, amount);
        f.contract.route_micropayment(f.recipient, amount); // 2ª llamada, acumulado=2 <= 5

        assert_eq!(
            f.env.balance_of(&f.recipient),
            recipient_before + U512::from(2 * ONE_CSPR)
        );
    }

    #[test]
    fn operator_third_micropayment_exceeds_epoch_cap_reverts() {
        let mut f = setup_with(
            U512::from(ONE_CSPR),
            2,
            U512::from(2 * ONE_CSPR), // epoch_cap = 2 CSPR
            3_600_000,
        );
        let amount = U512::from(ONE_CSPR);
        let recipient_before = f.env.balance_of(&f.recipient);

        f.env.set_caller(f.operator);
        f.contract.route_micropayment(f.recipient, amount); // acumulado=1
        f.contract.route_micropayment(f.recipient, amount); // acumulado=2, justo en el tope

        // La tercera llamada haría acumulado=3 > epoch_cap=2 → revierte.
        let result = f.contract.try_route_micropayment(f.recipient, amount);
        assert_eq!(result.unwrap_err(), Error::EpochCapExceeded.into());
        // La tercera no aplicó: balance del destinatario no cambió más.
        assert_eq!(
            f.env.balance_of(&f.recipient),
            recipient_before + U512::from(2 * ONE_CSPR)
        );
    }

    #[test]
    fn epoch_resets_after_window_allows_new_micropayment() {
        let mut f = setup_with(
            U512::from(ONE_CSPR),
            2,
            U512::from(ONE_CSPR), // epoch_cap = 1 CSPR
            60_000,               // ventana corta: 60s (60000 ms)
        );
        let amount = U512::from(ONE_CSPR);
        let recipient_before = f.env.balance_of(&f.recipient);

        f.env.set_caller(f.operator);
        f.contract.route_micropayment(f.recipient, amount);
        // acumulado == epoch_cap == 1 CSPR → siguiente debe revertir.

        let result = f.contract.try_route_micropayment(f.recipient, amount);
        assert_eq!(result.unwrap_err(), Error::EpochCapExceeded.into());

        // Avanzar el tiempo de bloque más allá de la ventana.
        f.env.advance_block_time(60_001);

        // Ahora la ventana se resetea: acumulado vuelve a 0.
        f.contract.route_micropayment(f.recipient, amount);

        assert_eq!(
            f.env.balance_of(&f.recipient),
            recipient_before + U512::from(2 * ONE_CSPR)
        );
    }

    // ---- propose_withdraw: validaciones nuevas ----

    #[test]
    fn propose_withdraw_by_random_caller_reverts() {
        let mut f = setup();
        let random = f.env.get_account(9);
        f.env.set_caller(random);
        let result = f
            .contract
            .try_propose_withdraw(f.recipient, U512::from(5 * ONE_CSPR));
        assert_eq!(result.unwrap_err(), Error::NotApprover.into());
    }

    #[test]
    fn propose_withdraw_zero_amount_reverts() {
        let mut f = setup();
        f.env.set_caller(f.admin);
        let result = f.contract.try_propose_withdraw(f.recipient, U512::zero());
        assert_eq!(result.unwrap_err(), Error::ZeroAmount.into());
    }

    #[test]
    fn propose_withdraw_contract_recipient_reverts() {
        let mut f = setup();
        let vault_addr = f.contract.contract_address();
        f.env.set_caller(f.admin);
        let result = f
            .contract
            .try_propose_withdraw(vault_addr, U512::from(5 * ONE_CSPR));
        assert_eq!(result.unwrap_err(), Error::NotAnAccount.into());
    }

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
                epoch_cap: U512::from(5 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
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
                epoch_cap: U512::from(5 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
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
                epoch_cap: U512::from(5 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
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
                epoch_cap: U512::from(5 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
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
                epoch_cap: U512::from(5 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
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
                epoch_cap: U512::from(5 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
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
                epoch_cap: U512::from(5 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    // ---- Init: validaciones nuevas post-auditoría (S2) ----

    #[test]
    fn init_reverts_when_zero_epoch_cap() {
        let env = odra_test::env();
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin: env.get_account(0),
                operator: env.get_account(1),
                approvers: vec![env.get_account(2), env.get_account(3)],
                required_approvals: 1,
                micropayment_cap: U512::from(ONE_CSPR),
                epoch_cap: U512::zero(),
                epoch_window_ms: 3_600_000,
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    #[test]
    fn init_reverts_when_zero_epoch_window() {
        let env = odra_test::env();
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin: env.get_account(0),
                operator: env.get_account(1),
                approvers: vec![env.get_account(2), env.get_account(3)],
                required_approvals: 1,
                micropayment_cap: U512::from(ONE_CSPR),
                epoch_cap: U512::from(5 * ONE_CSPR),
                epoch_window_ms: 0,
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidEpochWindow.into());
    }

    #[test]
    fn init_reverts_when_admin_is_approver() {
        let env = odra_test::env();
        let admin = env.get_account(0);
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin,
                operator: env.get_account(1),
                approvers: vec![admin, env.get_account(3)],
                required_approvals: 1,
                micropayment_cap: U512::from(ONE_CSPR),
                epoch_cap: U512::from(5 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
            },
        );
        assert_eq!(result.err().unwrap(), Error::AdminIsApprover.into());
    }

    #[test]
    fn init_reverts_when_too_many_approvers() {
        let env = odra_test::env();
        let one = env.get_account(0);
        // El check de longitud (>255) ocurre antes que el de duplicados.
        let many_approvers: Vec<Address> = vec![one; 256];
        let result = OhuVault::try_deploy(
            &env,
            OhuVaultInitArgs {
                admin: env.get_account(0),
                operator: env.get_account(1),
                approvers: many_approvers,
                required_approvals: 200,
                micropayment_cap: U512::from(ONE_CSPR),
                epoch_cap: U512::from(5 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
            },
        );
        assert_eq!(result.err().unwrap(), Error::ApproversTooMany.into());
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
        assert_eq!(f.contract.epoch_cap(), f.epoch_cap);
        assert_eq!(f.contract.epoch_window_ms(), f.epoch_window_ms);
        assert_eq!(f.contract.accumulated(), U512::zero());
        assert_eq!(f.contract.required_approvals(), 2);
        assert!(f.contract.is_approver(f.approver0));
        assert!(f.contract.is_approver(f.approver1));
        assert!(f.contract.is_approver(f.approver2));
        assert!(!f.contract.is_approver(f.operator));
        assert!(!f.contract.is_approver(f.admin));
    }

    // ===============================================================
    // S3 — Atestaciones gasless (INV-5): Ed25519 + anti-replay
    // ===============================================================

    /// Genera un par de claves Ed25519, firma una atestación y devuelve los
    /// bytes crudos de clave pública (32) y firma (64).
    fn sign_attestation(
        lote_id: u64,
        nonce: u64,
        received: bool,
    ) -> (SecretKey, [u8; 32], [u8; 64], Address) {
        let (secret_key, public_key) = crypto::generate_ed25519_keypair();
        let account_hash = public_key.to_account_hash();
        let signer = Address::Account(account_hash);

        let message = crate::attestation::build_attestation_message(lote_id, nonce, received);
        let signature = crypto::sign(&message, &secret_key, &public_key);

        let pk_bytes: [u8; 32] = Into::<Vec<u8>>::into(&public_key)
            .try_into()
            .expect("Ed25519 pk should be 32 bytes");
        let sig_bytes: [u8; 64] = Into::<Vec<u8>>::into(&signature)
            .try_into()
            .expect("Ed25519 sig should be 64 bytes");

        (secret_key, pk_bytes, sig_bytes, signer)
    }

    #[test]
    fn attestation_valid_signature_succeeds_and_records() {
        let mut f = setup();
        let lote_id = 1u64;
        let nonce = 5u64;
        let received = true;

        let (_sk, pk_bytes, sig_bytes, signer) =
            sign_attestation(lote_id, nonce, received);

        // Cualquier caller puede verificar (el agente paga el gas).
        f.env.set_caller(f.operator);
        let result = f
            .contract
            .verify_attestation(lote_id, nonce, received, pk_bytes, sig_bytes);

        assert!(result);
        assert_eq!(f.contract.attestation_nonce(signer), nonce);
        assert!(f.contract.attestation_recorded(lote_id, signer));
        assert!(f.env.emitted_event(
            &f.contract,
            AttestationRecorded {
                lote_id,
                signer,
                received,
                nonce,
            }
        ));
    }

    #[test]
    fn attestation_received_false_also_succeeds() {
        let mut f = setup();
        let (_sk, pk_bytes, sig_bytes, signer) =
            sign_attestation(2, 1, false);

        f.env.set_caller(f.operator);
        let result = f.contract.verify_attestation(2, 1, false, pk_bytes, sig_bytes);

        assert!(result);
        assert!(f.contract.attestation_recorded(2, signer));
    }

    #[test]
    fn attestation_multiple_signers_same_lote_succeeds() {
        let mut f = setup();
        let lote_id = 1u64;

        let (_sk1, pk1, sig1, signer1) = sign_attestation(lote_id, 1, true);
        let (_sk2, pk2, sig2, signer2) = sign_attestation(lote_id, 1, true);

        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(lote_id, 1, true, pk1, sig1));
        assert!(f.contract.verify_attestation(lote_id, 1, true, pk2, sig2));

        assert!(f.contract.attestation_recorded(lote_id, signer1));
        assert!(f.contract.attestation_recorded(lote_id, signer2));
    }

    // ── Negativos ────────────────────────────────────────────────────

    #[test]
    fn attestation_manipulated_public_key_reverts() {
        let mut f = setup();
        let (_, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, true);

        // Corrompe la clave pública.
        let mut bad_pk = pk_bytes;
        bad_pk[0] ^= 0xFF;

        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_verify_attestation(1, 1, true, bad_pk, sig_bytes);

        // Debe revertir: la firma no corresponde a esta clave.
        assert!(result.is_err());
        assert!(!f.contract.attestation_recorded(1, signer));
    }

    #[test]
    fn attestation_manipulated_signature_reverts() {
        let mut f = setup();
        let (_, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, true);

        // Corrompe la firma.
        let mut bad_sig = sig_bytes;
        bad_sig[10] ^= 0xFF;

        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_verify_attestation(1, 1, true, pk_bytes, bad_sig);

        assert!(result.is_err());
        // No se registró.
        assert!(!f.contract.attestation_recorded(1, signer));
    }

    #[test]
    fn attestation_manipulated_received_payload_reverts() {
        let mut f = setup();
        let (_, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, true);

        // La firma fue sobre `received = true` pero se intenta `received = false`.
        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_verify_attestation(1, 1, false, pk_bytes, sig_bytes);

        assert!(result.is_err());
        assert!(!f.contract.attestation_recorded(1, signer));
    }

    #[test]
    fn attestation_manipulated_nonce_reverts() {
        let mut f = setup();
        let (_, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 5, true);

        // La firma fue sobre nonce=5 pero se intenta nonce=3.
        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_verify_attestation(1, 3, true, pk_bytes, sig_bytes);

        assert!(result.is_err());
        assert!(!f.contract.attestation_recorded(1, signer));
    }

    #[test]
    fn attestation_replay_same_lote_nonce_reverts() {
        let mut f = setup();
        let (_, pk1, sig1, _signer1) = sign_attestation(1, 5, true);

        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(1, 5, true, pk1, sig1));

        // Replay con el mismo payload del mismo firmante: mismo nonce → nonce ya fue usado.
        let result = f
            .contract
            .try_verify_attestation(1, 5, true, pk1, sig1);

        assert_eq!(
            result.unwrap_err(),
            Error::AttestationInvalidNonce.into()
        );
    }

    #[test]
    fn attestation_replay_different_lote_same_signer_same_nonce_reverts() {
        let mut f = setup();
        let (sk, pk, _, _signer) = sign_attestation(1, 5, true);

        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(1, 5, true, pk, sign_second(&sk, 1, 5, true)));

        // Intenta usar el mismo nonce=5 en otro lote — debe revertir porque el nonce no aumentó.
        let sig_lote2 = sign_second(&sk, 2, 5, true);
        let result = f
            .contract
            .try_verify_attestation(2, 5, true, pk, sig_lote2);

        assert_eq!(
            result.unwrap_err(),
            Error::AttestationInvalidNonce.into()
        );
        // No se registró el lote 2 con nonce 5.
        assert!(!f.contract.attestation_recorded(2, Address::Account(PublicKey::from(&sk).to_account_hash())));
    }

    #[test]
    fn attestation_non_ascending_nonce_same_signer_reverts() {
        let mut f = setup();
        let (sk, pk, _, _signer) = sign_attestation(1, 10, true);

        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(1, 10, true, pk, sign_second(&sk, 1, 10, true)));

        // Firma para nonce=5 (< 10) con la MISMA clave.
        let msg5 = crate::attestation::build_attestation_message(1, 5, true);
        let public_key = PublicKey::from(&sk);
        let sig5 = crypto::sign(&msg5, &sk, &public_key);
        let sig5_bytes: [u8; 64] =
            Into::<Vec<u8>>::into(&sig5).try_into().unwrap();

        let result = f
            .contract
            .try_verify_attestation(1, 5, true, pk, sig5_bytes);

        assert_eq!(
            result.unwrap_err(),
            Error::AttestationInvalidNonce.into()
        );
    }

    #[test]
    fn attestation_same_signer_different_lote_succeeds() {
        let mut f = setup();
        let (sk, pk, _, _signer) = sign_attestation(1, 1, true);

        // Firma lote=1, nonce=1
        let public_key_for_signing = PublicKey::from(&sk);
        let msg1 = crate::attestation::build_attestation_message(1, 1, true);
        let sig1 = crypto::sign(&msg1, &sk, &public_key_for_signing);
        let sig1_bytes: [u8; 64] =
            Into::<Vec<u8>>::into(&sig1).try_into().unwrap();

        // Firma lote=2, nonce=2 (nonce creciente, lote distinto)
        let msg2 = crate::attestation::build_attestation_message(2, 2, true);
        let sig2 = crypto::sign(&msg2, &sk, &public_key_for_signing);
        let sig2_bytes: [u8; 64] =
            Into::<Vec<u8>>::into(&sig2).try_into().unwrap();

        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(1, 1, true, pk, sig1_bytes));
        assert!(f.contract.verify_attestation(2, 2, true, pk, sig2_bytes));
        // El nonce track es global por signer, no por lote.
        assert_eq!(f.contract.attestation_nonce(
            Address::Account(PublicKey::from(&sk).to_account_hash())
        ), 2);
    }

    #[test]
    fn attestation_same_nonce_different_signer_succeeds() {
        let mut f = setup();
        let (_sk1, pk1, sig1, _s1) = sign_attestation(1, 3, true);
        let (_sk2, pk2, sig2, _s2) = sign_attestation(1, 3, true);

        f.env.set_caller(f.operator);
        // Ambos usan nonce=3 pero son firmantes distintos → OK.
        assert!(f.contract.verify_attestation(1, 3, true, pk1, sig1));
        assert!(f.contract.verify_attestation(1, 3, true, pk2, sig2));
    }

    // ── Helper para firmar con una clave existente ──

    fn sign_second(
        sk: &SecretKey,
        lote_id: u64,
        nonce: u64,
        received: bool,
    ) -> [u8; 64] {
        let pk = PublicKey::from(sk);
        let msg = crate::attestation::build_attestation_message(lote_id, nonce, received);
        let sig = crypto::sign(&msg, sk, &pk);
        Into::<Vec<u8>>::into(&sig).try_into().unwrap()
    }
}

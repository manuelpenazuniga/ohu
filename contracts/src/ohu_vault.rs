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
//! `"OhuAttestation:" || lote_id || nonce || received || verifying_contract ||
//! chain_id`. El agente retransmite la firma on-chain vía `verify_attestation`.
//! La verificación on-chain usa `casper_types::crypto::verify` (Ed25519 pura)
//! y deriva la identidad del firmante de `PublicKey → AccountHash`.
//!
//! **Anti-replay (fix #3):** scoped a `(signer, lote_id)` — una atestación por
//! comprador por lote. No se impone monotonicidad global sobre el nonce.
//! **Domain separation (fix #4):** el mensaje incluye `verifyingContract`
//! (la dirección del propio vault) y `chain_id` (fijado por el deployer en init).
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
use odra::ContractRef;

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
    // ── W1-1: modelo de lote ──
    /// El lote_id ya existe (no se puede abrir dos veces).
    LoteAlreadyExists = 40,
    /// El lote_id no existe (no se ha abierto con `open_lote`).
    LoteNotFound = 41,
    /// El lote no está en estado OPEN (ya fue fondeado o no existe).
    LoteNotOpen = 42,
    /// Solo el productor registrado del lote puede ejecutar esta acción.
    NotProducer = 43,
    /// El bono del productor ya fue depositado para este lote.
    BondAlreadyPosted = 44,
    /// Overflow aritmético en la contabilidad por-lote (suma U512).
    Overflow = 45,
    /// El caller no es admin ni operator (gate de open_lote).
    NotAdminNorOperator = 46,
    /// El lote no está en estado FUNDED (release/propose/approve).
    LoteNotFunded = 47,
    /// Ya hay propuesta de release abierta para este lote.
    ReleaseAlreadyProposed = 48,
    /// No hay propuesta de release abierta para este lote.
    ReleaseNotProposed = 49,
    /// El productor no puede ser admin, operator ni approver (FIX 4 — separación de roles).
    ProducerIsPrivileged = 50,
    /// Error contable del reservado de lote (underflow en reserved_lote_balance).
    ReservedAccounting = 51,
    // ── W2-0: atestación ponderada y autorizada ──
    /// El firmante no es comprador del lote (lote_share == 0 en ese lote).
    NotABuyer = 52,
    /// La atestación expiró (now >= valid_before).
    AttestationExpired = 53,
    // ── W2-1: disparador paramétrico ──
    /// La ventana de atestación aún no cerró (now < lote_funded_at + window).
    WindowNotClosed = 54,
    /// El lote no está en estado EVAL_OK (no se puede liberar al productor).
    LoteNotReleasable = 55,
    // ── W2-2: SETTLED_FAIL ──
    /// El lote no está en estado EVAL_FAIL (no es fallable).
    LoteNotFailable = 56,
    /// El lote no está en estado SETTLED_FAIL (no se puede reclamar settlement).
    LoteNotSettledFail = 57,
    /// Este comprador ya reclamó su refund + indemnización de este lote.
    SettlementAlreadyClaimed = 58,
    // ── W2-3: MutualPool ──
    /// Los basis points deben estar en [0, 10000].
    InvalidBps = 59,
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

// ── W1-1: modelo de lote ──

/// Evento: se abre un nuevo lote de compra cooperativa (estado OPEN).
#[odra::event]
pub struct LoteOpened {
    pub lote_id: u64,
    pub producer: Address,
}

/// Evento: un comprador deposita su share earmarked a un lote (INV-7).
#[odra::event]
pub struct DepositedToLote {
    pub lote_id: u64,
    pub buyer: Address,
    pub amount: U512,
}

/// Evento: el productor publica su bono de cumplimiento.
#[odra::event]
pub struct BondPosted {
    pub lote_id: u64,
    pub producer: Address,
    pub amount: U512,
}

/// Evento: el lote alcanza el estado FUNDED (bono + fondeo ≥ 1 share).
#[odra::event]
pub struct LoteFunded {
    pub lote_id: u64,
}

// ── W1-2: settlement happy-path ──

/// Evento: se propone la liberación del escrow de un lote al productor.
#[odra::event]
pub struct ReleaseProposed {
    pub lote_id: u64,
    pub proposer: Address,
}

/// Evento: un approver vota la liberación del escrow de un lote.
#[odra::event]
pub struct ReleaseApproved {
    pub lote_id: u64,
    pub approver: Address,
    pub count: u8,
}

/// Evento: el escrow del lote se libera al productor (SETTLED_OK).
#[odra::event]
pub struct ReleasedToProducer {
    pub lote_id: u64,
    pub producer: Address,
    pub funded: U512,
    pub bond: U512,
}

// ── W2-1: disparador paramétrico ──

/// Evento: evaluate_lote fijó el resultado del lote (EVAL_OK o EVAL_FAIL)
/// sin mover fondos. El tally negativo y el funded van en U512 para
/// transparencia on-chain.
#[odra::event]
pub struct LoteEvaluated {
    pub lote_id: u64,
    pub result: u8,
    pub negative: U512,
    pub funded: U512,
}

// ── W2-2: SETTLED_FAIL ──

/// Evento: el lote falló (settle_failure). Estado → SETTLED_FAIL.
/// No mueve fondos; el bono queda como pool de indemnización del lote.
#[odra::event]
pub struct LoteSettledFail {
    pub lote_id: u64,
    pub funded: U512,
    pub bond: U512,
    pub producer: Address,
}

/// Evento: un comprador reclamó su refund + indemnización de un lote fallido.
/// PULL: cada comprador ejecuta withdraw_settlement.
#[odra::event]
pub struct SettlementWithdrawn {
    pub lote_id: u64,
    pub buyer: Address,
    pub refund: U512,
    pub indemnity: U512,
    pub amount: U512,
}

/// Contrato de custodia de Ohu.
///
/// TODO(audit): CES (`emit_event`) es el event standard soportado por Odra.
/// Verificar si CSPR.cloud indexa CES, native events, o ambos; ajustar a
/// `emit_native_event` si es necesario. Ver <https://odra.dev/docs/basics/events>.
#[odra::module(events = [Deposit, MicropaymentRouted, WithdrawProposed, WithdrawApproved, WithdrawExecuted, AttestationRecorded, LoteOpened, DepositedToLote, BondPosted, LoteFunded, LoteEvaluated, LoteSettledFail, SettlementWithdrawn, ReleaseProposed, ReleaseApproved, ReleasedToProducer])]
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
    /// S3: atestación registrada para `(lote_id, signer)` (anti-replay, fix #3).
    attestation_recorded: Mapping<(u64, Address), bool>,
    /// S3: identificador de cadena para domain separation (fix #4).
    chain_id: Var<u64>,
    // ── W2-1: disparador paramétrico ──
    /// Umbral de no-recepción en basis points (p.ej. 6000 = 60%).
    quorum_fail_bps: Var<u64>,
    /// Ventana tras FUNDED para atestar (ms); tras ella, silencio=recibido.
    attestation_window_ms: Var<u64>,
    // ── W1-1: modelo de lote (INV-7: escrow earmarked) ──
    /// INV-7 (FIX crítico): suma tracked de CSPR reservado para lotes activos.
    /// Se incrementa en `deposit_to_lote` y `post_bond`; se decrementa en
    /// `release_to_producer`. Los outflows genéricos (`route_micropayment`,
    /// `execute`) validan contra `self_balance() - reserved_lote_balance`.
    reserved_lote_balance: Var<U512>,
    /// Productor asignado a cada lote.
    lote_producer: Mapping<u64, Address>,
    /// Estado del lote: 0=inexistente, 1=OPEN, 2=FUNDED, 3=SETTLED_OK.
    lote_state: Mapping<u64, u8>,
    /// Suma total de shares depositadas en el lote (Σ depósitos de compradores).
    /// INV-7: esta contabilidad es POR LOTE, nunca se deriva de self_balance().
    lote_funded: Mapping<u64, U512>,
    /// Share depositada por `(lote_id, comprador)`.
    lote_share: Mapping<(u64, Address), U512>,
    /// Bono de cumplimiento depositado por el productor del lote.
    lote_bond: Mapping<u64, U512>,
    /// Timestamp (get_block_time) en que el lote pasó a FUNDED (W2-1).
    lote_funded_at: Mapping<u64, u64>,
    // ── W2-0: tally ponderado de atestaciones ──
    /// Suma de shares de firmantes que atestaron NO-recibido para el lote.
    lote_tally_negative: Mapping<u64, U512>,
    /// Suma de shares de firmantes que atestaron SÍ-recibido para el lote.
    lote_tally_positive: Mapping<u64, U512>,
    // ── W1-2: settlement M-de-N lote-aware ──
    /// `true` si ya hay una propuesta de release abierta para este lote.
    lote_release_proposed: Mapping<u64, bool>,
    /// Aprobaciones distintas acumuladas para el release de este lote.
    lote_release_approvals: Mapping<u64, u8>,
    /// `(lote_id, approver) -> true`: anti doble-aprobación.
    lote_release_has_approved: Mapping<(u64, Address), bool>,
    // ── W2-2: settlement fail ──
    /// `(lote_id, buyer) -> true`: el comprador ya reclamó su refund +
    /// indemnización en este lote (anti doble-claim).
    lote_settlement_claimed: Mapping<(u64, Address), bool>,
    // ── W2-3: MutualPool integration ──
    /// Dirección del contrato `MutualPool` (sin set = sin integración).
    mutual_pool: Var<Address>,
    /// Prima en basis points (p.ej. 50 = 0.5%) cobrada al producer en release feliz.
    /// 0 = sin prima.
    premium_bps: Var<u64>,
    /// Target de indemnización en basis points sobre `funded`.
    /// Si `bond < target`, la diferencia (cola) se trae del `MutualPool`.
    /// 0 = sin cola de mutual.
    indemnity_target_bps: Var<u64>,
    /// Pool de indemnización por lote para `withdraw_settlement`:
    /// solo el bono. La cola se guarda en `lote_tail` y la paga el MutualPool
    /// directo al comprador en cada `withdraw_settlement` (Casper-safe).
    /// Se fija en `settle_failure`.
    lote_indemnity_pool: Mapping<u64, U512>,
    /// Cola de indemnización del MutualPool para este lote (no está en el vault;
    /// se paga directo al comprador vía `pay_tail(caller, tail_share)` en cada
    /// `withdraw_settlement`). Se fija en `settle_failure`.
    lote_tail: Mapping<u64, U512>,
}

/// Constantes de estado de lote (W1-1, W2-1).
/// 0 = inexistente (default de Mapping); 1 = OPEN; 2 = FUNDED.
const LOTE_STATE_OPEN: u8 = 1;
const LOTE_STATE_FUNDED: u8 = 2;
const LOTE_STATE_SETTLED_OK: u8 = 3;
const LOTE_STATE_EVAL_OK: u8 = 4;
const LOTE_STATE_EVAL_FAIL: u8 = 5;
const LOTE_STATE_SETTLED_FAIL: u8 = 6;

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
    /// - `chain_id > 0` (se fija en deploy, domain separation fix #4).
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
        chain_id: u64,
        quorum_fail_bps: u64,
        attestation_window_ms: u64,
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
        if chain_id == 0 {
            self.env().revert(Error::InvalidSetup);
        }
        if quorum_fail_bps == 0 || quorum_fail_bps > 10000 {
            self.env().revert(Error::InvalidSetup);
        }
        if attestation_window_ms == 0 {
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
        self.reserved_lote_balance.set(U512::zero());
        self.required_approvals.set(required_approvals);
        self.next_request_id.set(0u64);
        self.chain_id.set(chain_id);
        self.quorum_fail_bps.set(quorum_fail_bps);
        self.attestation_window_ms.set(attestation_window_ms);
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

        if now.saturating_sub(window_start) >= epoch_window_ms {
            window_start = now;
            accumulated = U512::zero();
            self.window_start.set(window_start);
        }

        let new_accumulated = accumulated
            .checked_add(amount)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));
        if new_accumulated > epoch_cap {
            self.env().revert(Error::EpochCapExceeded);
        }

        // FIX crítico: sólo se puede gastar saldo NO reservado para lotes (INV-7).
        let reserved = self.reserved_lote_balance.get_or_default();
        let free = self
            .env()
            .self_balance()
            .checked_sub(reserved)
            .unwrap_or(U512::zero());
        if amount > free {
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

        // FIX crítico: ejecución genérica no puede gastar escrow reservado para lotes (INV-7).
        let reserved = self.reserved_lote_balance.get_or_default();
        let free = self
            .env()
            .self_balance()
            .checked_sub(reserved)
            .unwrap_or(U512::zero());
        if amount > free {
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
    /// nonce || received || verifying_contract || chain_id || valid_before`
    /// con su clave Ed25519. El agente retransmite la firma pagando el gas
    /// (gasless para el firmante).
    ///
    /// # Verificación
    /// 1. Decodifica `public_key_bytes` (32 bytes) y `signature_bytes` (64 bytes).
    /// 2. Reconstruye el mensaje (con `verifyingContract = self_address()`, el
    ///    `chain_id` guardado en init, y `valid_before`) y verifica la firma Ed25519.
    /// 3. Deriva `AccountHash` de la clave pública → identidad del firmante.
    /// 4. Expiry (S3 #2): revierte si `now >= valid_before`.
    /// 5. Autorización (S3 #1): revierte si el firmante no es comprador del lote
    ///    (`lote_share[(lote_id, signer)] == 0`).
    /// 6. Anti-replay (fix #3): scoped a `(signer, lote_id)` vía
    ///    `attestation_recorded`. Una atestación por comprador por lote.
    /// 7. Tally ponderado: acumula la share del firmante en `lote_tally_positive`
    ///    o `lote_tally_negative` según `received`.
    /// 8. Domain separation (fix #4): `verifyingContract`, `chain_id` y `valid_before`
    ///    van en el mensaje firmado, impidiendo replay cross-contract/cross-chain.
    ///
    /// # Retorna
    /// `true` si la atestación es válida y se registró; revierte en caso
    /// contrario (firma inválida, expiry, no autorizada, replay, etc.).
    ///
    /// TODO(audit): migrar a EIP-712 cuando `casper-eip-712` (v1.2.0+) sea
    /// compatible con Odra 2.8.2. El mensaje sería el digest EIP-712 en lugar
    /// del mensaje plano Ed25519. Ver `attestation.rs`.
    pub fn verify_attestation(
        &mut self,
        lote_id: u64,
        nonce: u64,
        received: bool,
        valid_before: u64,
        public_key_bytes: [u8; 32],
        signature_bytes: [u8; 64],
    ) -> bool {
        use crate::attestation::verify_attestation_signature;

        let verifying_contract = self.env().self_address();
        let chain_id = self.chain_id.get_or_revert_with(Error::NotInitialized);

        // Gate 1: verificar firma Ed25519 (más costoso, pero debe ir primero:
        // si la firma no es válida no tiene sentido validar nada más).
        let signer = verify_attestation_signature(
            lote_id,
            nonce,
            received,
            verifying_contract,
            chain_id,
            valid_before,
            public_key_bytes,
            signature_bytes,
        );

        let signer = match signer {
            Ok(s) => s,
            Err(e) => match e {
                crate::attestation::AttestationError::InvalidPublicKey => {
                    self.env().revert(Error::AttestationInvalidPublicKey)
                }
                crate::attestation::AttestationError::InvalidSignatureBytes => {
                    self.env().revert(Error::AttestationInvalidSignatureBytes)
                }
                crate::attestation::AttestationError::InvalidSignature => {
                    self.env().revert(Error::AttestationInvalidSignature)
                }
            },
        };

        // Gate 2: expiry (S3 #2) — barato: una lectura + comparación.
        // W2-0: valid_before va DENTRO del mensaje firmado (binding).
        if self.env().get_block_time() >= valid_before {
            self.env().revert(Error::AttestationExpired);
        }

        // Gate 3: autorización (S3 #1) — el firmante debe ser comprador del lote.
        let share = self.lote_share.get_or_default(&(lote_id, signer));
        if share == U512::zero() {
            self.env().revert(Error::NotABuyer);
        }

        // Gate 4: anti-replay (fix #3) — una atestación por comprador por lote.
        let replay_key = (lote_id, signer);
        if self.attestation_recorded.get_or_default(&replay_key) {
            self.env().revert(Error::AttestationNonceAlreadyUsed);
        }

        // Gate 5: acumular tally ponderado (W2-0).
        // Mapping de Odra no es iterable → acumulación INCREMENTAL al llegar
        // cada atestación. `share = lote_share[(lote_id, signer)]` es el peso.
        if received {
            let current = self.lote_tally_positive.get_or_default(&lote_id);
            let new_total = current
                .checked_add(share)
                .unwrap_or_else(|| self.env().revert(Error::Overflow));
            self.lote_tally_positive.set(&lote_id, new_total);
        } else {
            let current = self.lote_tally_negative.get_or_default(&lote_id);
            let new_total = current
                .checked_add(share)
                .unwrap_or_else(|| self.env().revert(Error::Overflow));
            self.lote_tally_negative.set(&lote_id, new_total);
        }

        // Registrar anti-replay + emitir evento.
        self.attestation_recorded.set(&replay_key, true);

        self.env().emit_event(AttestationRecorded {
            lote_id,
            signer,
            received,
            nonce,
        });

        true
    }

    // ── W1-1: modelo de lote (INV-7: escrow earmarked) ──

    /// Abre un nuevo lote de compra cooperativa en estado OPEN.
    ///
    /// Gates:
    /// - `caller == admin` o `caller == operator`.
    /// - `lote_id` no debe existir ya (`lote_state` ≠ 0).
    /// - `producer` debe ser cuenta (no contrato, INV-3).
    ///
    /// El lote nace con `lote_funded = 0`, `lote_bond = 0`, sin shares.
    pub fn open_lote(&mut self, lote_id: u64, producer: Address) {
        let caller = self.env().caller();
        let admin = self.admin.get_or_revert_with(Error::NotInitialized);
        let operator = self.operator.get_or_revert_with(Error::NotInitialized);
        if caller != admin && caller != operator {
            self.env().revert(Error::NotAdminNorOperator);
        }

        if self.lote_state.get_or_default(&lote_id) != 0 {
            self.env().revert(Error::LoteAlreadyExists);
        }

        if producer.is_contract() {
            self.env().revert(Error::NotAnAccount);
        }

        // FIX 4: el productor no puede ser un rol privilegiado del vault.
        if producer == admin {
            self.env().revert(Error::ProducerIsPrivileged);
        }
        if producer == operator {
            self.env().revert(Error::ProducerIsPrivileged);
        }
        if self.is_approver.get_or_default(&producer) {
            self.env().revert(Error::ProducerIsPrivileged);
        }

        self.lote_producer.set(&lote_id, producer);
        self.lote_state.set(&lote_id, LOTE_STATE_OPEN);
        // lote_funded y lote_bond comienzan en ceros (default de Mapping).

        self.env().emit_event(LoteOpened { lote_id, producer });
    }

    /// Deposita la share earmarked del comprador a un lote (INV-7).
    ///
    /// `#[odra(payable)]`: el caller envía CSPR junto con la llamada. El
    /// monto se registra en `lote_share[(lote_id, caller)]` y se acumula en
    /// `lote_funded[lote_id]`.
    ///
    /// Gates:
    /// - El lote debe existir (`lote_state` ≠ 0).
    /// - El lote debe estar en estado OPEN (no FUNDED/SETTLED).
    /// - `attached_value() > 0`.
    ///
    /// INV-7: los fondos se contabilizan por lote en los Mappings, nunca
    /// desde `self_balance()`. Dos lotes jamás comparten saldo entre sí.
    ///
    /// TODO(audit): si el lote ya tiene bono y este es el primer depósito,
    /// la transición a FUNDED ocurre aquí mismo (regla conservadora: se
    /// necesita tanto bono como fondeo > 0).
    #[odra(payable)]
    pub fn deposit_to_lote(&mut self, lote_id: u64) {
        let buyer = self.env().caller();
        let amount = self.env().attached_value();

        if amount == U512::zero() {
            self.env().revert(Error::ZeroAmount);
        }

        let state = self.lote_state.get_or_default(&lote_id);
        if state == 0 {
            self.env().revert(Error::LoteNotFound);
        }
        if state != LOTE_STATE_OPEN {
            self.env().revert(Error::LoteNotOpen);
        }

        // INV-7: checked_add — overflow U512 revierte con Error::Overflow (el `+` plano
        // envolvería en silencio en release de WASM).
        let share_key = (lote_id, buyer);
        let old_share = self.lote_share.get_or_default(&share_key);
        let new_share = old_share
            .checked_add(amount)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));
        self.lote_share.set(&share_key, new_share);

        let old_funded = self.lote_funded.get_or_default(&lote_id);
        let new_funded = old_funded
            .checked_add(amount)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));
        self.lote_funded.set(&lote_id, new_funded);

        // FIX crítico: el CSPR depositado al lote queda reservado (INV-7).
        let old_reserved = self.reserved_lote_balance.get_or_default();
        let new_reserved = old_reserved
            .checked_add(amount)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));
        self.reserved_lote_balance.set(new_reserved);

        self.env().emit_event(DepositedToLote {
            lote_id,
            buyer,
            amount,
        });

        // Transición a FUNDED si el bono ya fue depositado.
        self.try_transition_to_funded(lote_id);
    }

    /// El productor del lote deposita su bono de cumplimiento.
    ///
    /// `#[odra(payable)]`: el monto enviado queda en el purse del contrato
    /// como parte del escrow global. La contabilidad del bono es por-lote
    /// en `lote_bond[lote_id]`.
    ///
    /// Gates:
    /// - `caller` debe ser el `lote_producer` registrado.
    /// - El lote debe existir.
    /// - El bono no debe haber sido depositado ya (`lote_bond[lote_id] == 0`).
    ///
    /// Transición a FUNDED si además `lote_funded[lote_id] > 0`.
    ///
    /// TODO(audit): verificar si la transición a FUNDED debe exigir un
    /// mínimo de fondeo (umbral paramétrico). La regla actual (bono>0 ∧
    /// funded>0) es la más conservadora.
    #[odra(payable)]
    pub fn post_bond(&mut self, lote_id: u64) {
        let caller = self.env().caller();
        let amount = self.env().attached_value();

        if amount == U512::zero() {
            self.env().revert(Error::ZeroAmount);
        }

        let state = self.lote_state.get_or_default(&lote_id);
        if state == 0 {
            self.env().revert(Error::LoteNotFound);
        }
        if state != LOTE_STATE_OPEN {
            self.env().revert(Error::LoteNotOpen);
        }

        let producer = match self.lote_producer.get(&lote_id) {
            Some(p) => p,
            None => self.env().revert(Error::LoteNotFound),
        };

        if caller != producer {
            self.env().revert(Error::NotProducer);
        }

        let existing_bond = self.lote_bond.get_or_default(&lote_id);
        if existing_bond > U512::zero() {
            self.env().revert(Error::BondAlreadyPosted);
        }

        self.lote_bond.set(&lote_id, amount);

        // FIX crítico: el bono depositado al lote queda reservado (INV-7).
        let old_reserved = self.reserved_lote_balance.get_or_default();
        let new_reserved = old_reserved
            .checked_add(amount)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));
        self.reserved_lote_balance.set(new_reserved);

        self.env().emit_event(BondPosted {
            lote_id,
            producer: caller,
            amount,
        });

        // Transición a FUNDED si ya hay fondeo.
        self.try_transition_to_funded(lote_id);
    }

    /// Intenta la transición OPEN → FUNDED cuando tanto el bono como el
    /// fondeo del lote son > 0.
    ///
    /// INV-7: solo opera sobre el lote indicado; no cruza contabilidad
    /// entre lotes.
    fn try_transition_to_funded(&mut self, lote_id: u64) {
        let state = self.lote_state.get_or_default(&lote_id);
        if state != LOTE_STATE_OPEN {
            return;
        }
        let bond = self.lote_bond.get_or_default(&lote_id);
        let funded = self.lote_funded.get_or_default(&lote_id);
        if bond > U512::zero() && funded > U512::zero() {
            self.lote_state.set(&lote_id, LOTE_STATE_FUNDED);
            self.lote_funded_at
                .set(&lote_id, self.env().get_block_time());
            self.env().emit_event(LoteFunded { lote_id });
        }
    }

    // ── W2-1: disparador paramétrico (INV-2) ──

    /// Evalúa el resultado del lote usando el tally ponderado de atestaciones
    /// (INV-2: condición on-chain determinista, sin juicio del agente).
    ///
    /// Gates:
    /// - caller ∈ {admin, operator} (NotAdminNorOperator).
    /// - lote en estado FUNDED (LoteNotFunded).
    /// - ventana de atestación cerrada: now >= lote_funded_at + window (WindowNotClosed).
    ///
    /// Cálculo (silencio=recibido):
    ///   si neg * 10000 >= funded * quorum_fail_bps → EVAL_FAIL
    ///   si no → EVAL_OK
    ///
    /// Emite LoteEvaluated. No mueve fondos.
    pub fn evaluate_lote(&mut self, lote_id: u64) {
        let caller = self.env().caller();
        let admin = self.admin.get_or_revert_with(Error::NotInitialized);
        let operator = self.operator.get_or_revert_with(Error::NotInitialized);
        if caller != admin && caller != operator {
            self.env().revert(Error::NotAdminNorOperator);
        }

        let state = self.lote_state.get_or_default(&lote_id);
        if state != LOTE_STATE_FUNDED {
            self.env().revert(Error::LoteNotFunded);
        }

        let funded_at = self.lote_funded_at.get_or_default(&lote_id);
        let window = self
            .attestation_window_ms
            .get_or_revert_with(Error::NotInitialized);
        let deadline = funded_at
            .checked_add(window)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));
        if self.env().get_block_time() < deadline {
            self.env().revert(Error::WindowNotClosed);
        }

        let neg = self.lote_tally_negative.get_or_default(&lote_id);
        let funded = self.lote_funded.get_or_default(&lote_id);
        let qbps = self
            .quorum_fail_bps
            .get_or_revert_with(Error::NotInitialized);

        // Silencio=recibido: quórum de fallo si neg * 10000 >= funded * qbps.
        // U512::checked_mul en ambos lados para prevenir overflow.
        let neg_scaled = neg
            .checked_mul(U512::from(10000u64))
            .unwrap_or_else(|| self.env().revert(Error::Overflow));
        let threshold = funded
            .checked_mul(U512::from(qbps))
            .unwrap_or_else(|| self.env().revert(Error::Overflow));

        let result_state = if neg_scaled >= threshold {
            LOTE_STATE_EVAL_FAIL
        } else {
            LOTE_STATE_EVAL_OK
        };

        self.lote_state.set(&lote_id, result_state);

        self.env().emit_event(LoteEvaluated {
            lote_id,
            result: result_state,
            negative: neg,
            funded,
        });
    }

    // ── W1-2: settlement M-de-N lote-aware ──

    /// Propone la liberación del escrow de un lote al productor.
    ///
    /// Gate: `caller == admin` o `caller` es approver. El operator NO.
    /// El lote debe estar en estado FUNDED.
    /// Idempotente: si ya hay propuesta abierta → `ReleaseAlreadyProposed`.
    pub fn propose_release(&mut self, lote_id: u64) {
        let caller = self.env().caller();
        let admin = self.admin.get_or_revert_with(Error::NotInitialized);
        let caller_is_approver = self.is_approver.get_or_default(&caller);
        if caller != admin && !caller_is_approver {
            self.env().revert(Error::NotApprover);
        }

        let state = self.lote_state.get_or_default(&lote_id);
        if state != LOTE_STATE_FUNDED {
            self.env().revert(Error::LoteNotFunded);
        }

        if self.lote_release_proposed.get_or_default(&lote_id) {
            self.env().revert(Error::ReleaseAlreadyProposed);
        }

        self.lote_release_proposed.set(&lote_id, true);

        self.env().emit_event(ReleaseProposed {
            lote_id,
            proposer: caller,
        });
    }

    /// Aprueba el release de un lote. Solo `approvers`; un mismo approver
    /// no puede aprobar dos veces el release del mismo lote.
    pub fn approve_release(&mut self, lote_id: u64) {
        let caller = self.env().caller();
        if !self.is_approver.get_or_default(&caller) {
            self.env().revert(Error::NotApprover);
        }

        let state = self.lote_state.get_or_default(&lote_id);
        if state != LOTE_STATE_FUNDED {
            self.env().revert(Error::LoteNotFunded);
        }

        if !self.lote_release_proposed.get_or_default(&lote_id) {
            self.env().revert(Error::ReleaseNotProposed);
        }

        let key = (lote_id, caller);
        if self.lote_release_has_approved.get_or_default(&key) {
            self.env().revert(Error::AlreadyApproved);
        }
        self.lote_release_has_approved.set(&key, true);

        let count = self.lote_release_approvals.get_or_default(&lote_id) + 1;
        self.lote_release_approvals.set(&lote_id, count);

        self.env().emit_event(ReleaseApproved {
            lote_id,
            approver: caller,
            count,
        });
    }

    /// Ejecuta el settlement de un lote: libera el escrow (`funded`)
    /// al productor y le devuelve su bono. Estado → SETTLED_OK.
    ///
    /// Gate (INV-1 + INV-2):
    /// - `caller == admin` (NotAdmin). El admin ejecuta; el agente nunca mueve capital.
    /// - `state == EVAL_OK` (LoteNotReleasable). El tally on-chain autoriza (INV-2);
    ///   el admin no decide si procede.
    ///
    /// CEI estricto: marca SETTLED_OK antes de `transfer_tokens`.
    ///
    /// **W2-3 — MutualPool (prima):** si `mutual_pool` está configurado Y
    /// `premium_bps > 0`, se deduce `premium = funded * premium_bps / 10000`
    /// del pago al productor y se envía al `MutualPool` vía cross-contract call
    /// con `with_tokens(premium).collect_premium()`. El productor recibe
    /// `funded + bond - premium`. `reserved_lote_balance` baja en `funded + bond`
    /// (la prima sale del purse igual que el pago al productor).
    /// Sin integración: comportamiento sin cambios.
    ///
    /// TODO(audit): confirmar firma exacta de `MutualPoolContractRef::new`
    /// y `with_tokens` en cross-contract calls (Odra 2.8.2).
    /// Ver <https://odra.dev/docs/basics/cross-calls>.
    ///
    /// TODO(Sem2: repurpose como emergency override): el M-de-N en-contrato
    /// (`propose_release` / `approve_release` / `lote_release_approvals`)
    /// queda como vestigio para una futura ruta de emergencia que puentee el
    /// tally normal. El release normal NO lo exige.
    /// La validación vestigial de approvals se conserva abajo como código
    /// comentado con el prefijo `// EMERGENCY:`.
    pub fn release_to_producer(&mut self, lote_id: u64) {
        let caller = self.env().caller();
        let admin = self.admin.get_or_revert_with(Error::NotInitialized);
        if caller != admin {
            self.env().revert(Error::NotAdmin);
        }

        let state = self.lote_state.get_or_default(&lote_id);
        if state != LOTE_STATE_EVAL_OK {
            self.env().revert(Error::LoteNotReleasable);
        }

        // EMERGENCY: vestigio del gate M-de-N original (W1-2), conservado como
        // código comentado para una futura ruta de override de emergencia.
        // let count = self.lote_release_approvals.get_or_default(&lote_id);
        // let required = self.required_approvals.get_or_revert_with(Error::NotInitialized);
        // if count < required {
        //     self.env().revert(Error::InsufficientApprovals);
        // }

        let payout = self.lote_funded.get_or_default(&lote_id);
        let bond = self.lote_bond.get_or_default(&lote_id);

        let total = payout
            .checked_add(bond)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));

        let balance = self.env().self_balance();
        if total > balance {
            self.env().revert(Error::InsufficientBalance);
        }

        let producer = match self.lote_producer.get(&lote_id) {
            Some(p) => p,
            None => self.env().revert(Error::LoteNotFound),
        };

        // W2-3: MutualPool premium (checked_mul antes de div).
        let premium_bps = self.premium_bps.get_or_default();
        let pool_addr_opt = self.mutual_pool.get();
        let premium = if premium_bps > 0 {
            if let Some(_pool_addr) = &pool_addr_opt {
                payout
                    .checked_mul(U512::from(premium_bps))
                    .unwrap_or_else(|| self.env().revert(Error::Overflow))
                    .checked_div(U512::from(10000u64))
                    .unwrap_or_else(|| self.env().revert(Error::Overflow))
            } else {
                U512::zero()
            }
        } else {
            U512::zero()
        };

        // El productor recibe (funded + bond - premium); la premium va al pool.
        // premium <= total garantizado: premium = funded * bps/10000 <= funded < total.
        let producer_payout = total
            .checked_sub(premium)
            .unwrap_or(U512::zero());

        // CEI: effects antes que interactions.
        self.lote_state.set(&lote_id, LOTE_STATE_SETTLED_OK);

        // FIX crítico: el lote liquidado libera su escrow reservado.
        // La prima también sale del purse → reserved baja en (funded + bond) total.
        let old_reserved = self.reserved_lote_balance.get_or_default();
        let new_reserved = old_reserved
            .checked_sub(total)
            .unwrap_or_else(|| self.env().revert(Error::ReservedAccounting));
        self.reserved_lote_balance.set(new_reserved);

        self.env().transfer_tokens(&producer, &producer_payout);

        if premium > U512::zero() {
            if let Some(pool_addr) = pool_addr_opt {
                let pool =
                    crate::mutual_pool::MutualPoolContractRef::new(self.env(), pool_addr);
                pool.with_tokens(premium).collect_premium();
            }
        }

        self.env().emit_event(ReleasedToProducer {
            lote_id,
            producer,
            funded: payout,
            bond,
        });
    }

    // ── W2-2: SETTLED_FAIL ──

    /// Cierra el lote fallido. Marca `SETTLED_FAIL` y fija:
    /// - `lote_indemnity_pool[lote] = bond` (el bono, que está en el vault).
    /// - `lote_tail[lote] = tail` (la cola del MutualPool; NO se trae al vault).
    ///
    /// **FIX CRÍTICO (Casper WASM):** NO transfiere la cola en este paso porque
    /// `transfer_tokens` a un `Address::Contract` revierte on-chain. La cola la
    /// paga el MutualPool directo al comprador (Address::Account → permitido) en
    /// cada `withdraw_settlement`.
    ///
    /// Gates:
    /// - `caller == admin` (NotAdmin). El agente nunca mueve capital (INV-1).
    /// - `state == EVAL_FAIL` (LoteNotFailable). El tally on-chain autoriza (INV-2).
    ///
    /// CEI: solo effects (no interactions, no transferencias). El pool no se toca.
    ///
    /// W2-3: el bono sigue siendo el pagador primario; la cola es acotada a la
    /// reserva del pool y se cobra per-cápita en cada withdraw.
    pub fn settle_failure(&mut self, lote_id: u64) {
        let caller = self.env().caller();
        let admin = self.admin.get_or_revert_with(Error::NotInitialized);
        if caller != admin {
            self.env().revert(Error::NotAdmin);
        }

        let state = self.lote_state.get_or_default(&lote_id);
        if state != LOTE_STATE_EVAL_FAIL {
            self.env().revert(Error::LoteNotFailable);
        }

        let funded = self.lote_funded.get_or_default(&lote_id);
        let bond = self.lote_bond.get_or_default(&lote_id);
        let producer = match self.lote_producer.get(&lote_id) {
            Some(p) => p,
            None => self.env().revert(Error::LoteNotFound),
        };

        // W2-3: calcular cola de indemnización del MutualPool.
        let indemnity_bps = self.indemnity_target_bps.get_or_default();
        let pool_addr_opt = self.mutual_pool.get();
        let tail = if let Some(pool_addr) = pool_addr_opt {
            if indemnity_bps > 0 {
                let target = funded
                    .checked_mul(U512::from(indemnity_bps))
                    .unwrap_or_else(|| self.env().revert(Error::Overflow))
                    .checked_div(U512::from(10000u64))
                    .unwrap_or_else(|| self.env().revert(Error::Overflow));
                if bond < target {
                    let deficit = target
                        .checked_sub(bond)
                        .unwrap_or(U512::zero());
                    let pool =
                        crate::mutual_pool::MutualPoolContractRef::new(self.env(), pool_addr);
                    let pool_reserve = pool.reserve();
                    if deficit < pool_reserve {
                        deficit
                    } else {
                        pool_reserve
                    }
                } else {
                    U512::zero()
                }
            } else {
                U512::zero()
            }
        } else {
            U512::zero()
        };

        // CEI: effects (solo escrituras, sin transferencias ni cross-calls).
        // FIX CRÍTICO: la cola NO se trae al vault; se guarda en lote_tail y
        // el comprador la cobra directo del pool en withdraw_settlement.
        self.lote_state.set(&lote_id, LOTE_STATE_SETTLED_FAIL);
        self.lote_indemnity_pool.set(&lote_id, bond);
        self.lote_tail.set(&lote_id, tail);

        self.env().emit_event(LoteSettledFail {
            lote_id,
            funded,
            bond,
            producer,
        });
    }

    /// PULL: un comprador reclama su refund (share) + indemnización de un lote
    /// fallido, desde DOS fuentes:
    ///
    /// 1. **VAULT** (`transfer_tokens → cuenta`, Casper-safe):
    ///    `refund = share`, `bond_indemnity = lote_indemnity_pool[lote] * share / funded`
    ///    (= `bond * share / funded`). Total vault = `share + bond_indemnity`.
    ///
    /// 2. **POOL** (`MutualPool.pay_tail(caller, tail_share)`, destinatario cuenta →
    ///    Casper-safe): `tail_share = lote_tail[lote] * share / funded`.
    ///
    /// Gates:
    /// - `state == SETTLED_FAIL` (LoteNotSettledFail).
    /// - `caller` es comprador: `share = lote_share[(lote, caller)] > 0`
    ///   (NotABuyer, ya existente).
    /// - No reclamado antes: `lote_settlement_claimed[(lote, caller)] == false`
    ///   (SettlementAlreadyClaimed).
    ///
    /// CEI estricto: marca claimed + decrementa reserved (solo la parte vault) ANTES
    /// de transferir.
    ///
    /// W2-3 FIX CRÍTICO (Casper WASM): la cola se paga directo al comprador
    /// (Address::Account → `transfer_tokens` permitido), NO a través del vault
    /// (Address::Contract → `transfer_tokens` revierte on-chain). `pay_tail` valida
    /// `amount <= reserve` en el momento del withdraw; si la reserva del pool ya no
    /// alcanza, el withdraw revierte — es un riesgo de liquidez, no de pérdida.
    ///
    /// Dust de división entera: la suma de todas las indemnizaciones puede ser
    /// < pool/tail. Ese dust queda en el vault/pool (inocuo).
    pub fn withdraw_settlement(&mut self, lote_id: u64) {
        let caller = self.env().caller();

        let state = self.lote_state.get_or_default(&lote_id);
        if state != LOTE_STATE_SETTLED_FAIL {
            self.env().revert(Error::LoteNotSettledFail);
        }

        let share = self.lote_share.get_or_default(&(lote_id, caller));
        if share == U512::zero() {
            self.env().revert(Error::NotABuyer);
        }

        let claim_key = (lote_id, caller);
        if self.lote_settlement_claimed.get_or_default(&claim_key) {
            self.env().revert(Error::SettlementAlreadyClaimed);
        }

        let funded = self.lote_funded.get_or_default(&lote_id);
        let pool = self.lote_indemnity_pool.get_or_default(&lote_id); // bond only

        // Indemnity from vault (bond portion).
        let bond_indemnity = pool
            .checked_mul(share)
            .unwrap_or_else(|| self.env().revert(Error::Overflow))
            .checked_div(funded)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));

        // Tail from MutualPool (paid directly to buyer account — Casper-safe).
        let tail_pool = self.lote_tail.get_or_default(&lote_id);
        let tail_share = tail_pool
            .checked_mul(share)
            .unwrap_or_else(|| self.env().revert(Error::Overflow))
            .checked_div(funded)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));

        let amount_vault = share
            .checked_add(bond_indemnity)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));

        let indemnity = bond_indemnity
            .checked_add(tail_share)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));

        let amount_total = amount_vault
            .checked_add(tail_share)
            .unwrap_or_else(|| self.env().revert(Error::Overflow));

        // CEI: effects antes que interactions.
        self.lote_settlement_claimed.set(&claim_key, true);

        let old_reserved = self.reserved_lote_balance.get_or_default();
        let new_reserved = old_reserved
            .checked_sub(amount_vault) // Solo la parte que sale del vault
            .unwrap_or_else(|| self.env().revert(Error::ReservedAccounting));
        self.reserved_lote_balance.set(new_reserved);

        // Transfer from vault (share + bond_indemnity → cuenta, Casper-safe).
        self.env().transfer_tokens(&caller, &amount_vault);

        // Transfer from MutualPool (tail_share → cuenta del comprador, Casper-safe).
        // pay_tail valida caller==authorized_vault y amount<=reserve en el momento.
        // Si la reserva del pool ya no alcanza, revierte — liquidez, no pérdida.
        if tail_share > U512::zero() {
            let pool_addr = match self.mutual_pool.get() {
                Some(addr) => addr,
                None => self.env().revert(Error::NotInitialized),
            };
            let mut pool = crate::mutual_pool::MutualPoolContractRef::new(self.env(), pool_addr);
            pool.pay_tail(caller, tail_share);
        }

        self.env().emit_event(SettlementWithdrawn {
            lote_id,
            buyer: caller,
            refund: share,
            indemnity,
            amount: amount_total,
        });
    }

    // ── Getters de atestación (S3) ──

    /// ¿La atestación para `(lote_id, signer)` ya fue registrada?
    pub fn attestation_recorded(&self, lote_id: u64, signer: Address) -> bool {
        self.attestation_recorded.get_or_default(&(lote_id, signer))
    }

    /// `chain_id` fijado en init (domain separation, fix #4).
    pub fn chain_id(&self) -> u64 {
        self.chain_id.get_or_revert_with(Error::NotInitialized)
    }

    /// Suma de shares de firmantes que atestaron NO-recibido para el lote (W2-0).
    pub fn lote_tally_negative(&self, lote_id: u64) -> U512 {
        self.lote_tally_negative.get_or_default(&lote_id)
    }

    /// Suma de shares de firmantes que atestaron SÍ-recibido para el lote (W2-0).
    pub fn lote_tally_positive(&self, lote_id: u64) -> U512 {
        self.lote_tally_positive.get_or_default(&lote_id)
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

    /// Umbral de no-recepción en basis points (W2-1).
    pub fn quorum_fail_bps(&self) -> u64 {
        self.quorum_fail_bps
            .get_or_revert_with(Error::NotInitialized)
    }

    /// Ventana de atestación en milisegundos (W2-1).
    pub fn attestation_window_ms(&self) -> u64 {
        self.attestation_window_ms
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

    // ── W1-1: getters de lote ──

    /// Estado del lote: 0=inexistente, 1=OPEN, 2=FUNDED, 3=SETTLED_OK.
    pub fn lote_state(&self, lote_id: u64) -> u8 {
        self.lote_state.get_or_default(&lote_id)
    }

    /// Productor asignado al lote (revert si no existe).
    pub fn lote_producer(&self, lote_id: u64) -> Address {
        match self.lote_producer.get(&lote_id) {
            Some(p) => p,
            None => self.env().revert(Error::LoteNotFound),
        }
    }

    /// Suma total de shares depositadas en el lote (0 si no existe).
    /// INV-7: nunca se deriva de `self_balance()`, es contabilidad por-lote.
    pub fn lote_funded(&self, lote_id: u64) -> U512 {
        self.lote_funded.get_or_default(&lote_id)
    }

    /// Share depositada por un comprador específico en este lote.
    /// Devuelve 0 si el comprador nunca depositó o el lote no existe.
    pub fn lote_share(&self, lote_id: u64, buyer: Address) -> U512 {
        self.lote_share.get_or_default(&(lote_id, buyer))
    }

    /// Bono depositado por el productor para este lote (0 si no existe).
    pub fn lote_bond(&self, lote_id: u64) -> U512 {
        self.lote_bond.get_or_default(&lote_id)
    }

    /// Suma tracked del escrow reservado para lotes activos (INV-7, FIX crítico).
    /// Los outflows genéricos (`route_micropayment`, `execute`) validan contra
    /// `self_balance() - reserved_lote_balance`.
    pub fn reserved_lote_balance(&self) -> U512 {
        self.reserved_lote_balance.get_or_default()
    }

    /// Timestamp (`get_block_time`) en que el lote pasó a FUNDED (W2-1).
    pub fn lote_funded_at(&self, lote_id: u64) -> u64 {
        self.lote_funded_at.get_or_default(&lote_id)
    }

    // ── W1-2: getters de settlement ──

    /// Aprobaciones distintas acumuladas para el release del lote.
    pub fn lote_release_approvals(&self, lote_id: u64) -> u8 {
        self.lote_release_approvals.get_or_default(&lote_id)
    }

    // ── W2-2: getters de settlement fail ──

    /// ¿El comprador ya reclamó su refund + indemnización de este lote fallido?
    pub fn lote_settlement_claimed(&self, lote_id: u64, buyer: Address) -> bool {
        self.lote_settlement_claimed.get_or_default(&(lote_id, buyer))
    }

    // ── W2-3: MutualPool setters (admin-only) ───────────────────────

    /// Configura la dirección del contrato `MutualPool`.
    /// Solo `admin`. Sin set por default (sin integración).
    pub fn set_mutual_pool(&mut self, addr: Address) {
        let admin = self.admin.get_or_revert_with(Error::NotInitialized);
        if self.env().caller() != admin {
            self.env().revert(Error::NotAdmin);
        }
        self.mutual_pool.set(addr);
    }

    /// Configura la prima en basis points (0–10000). Solo `admin`.
    /// 0 = sin prima (default).
    pub fn set_premium_bps(&mut self, bps: u64) {
        let admin = self.admin.get_or_revert_with(Error::NotInitialized);
        if self.env().caller() != admin {
            self.env().revert(Error::NotAdmin);
        }
        if bps > 10000 {
            self.env().revert(Error::InvalidBps);
        }
        self.premium_bps.set(bps);
    }

    /// Configura el target de indemnización en basis points (0–10000).
    /// Solo `admin`. 0 = sin cola de mutual (default).
    pub fn set_indemnity_target_bps(&mut self, bps: u64) {
        let admin = self.admin.get_or_revert_with(Error::NotInitialized);
        if self.env().caller() != admin {
            self.env().revert(Error::NotAdmin);
        }
        if bps > 10000 {
            self.env().revert(Error::InvalidBps);
        }
        self.indemnity_target_bps.set(bps);
    }

    // ── W2-3: MutualPool getters ────────────────────────────────────

    /// Dirección del `MutualPool` configurado (revert si no se ha seteado).
    pub fn mutual_pool_addr(&self) -> Address {
        match self.mutual_pool.get() {
            Some(addr) => addr,
            None => self.env().revert(Error::NotInitialized),
        }
    }

    /// Prima en basis points (0 = sin prima).
    pub fn premium_bps(&self) -> u64 {
        self.premium_bps.get_or_default()
    }

    /// Target de indemnización en basis points (0 = sin cola).
    pub fn indemnity_target_bps(&self) -> u64 {
        self.indemnity_target_bps.get_or_default()
    }

    /// Pool de indemnización del lote (solo el bono; la cola está en `lote_tail`).
    pub fn lote_indemnity_pool(&self, lote_id: u64) -> U512 {
        self.lote_indemnity_pool.get_or_default(&lote_id)
    }

    /// Cola de indemnización del lote desde el MutualPool.
    /// 0 si no hay integración o el bono cubrió el target.
    pub fn lote_tail(&self, lote_id: u64) -> U512 {
        self.lote_tail.get_or_default(&lote_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AttestationRecorded, BondPosted, DepositedToLote, Deposit, Error, LoteFunded, LoteOpened,
        LOTE_STATE_EVAL_FAIL, LOTE_STATE_EVAL_OK,
        LOTE_STATE_FUNDED, LOTE_STATE_OPEN, LOTE_STATE_SETTLED_FAIL, LOTE_STATE_SETTLED_OK,
        LoteEvaluated, LoteSettledFail, SettlementWithdrawn,
        MicropaymentRouted, OhuVault, OhuVaultHostRef, OhuVaultInitArgs,
        ReleaseApproved, ReleasedToProducer, ReleaseProposed,
        WithdrawApproved, WithdrawExecuted, WithdrawProposed,
    };
    use crate::mutual_pool::{MutualPool, MutualPoolHostRef, MutualPoolInitArgs};
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
        chain_id: u64,
        #[allow(dead_code)]
        quorum_fail_bps: u64,
        attestation_window_ms: u64,
    }

    fn setup() -> Fixture {
        setup_with_chain(U512::from(ONE_CSPR), 2, U512::from(5 * ONE_CSPR), 3_600_000, 1, 6000, 86_400_000)
    }

    fn setup_with(cap: U512, required: u8, epoch_cap: U512, epoch_window_ms: u64) -> Fixture {
        setup_with_chain(cap, required, epoch_cap, epoch_window_ms, 1, 6000, 86_400_000)
    }

    fn setup_with_chain(cap: U512, required: u8, epoch_cap: U512, epoch_window_ms: u64, chain_id: u64, quorum_fail_bps: u64, attestation_window_ms: u64) -> Fixture {
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
                chain_id,
                quorum_fail_bps,
                attestation_window_ms,
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
            chain_id,
            quorum_fail_bps,
            attestation_window_ms,
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidEpochWindow.into());
    }

    #[test]
    fn init_reverts_when_zero_chain_id() {
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
                epoch_window_ms: 3_600_000,
                chain_id: 0,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
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
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 86_400_000,
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

    thread_local! {
        /// Contador para repartir cuentas de test FONDEADAS (índice 10..19) como
        /// firmantes-compradores distintos. Ver sign_attestation.
        static NEXT_BUYER_IDX: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    }

    /// Firma una atestación COMO una cuenta de test fondeada (firmante = comprador,
    /// requisito del gate NotABuyer) y devuelve `(secret_key, pk_bytes[32],
    /// sig_bytes[64], signer)`. La cuenta se reparte de `NEXT_BUYER_IDX` (10..19).
    fn sign_attestation(
        lote_id: u64,
        nonce: u64,
        received: bool,
        verifying_contract: Address,
        chain_id: u64,
        valid_before: u64,
    ) -> (SecretKey, [u8; 32], [u8; 64], Address) {
        // W2-0 fix: el firmante DEBE ser un comprador fondeado (el gate NotABuyer
        // exige lote_share > 0, lo que requiere depositar → necesita fondos).
        // odra-test fondea get_account(0..19) derivando la cuenta k del secret [k;32]
        // (ver odra_core::crypto::generate_key_pairs). El fixture usa los roles en
        // 0..9, así que los compradores usan 10..19. Un contador acotado a ese rango
        // da firmantes DISTINTOS y SIEMPRE fondeados (cargo test reusa threads, por eso
        // el módulo: un contador monótono se saldría de [10,19] a cuentas sin fondos).
        let idx = NEXT_BUYER_IDX.with(|c| {
            let i = c.get();
            c.set(i.wrapping_add(1));
            10u8 + (i % 10)
        });
        let secret_key = SecretKey::ed25519_from_bytes([idx; 32])
            .expect("ed25519 secret from [idx;32]");
        let public_key = PublicKey::from(&secret_key);
        let account_hash = public_key.to_account_hash();
        let signer = Address::Account(account_hash);

        let message = crate::attestation::build_attestation_message(
            lote_id, nonce, received, verifying_contract, chain_id, valid_before,
        );
        let signature = crypto::sign(&message, &secret_key, &public_key);

        let pk_bytes: [u8; 32] = Into::<Vec<u8>>::into(&public_key)
            .try_into()
            .expect("Ed25519 pk should be 32 bytes");
        let sig_bytes: [u8; 64] = Into::<Vec<u8>>::into(&signature)
            .try_into()
            .expect("Ed25519 sig should be 64 bytes");

        (secret_key, pk_bytes, sig_bytes, signer)
    }

    /// Firma una atestación con una clave existente.
    fn sign_second(
        sk: &SecretKey,
        lote_id: u64,
        nonce: u64,
        received: bool,
        verifying_contract: Address,
        chain_id: u64,
        valid_before: u64,
    ) -> [u8; 64] {
        let pk = PublicKey::from(sk);
        let msg = crate::attestation::build_attestation_message(
            lote_id, nonce, received, verifying_contract, chain_id, valid_before,
        );
        let sig = crypto::sign(&msg, sk, &pk);
        Into::<Vec<u8>>::into(&sig).try_into().unwrap()
    }

    /// Devuelve el `(verifying_contract, chain_id)` de la fixture actual.
    fn vault_domain(f: &Fixture) -> (Address, u64) {
        (f.contract.contract_address(), f.chain_id)
    }

    /// W2-0: asegura que `buyer` sea comprador del lote (necesario para el gate
    /// de autorización). Abre el lote si no existe y deposita `share` CSPR.
    fn ensure_buyer(f: &mut Fixture, lote_id: u64, buyer: Address, share_cspr: u64) {
        let producer = f.env.get_account(7);
        f.env.set_caller(f.admin);
        // Solo abre si el lote no existe todavía.
        if f.contract.lote_state(lote_id) == 0 {
            f.contract.open_lote(lote_id, producer);
        }
        f.env.set_caller(buyer);
        let result = f
            .contract
            .with_tokens(U512::from(share_cspr * ONE_CSPR))
            .try_deposit_to_lote(lote_id);
        // Si falla, mostramos el error para debug.
        if let Err(e) = result {
            panic!("ensure_buyer: deposit_to_lote failed for lote {} buyer {:?}: {:?}", lote_id, buyer, e);
        }
    }

    // ── Positivos ────────────────────────────────────────────────────

    #[test]
    fn attestation_valid_signature_succeeds_and_records() {
        let mut f = setup();
        let lote_id = 1u64;
        let nonce = 5u64;
        let received = true;
        let (vc_addr, chain_id) = vault_domain(&f);

        let (_sk, pk_bytes, sig_bytes, signer) =
            sign_attestation(lote_id, nonce, received, vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, lote_id, signer, 10);
        f.env.set_caller(f.operator);
        let result = f
            .contract
            .verify_attestation(lote_id, nonce, received, u64::MAX, pk_bytes, sig_bytes);

        assert!(result);
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
        let (vc_addr, chain_id) = vault_domain(&f);
        let (_sk, pk_bytes, sig_bytes, signer) =
            sign_attestation(2, 1, false, vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, 2, signer, 10);
        f.env.set_caller(f.operator);
        let ok = f
            .contract
            .verify_attestation(2, 1, false, u64::MAX, pk_bytes, sig_bytes);

        assert!(ok);
        assert!(f.contract.attestation_recorded(2, signer));
        assert!(f.env.emitted_event(
            &f.contract,
            AttestationRecorded {
                lote_id: 2,
                signer,
                received: false,
                nonce: 1,
            }
        ));
    }

    #[test]
    fn attestation_multiple_signers_same_lote_succeeds() {
        let mut f = setup();
        let lote_id = 1u64;
        let (vc_addr, chain_id) = vault_domain(&f);

        let (_sk1, pk1, sig1, signer1) =
            sign_attestation(lote_id, 1, true, vc_addr, chain_id, u64::MAX);
        ensure_buyer(&mut f, lote_id, signer1, 10);
        let (_sk2, pk2, sig2, signer2) =
            sign_attestation(lote_id, 1, true, vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, lote_id, signer2, 10);
        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(lote_id, 1, true, u64::MAX, pk1, sig1));
        assert!(f.contract.verify_attestation(lote_id, 1, true, u64::MAX, pk2, sig2));

        assert!(f.contract.attestation_recorded(lote_id, signer1));
        assert!(f.contract.attestation_recorded(lote_id, signer2));
    }

    /// Fix #3 mandatory: submit lote B before lote A → AMBOS pasan.
    #[test]
    fn attestation_submit_lote_b_before_lote_a_both_pass() {
        let mut f = setup();
        let (vc_addr, chain_id) = vault_domain(&f);
        let (sk, pk, _, signer) =
            sign_attestation(1, 1, true, vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, 1, signer, 10);
        ensure_buyer(&mut f, 2, signer, 10);
        // Firma ambos lotes.
        let sig_a = sign_second(&sk, 1, 1, true, vc_addr, chain_id, u64::MAX);
        let sig_b = sign_second(&sk, 2, 2, true, vc_addr, chain_id, u64::MAX);

        f.env.set_caller(f.operator);
        // Submit lote B (nonce=2) primero.
        assert!(f.contract.verify_attestation(2, 2, true, u64::MAX, pk, sig_b));
        // Luego lote A (nonce=1) — debe pasar porque el scope es (signer, lote_id).
        assert!(f.contract.verify_attestation(1, 1, true, u64::MAX, pk, sig_a));

        assert!(f.contract.attestation_recorded(1, signer));
        assert!(f.contract.attestation_recorded(2, signer));
    }

    /// Mismo signer, distinto lote, nonce arbitrario — OK (fix #3: no global monotonicity).
    #[test]
    fn attestation_same_signer_different_lote_succeeds() {
        let mut f = setup();
        let (vc_addr, chain_id) = vault_domain(&f);
        let (sk, pk, _, signer) =
            sign_attestation(1, 1, true, vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, 1, signer, 10);
        ensure_buyer(&mut f, 2, signer, 10);
        let sig1 = sign_second(&sk, 1, 1, true, vc_addr, chain_id, u64::MAX);
        let sig2 = sign_second(&sk, 2, 100, true, vc_addr, chain_id, u64::MAX);

        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(1, 1, true, u64::MAX, pk, sig1));
        assert!(f.contract.verify_attestation(2, 100, true, u64::MAX, pk, sig2));
    }

    #[test]
    fn attestation_same_nonce_different_signer_succeeds() {
        let mut f = setup();
        let (vc_addr, chain_id) = vault_domain(&f);
        let (_sk1, pk1, sig1, s1) =
            sign_attestation(1, 3, true, vc_addr, chain_id, u64::MAX);
        ensure_buyer(&mut f, 1, s1, 10);
        let (_sk2, pk2, sig2, s2) =
            sign_attestation(1, 3, true, vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, 1, s2, 10);
        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(1, 3, true, u64::MAX, pk1, sig1));
        assert!(f.contract.verify_attestation(1, 3, true, u64::MAX, pk2, sig2));
    }

    // ── Negativos ────────────────────────────────────────────────────

    #[test]
    fn attestation_manipulated_public_key_reverts() {
        let mut f = setup();
        let (vc_addr, chain_id) = vault_domain(&f);
        let (_, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, true, vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, 1, signer, 10);
        let mut bad_pk = pk_bytes;
        bad_pk[0] ^= 0xFF;

        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_verify_attestation(1, 1, true, u64::MAX, bad_pk, sig_bytes);

        assert!(result.is_err());
        assert!(!f.contract.attestation_recorded(1, signer));
    }

    #[test]
    fn attestation_manipulated_signature_reverts() {
        let mut f = setup();
        let (vc_addr, chain_id) = vault_domain(&f);
        let (_, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, true, vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, 1, signer, 10);
        let mut bad_sig = sig_bytes;
        bad_sig[10] ^= 0xFF;

        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_verify_attestation(1, 1, true, u64::MAX, pk_bytes, bad_sig);

        assert!(result.is_err());
        assert!(!f.contract.attestation_recorded(1, signer));
    }

    #[test]
    fn attestation_manipulated_received_payload_reverts() {
        let mut f = setup();
        let (vc_addr, chain_id) = vault_domain(&f);
        let (_, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, true, vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, 1, signer, 10);
        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_verify_attestation(1, 1, false, u64::MAX, pk_bytes, sig_bytes);

        assert!(result.is_err());
        assert!(!f.contract.attestation_recorded(1, signer));
    }

    #[test]
    fn attestation_manipulated_nonce_reverts() {
        let mut f = setup();
        let (vc_addr, chain_id) = vault_domain(&f);
        let (_, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 5, true, vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, 1, signer, 10);
        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_verify_attestation(1, 3, true, u64::MAX, pk_bytes, sig_bytes);

        assert!(result.is_err());
        assert!(!f.contract.attestation_recorded(1, signer));
    }

    /// Replay: mismo (lote, signer) → revierte (fix #3: attestation_recorded guard).
    #[test]
    fn attestation_replay_same_lote_same_signer_reverts() {
        let mut f = setup();
        let (vc_addr, chain_id) = vault_domain(&f);
        let (sk, pk, _, signer) =
            sign_attestation(1, 5, true, vc_addr, chain_id, u64::MAX);
        ensure_buyer(&mut f, 1, signer, 10);
        let sig = sign_second(&sk, 1, 5, true, vc_addr, chain_id, u64::MAX);

        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(1, 5, true, u64::MAX, pk, sig));

        // Replay exacto del mismo payload → attestation_recorded ya es true.
        let result = f
            .contract
            .try_verify_attestation(1, 5, true, u64::MAX, pk, sig);

        assert_eq!(
            result.unwrap_err(),
            Error::AttestationNonceAlreadyUsed.into()
        );
    }

    /// Fix #4 mandatory: firma construida con OTRA dirección de contrato → revierte.
    #[test]
    fn attestation_different_verifying_contract_reverts() {
        let mut f = setup();
        let (_, chain_id) = vault_domain(&f);
        // Construye un Address de contrato distinto al real.
        let fake_hash = [0xABu8; 32];
        let fake_vc_addr = Address::Contract(
            odra::casper_types::contracts::ContractPackageHash::new(fake_hash),
        );
        // Verifica que sea distinto del real.
        assert_ne!(f.contract.contract_address(), fake_vc_addr);

        // Firma con la dirección FALSA.
        let (_, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, true, fake_vc_addr, chain_id, u64::MAX);

        ensure_buyer(&mut f, 1, signer, 10);
        f.env.set_caller(f.operator);
        // El vault usa su dirección REAL en el mensaje, no `fake_vc_addr` → firma no coincide.
        let result = f
            .contract
            .try_verify_attestation(1, 1, true, u64::MAX, pk_bytes, sig_bytes);

        assert!(result.is_err());
    }

    /// Fix #4 mandatory: firma construida con OTRO chain_id → revierte.
    #[test]
    fn attestation_different_chain_id_reverts() {
        let mut f = setup_with_chain(
            U512::from(ONE_CSPR), 2, U512::from(5 * ONE_CSPR), 3_600_000, 999, 6000, 86_400_000,
        );
        let (vc_addr, _) = vault_domain(&f);
        // Firma con chain_id=1 (incorrecto).
        let (_, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, true, vc_addr, 1, u64::MAX);

        ensure_buyer(&mut f, 1, signer, 10);
        f.env.set_caller(f.operator);
        // El vault usa chain_id=999, pero la firma es sobre chain_id=1.
        let result = f
            .contract
            .try_verify_attestation(1, 1, true, u64::MAX, pk_bytes, sig_bytes);

        assert!(result.is_err());
        assert!(!f.contract.attestation_recorded(1, signer));
    }

    // ===============================================================
    // W2-0 — Atestación ponderada y autorizada (S3 #1 + S3 #2)
    // ===============================================================

    /// Un firmante sin lote_share en el lote → revierte NotABuyer (S3 #1).
    #[test]
    fn attestation_non_buyer_reverts() {
        let mut f = setup();
        let lote_id = 1u64;
        let (vc_addr, chain_id) = vault_domain(&f);
        let valid_before = u64::MAX;

        let (_sk, pk_bytes, sig_bytes, _signer) =
            sign_attestation(lote_id, 1, true, vc_addr, chain_id, valid_before);

        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_verify_attestation(lote_id, 1, true, valid_before, pk_bytes, sig_bytes);

        assert_eq!(result.unwrap_err(), Error::NotABuyer.into());
    }

    /// Expiry: valid_before <= now → revierte AttestationExpired (S3 #2).
    #[test]
    fn attestation_expired_reverts() {
        let mut f = setup();
        let lote_id = 1u64;
        let (vc_addr, chain_id) = vault_domain(&f);
        // Usamos un valid_before fijo en 100_000 ms.
        let valid_before = 100_000u64;

        let (_sk, pk_bytes, sig_bytes, signer) =
            sign_attestation(lote_id, 1, true, vc_addr, chain_id, valid_before);

        ensure_buyer(&mut f, lote_id, signer, 10);

        // Avanzar el reloj para que now >= valid_before.
        f.env.advance_block_time(valid_before);

        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_verify_attestation(lote_id, 1, true, valid_before, pk_bytes, sig_bytes);

        assert_eq!(result.unwrap_err(), Error::AttestationExpired.into());
    }

    /// Tally ponderado: dos compradores con shares distintas atestan
    /// received=false → lote_tally_negative = suma exacta de sus shares.
    /// Un tercero con received=true → lote_tally_positive = su share.
    #[test]
    fn attestation_weighted_tally_correct() {
        let mut f = setup();
        let lote_id = 1u64;
        let (vc_addr, chain_id) = vault_domain(&f);
        let valid_before = u64::MAX;

        // Comprador A: share = 3 CSPR, atesta NO-recibido.
        let (_sk_a, pk_a, sig_a, signer_a) =
            sign_attestation(lote_id, 1, false, vc_addr, chain_id, valid_before);
        ensure_buyer(&mut f, lote_id, signer_a, 3);

        // Comprador B: share = 7 CSPR, atesta NO-recibido.
        let (_sk_b, pk_b, sig_b, signer_b) =
            sign_attestation(lote_id, 2, false, vc_addr, chain_id, valid_before);
        ensure_buyer(&mut f, lote_id, signer_b, 7);

        // Comprador C: share = 5 CSPR, atesta SÍ-recibido.
        let (_sk_c, pk_c, sig_c, signer_c) =
            sign_attestation(lote_id, 3, true, vc_addr, chain_id, valid_before);
        ensure_buyer(&mut f, lote_id, signer_c, 5);

        f.env.set_caller(f.operator);

        // A atesta no-recibido.
        assert!(f.contract.verify_attestation(lote_id, 1, false, valid_before, pk_a, sig_a));
        // B atesta no-recibido.
        assert!(f.contract.verify_attestation(lote_id, 2, false, valid_before, pk_b, sig_b));
        // C atesta sí-recibido.
        assert!(f.contract.verify_attestation(lote_id, 3, true, valid_before, pk_c, sig_c));

        // Verificar tally.
        assert_eq!(
            f.contract.lote_tally_negative(lote_id),
            U512::from(10 * ONE_CSPR) // 3 + 7 = 10 CSPR
        );
        assert_eq!(
            f.contract.lote_tally_positive(lote_id),
            U512::from(5 * ONE_CSPR) // solo C
        );
    }

    /// Una atestación válida y a tiempo de un comprador → NO revierte y
    /// acumula su peso en el tally correspondiente.
    #[test]
    fn attestation_valid_accumulates_weight() {
        let mut f = setup();
        let lote_id = 1u64;
        let (vc_addr, chain_id) = vault_domain(&f);
        let valid_before = u64::MAX;

        let (_sk, pk_bytes, sig_bytes, signer) =
            sign_attestation(lote_id, 1, false, vc_addr, chain_id, valid_before);
        ensure_buyer(&mut f, lote_id, signer, 4);

        f.env.set_caller(f.operator);
        let result = f
            .contract
            .verify_attestation(lote_id, 1, false, valid_before, pk_bytes, sig_bytes);

        assert!(result);
        assert!(f.contract.attestation_recorded(lote_id, signer));
        assert_eq!(
            f.contract.lote_tally_negative(lote_id),
            U512::from(4 * ONE_CSPR)
        );
        assert_eq!(
            f.contract.lote_tally_positive(lote_id),
            U512::zero()
        );
    }

    // ===============================================================
    // W1-1 — Modelo de lote + escrow earmarked (INV-7)
    // ===============================================================

    fn simple_setup() -> Fixture {
        setup()
    }

    /// Abre un lote con el admin y devuelve el producer.
    fn open_lote(f: &mut Fixture, lote_id: u64, producer: Address) {
        f.env.set_caller(f.admin);
        f.contract.open_lote(lote_id, producer);
    }

    // ── Positivos: open_lote ────────────────────────────────────────

    #[test]
    fn open_lote_as_admin_succeeds() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        open_lote(&mut f, 1, producer);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_OPEN);
        assert_eq!(f.contract.lote_producer(1), producer);
        assert_eq!(f.contract.lote_funded(1), U512::zero());
        assert_eq!(f.contract.lote_bond(1), U512::zero());
        assert!(f.env.emitted_event(
            &f.contract,
            LoteOpened {
                lote_id: 1,
                producer,
            }
        ));
    }

    #[test]
    fn open_lote_as_operator_succeeds() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        f.env.set_caller(f.operator);
        f.contract.open_lote(2, producer);
        assert_eq!(f.contract.lote_state(2), LOTE_STATE_OPEN);
        assert_eq!(f.contract.lote_producer(2), producer);
    }

    #[test]
    fn open_multiple_lotes_different_ids_succeeds() {
        let mut f = simple_setup();
        let p0 = f.env.get_account(7);
        let p1 = f.env.get_account(8);
        open_lote(&mut f, 1, p0);
        open_lote(&mut f, 2, p1);
        assert_eq!(f.contract.lote_producer(1), p0);
        assert_eq!(f.contract.lote_producer(2), p1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_OPEN);
        assert_eq!(f.contract.lote_state(2), LOTE_STATE_OPEN);
    }

    // ── Negativos: open_lote ───────────────────────────────────────

    #[test]
    fn open_lote_by_approver_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        f.env.set_caller(f.approver0);
        let result = f.contract.try_open_lote(1, producer);
        assert_eq!(result.unwrap_err(), Error::NotAdminNorOperator.into());
    }

    #[test]
    fn open_lote_by_random_caller_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        f.env.set_caller(f.env.get_account(9));
        let result = f.contract.try_open_lote(1, producer);
        assert_eq!(result.unwrap_err(), Error::NotAdminNorOperator.into());
    }

    #[test]
    fn open_lote_duplicate_id_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        open_lote(&mut f, 1, producer);
        f.env.set_caller(f.admin);
        let result = f.contract.try_open_lote(1, f.env.get_account(8));
        assert_eq!(result.unwrap_err(), Error::LoteAlreadyExists.into());
        // El productor original sigue intacto.
        assert_eq!(f.contract.lote_producer(1), producer);
    }

    #[test]
    fn open_lote_contract_as_producer_reverts() {
        let mut f = simple_setup();
        let vault_addr = f.contract.contract_address();
        f.env.set_caller(f.admin);
        let result = f.contract.try_open_lote(1, vault_addr);
        assert_eq!(result.unwrap_err(), Error::NotAnAccount.into());
    }

    // ── Positivos: deposit_to_lote ─────────────────────────────────

    #[test]
    fn deposit_to_lote_records_share_and_emits_event() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        let amount = U512::from(3 * ONE_CSPR);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(buyer);
        f.contract.with_tokens(amount).deposit_to_lote(1);

        assert_eq!(f.contract.lote_share(1, buyer), amount);
        assert_eq!(f.contract.lote_funded(1), amount);
        assert!(f.env.emitted_event(
            &f.contract,
            DepositedToLote {
                lote_id: 1,
                buyer,
                amount,
            }
        ));
    }

    #[test]
    fn multiple_buyers_deposit_to_same_lote_accumulates() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let b0 = f.env.get_account(8);
        let b1 = f.env.get_account(9);
        let a0 = U512::from(2 * ONE_CSPR);
        let a1 = U512::from(3 * ONE_CSPR);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(b0);
        f.contract.with_tokens(a0).deposit_to_lote(1);
        f.env.set_caller(b1);
        f.contract.with_tokens(a1).deposit_to_lote(1);

        assert_eq!(f.contract.lote_share(1, b0), a0);
        assert_eq!(f.contract.lote_share(1, b1), a1);
        assert_eq!(f.contract.lote_funded(1), a0 + a1);
    }

    #[test]
    fn same_buyer_deposits_twice_accumulates_share() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        let a0 = U512::from(ONE_CSPR);
        let a1 = U512::from(ONE_CSPR);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(buyer);
        f.contract.with_tokens(a0).deposit_to_lote(1);
        f.contract.with_tokens(a1).deposit_to_lote(1);

        assert_eq!(f.contract.lote_share(1, buyer), a0 + a1);
        assert_eq!(f.contract.lote_funded(1), a0 + a1);
    }

    // ── Negativos: deposit_to_lote ─────────────────────────────────

    #[test]
    fn deposit_to_nonexistent_lote_reverts() {
        let f = simple_setup();
        let buyer = f.env.get_account(8);
        f.env.set_caller(buyer);
        let result = f
            .contract
            .with_tokens(U512::from(ONE_CSPR))
            .try_deposit_to_lote(99);
        assert_eq!(result.unwrap_err(), Error::LoteNotFound.into());
    }

    #[test]
    fn deposit_to_lote_zero_amount_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(buyer);
        let result = f.contract.with_tokens(U512::zero()).try_deposit_to_lote(1);
        assert_eq!(result.unwrap_err(), Error::ZeroAmount.into());
        assert_eq!(f.contract.lote_funded(1), U512::zero());
    }

    #[test]
    fn deposit_to_funded_lote_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        open_lote(&mut f, 1, producer);

        // Primero: buyer deposita
        f.env.set_caller(buyer);
        f.contract
            .with_tokens(U512::from(ONE_CSPR))
            .deposit_to_lote(1);
        // Luego: producer pone bono → transición a FUNDED
        f.env.set_caller(producer);
        f.contract
            .with_tokens(U512::from(5 * ONE_CSPR))
            .post_bond(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);

        // Ahora depositar a un lote FUNDED revierte.
        f.env.set_caller(buyer);
        let result = f
            .contract
            .with_tokens(U512::from(ONE_CSPR))
            .try_deposit_to_lote(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotOpen.into());
    }

    // ── Positivos: post_bond ───────────────────────────────────────

    #[test]
    fn post_bond_by_producer_succeeds() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let bond = U512::from(10 * ONE_CSPR);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(producer);
        f.contract.with_tokens(bond).post_bond(1);

        assert_eq!(f.contract.lote_bond(1), bond);
        assert!(f.env.emitted_event(
            &f.contract,
            BondPosted {
                lote_id: 1,
                producer,
                amount: bond,
            }
        ));
    }

    #[test]
    fn post_bond_transitions_to_funded_when_funded_gt_zero() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        open_lote(&mut f, 1, producer);

        // Deposita primero
        f.env.set_caller(buyer);
        f.contract
            .with_tokens(U512::from(2 * ONE_CSPR))
            .deposit_to_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_OPEN);

        // Bono cierra el lote → FUNDED
        f.env.set_caller(producer);
        f.contract
            .with_tokens(U512::from(5 * ONE_CSPR))
            .post_bond(1);

        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);
        assert!(f.env.emitted_event(&f.contract, LoteFunded { lote_id: 1 }));
    }

    #[test]
    fn deposit_after_bond_transitions_to_funded() {
        // Caso inverso: el bono llega primero, luego el depósito.
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        open_lote(&mut f, 1, producer);

        // Bono primero
        f.env.set_caller(producer);
        f.contract
            .with_tokens(U512::from(5 * ONE_CSPR))
            .post_bond(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_OPEN); // aún no fondeado

        // Depósito → transición
        f.env.set_caller(buyer);
        f.contract
            .with_tokens(U512::from(2 * ONE_CSPR))
            .deposit_to_lote(1);

        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);
        assert!(f.env.emitted_event(&f.contract, LoteFunded { lote_id: 1 }));
    }

    #[test]
    fn bond_without_deposits_does_not_transition() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(producer);
        f.contract
            .with_tokens(U512::from(5 * ONE_CSPR))
            .post_bond(1);

        // Sin depósitos, el lote sigue OPEN.
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_OPEN);
        assert_eq!(f.contract.lote_bond(1), U512::from(5 * ONE_CSPR));
    }

    // ── Negativos: post_bond ───────────────────────────────────────

    #[test]
    fn post_bond_by_non_producer_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(f.env.get_account(9));
        let result = f
            .contract
            .with_tokens(U512::from(5 * ONE_CSPR))
            .try_post_bond(1);
        assert_eq!(result.unwrap_err(), Error::NotProducer.into());
        assert_eq!(f.contract.lote_bond(1), U512::zero());
    }

    #[test]
    fn post_bond_twice_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(producer);
        f.contract
            .with_tokens(U512::from(5 * ONE_CSPR))
            .post_bond(1);

        // Segundo intento
        let result = f
            .contract
            .with_tokens(U512::from(3 * ONE_CSPR))
            .try_post_bond(1);
        assert_eq!(result.unwrap_err(), Error::BondAlreadyPosted.into());
        // El bono original no se alteró.
        assert_eq!(f.contract.lote_bond(1), U512::from(5 * ONE_CSPR));
    }

    #[test]
    fn post_bond_nonexistent_lote_reverts() {
        let f = simple_setup();
        let producer = f.env.get_account(7);
        f.env.set_caller(producer);
        let result = f
            .contract
            .with_tokens(U512::from(ONE_CSPR))
            .try_post_bond(99);
        assert_eq!(result.unwrap_err(), Error::LoteNotFound.into());
    }

    #[test]
    fn post_bond_zero_amount_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(producer);
        let result = f.contract.with_tokens(U512::zero()).try_post_bond(1);
        assert_eq!(result.unwrap_err(), Error::ZeroAmount.into());
    }

    // ── Getters ─────────────────────────────────────────────────────

    #[test]
    fn lote_getters_reflect_state_correctly() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        let amount = U512::from(4 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(buyer);
        f.contract.with_tokens(amount).deposit_to_lote(1);
        f.env.set_caller(producer);
        f.contract.with_tokens(bond).post_bond(1);

        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);
        assert_eq!(f.contract.lote_producer(1), producer);
        assert_eq!(f.contract.lote_funded(1), amount);
        assert_eq!(f.contract.lote_share(1, buyer), amount);
        assert_eq!(f.contract.lote_bond(1), bond);
    }

    #[test]
    fn lote_state_nonexistent_returns_zero() {
        let f = simple_setup();
        assert_eq!(f.contract.lote_state(99), 0);
    }

    #[test]
    fn lote_producer_nonexistent_reverts() {
        let f = simple_setup();
        // El getter de productor revierte si el lote no existe.
        let result = f.contract.try_lote_producer(99);
        assert_eq!(result.unwrap_err(), Error::LoteNotFound.into());
    }

    #[test]
    fn lote_share_nonexistent_returns_zero() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        open_lote(&mut f, 1, producer);
        // Comprador que nunca depositó → share = 0
        assert_eq!(f.contract.lote_share(1, buyer), U512::zero());
    }

    // ===============================================================
    // INV-7 — Aislamiento entre lotes
    // ===============================================================

    #[test]
    fn inv7_two_lotes_deposits_isolated() {
        let mut f = simple_setup();
        let p0 = f.env.get_account(7);
        let p1 = f.env.get_account(8);
        let buyer = f.env.get_account(9);
        let a0 = U512::from(3 * ONE_CSPR);
        let a1 = U512::from(5 * ONE_CSPR);
        open_lote(&mut f, 1, p0);
        open_lote(&mut f, 2, p1);

        // Deposita al lote 1
        f.env.set_caller(buyer);
        f.contract.with_tokens(a0).deposit_to_lote(1);
        // Deposita al lote 2 (mismo comprador, lote distinto)
        f.contract.with_tokens(a1).deposit_to_lote(2);

        // Contabilidad de cada lote independiente (INV-7)
        assert_eq!(f.contract.lote_funded(1), a0);
        assert_eq!(f.contract.lote_funded(2), a1);
        assert_eq!(f.contract.lote_share(1, buyer), a0);
        assert_eq!(f.contract.lote_share(2, buyer), a1);
        // Ningún lote se "llevó" el saldo del otro
        assert_ne!(f.contract.lote_funded(1), f.contract.lote_funded(2));
    }

    #[test]
    fn inv7_bond_isolated_between_lotes() {
        let mut f = simple_setup();
        let p0 = f.env.get_account(7);
        let p1 = f.env.get_account(8);
        let b0 = U512::from(10 * ONE_CSPR);
        let b1 = U512::from(20 * ONE_CSPR);
        open_lote(&mut f, 1, p0);
        open_lote(&mut f, 2, p1);

        f.env.set_caller(p0);
        f.contract.with_tokens(b0).post_bond(1);
        f.env.set_caller(p1);
        f.contract.with_tokens(b1).post_bond(2);

        assert_eq!(f.contract.lote_bond(1), b0);
        assert_eq!(f.contract.lote_bond(2), b1);
    }

    #[test]
    fn inv7_deposit_to_lote_a_does_not_affect_lote_b() {
        let mut f = simple_setup();
        let p0 = f.env.get_account(7);
        let p1 = f.env.get_account(8);
        let buyer = f.env.get_account(9);
        open_lote(&mut f, 1, p0);
        open_lote(&mut f, 2, p1);

        // Solo se deposita en lote 1
        f.env.set_caller(buyer);
        f.contract
            .with_tokens(U512::from(ONE_CSPR))
            .deposit_to_lote(1);

        // Lote 1 tiene fondeo; lote 2 sigue en 0
        assert_eq!(f.contract.lote_funded(1), U512::from(ONE_CSPR));
        assert_eq!(f.contract.lote_funded(2), U512::zero());
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_OPEN);
        assert_eq!(f.contract.lote_state(2), LOTE_STATE_OPEN);
    }

    #[test]
    fn inv7_lote_b_funding_does_not_trigger_lote_a_transition() {
        let mut f = simple_setup();
        let p0 = f.env.get_account(7);
        let p1 = f.env.get_account(8);
        let buyer = f.env.get_account(9);
        open_lote(&mut f, 1, p0);
        open_lote(&mut f, 2, p1);

        // Lote 1: tiene bono pero sin fondeo
        f.env.set_caller(p0);
        f.contract
            .with_tokens(U512::from(5 * ONE_CSPR))
            .post_bond(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_OPEN); // sin fondeo → sigue OPEN

        // Lote 2: se fondea y transiciona normalmente
        f.env.set_caller(buyer);
        f.contract
            .with_tokens(U512::from(2 * ONE_CSPR))
            .deposit_to_lote(2);
        f.env.set_caller(p1);
        f.contract
            .with_tokens(U512::from(5 * ONE_CSPR))
            .post_bond(2);
        assert_eq!(f.contract.lote_state(2), LOTE_STATE_FUNDED);

        // Lote 1 NO fue arrastrado a FUNDED por el evento del lote 2
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_OPEN);
    }

    #[test]
    fn inv7_share_query_returns_only_target_lote() {
        let mut f = simple_setup();
        let p0 = f.env.get_account(7);
        let p1 = f.env.get_account(8);
        let buyer = f.env.get_account(9);
        open_lote(&mut f, 1, p0);
        open_lote(&mut f, 2, p1);

        // El comprador deposita SOLO en lote 1
        f.env.set_caller(buyer);
        f.contract
            .with_tokens(U512::from(3 * ONE_CSPR))
            .deposit_to_lote(1);

        // Share en lote 2 del mismo comprador = 0 (INV-7: sin cruce)
        assert_eq!(
            f.contract.lote_share(1, buyer),
            U512::from(3 * ONE_CSPR)
        );
        assert_eq!(f.contract.lote_share(2, buyer), U512::zero());
    }

    #[test]
    fn inv7_three_lotes_fully_isolated() {
        let mut f = simple_setup();
        let p = [f.env.get_account(7), f.env.get_account(8), f.env.get_account(9)];
        let buyers: Vec<Address> = (10..13).map(|i| f.env.get_account(i)).collect();
        let amounts = [
            U512::from(ONE_CSPR),
            U512::from(2 * ONE_CSPR),
            U512::from(3 * ONE_CSPR),
        ];
        let bonds = [
            U512::from(10 * ONE_CSPR),
            U512::from(20 * ONE_CSPR),
            U512::from(30 * ONE_CSPR),
        ];

        for i in 0..3u64 {
            open_lote(&mut f, i + 1, p[i as usize]);
        }

        // Depósitos cruzados: cada comprador deposita a su lote
        for i in 0..3u64 {
            f.env.set_caller(buyers[i as usize]);
            f.contract.with_tokens(amounts[i as usize]).deposit_to_lote(i + 1);
        }

        // Bonos
        for i in 0..3u64 {
            f.env.set_caller(p[i as usize]);
            f.contract.with_tokens(bonds[i as usize]).post_bond(i + 1);
        }

        // Verifica aislamiento total
        for i in 0..3u64 {
            let id = i + 1;
            assert_eq!(f.contract.lote_funded(id), amounts[i as usize]);
            assert_eq!(f.contract.lote_bond(id), bonds[i as usize]);
            assert_eq!(f.contract.lote_state(id), LOTE_STATE_FUNDED);
            assert_eq!(
                f.contract.lote_share(id, buyers[i as usize]),
                amounts[i as usize]
            );
            // Compradores de otros lotes no tienen share aquí
            for j in 0..3u64 {
                if i != j {
                    assert_eq!(
                        f.contract.lote_share(id, buyers[j as usize]),
                        U512::zero(),
                        "lote {} no debe tener share de comprador del lote {}",
                        id,
                        j + 1
                    );
                }
            }
        }
    }

    #[test]
    fn inv7_lote_a_bond_does_not_count_towards_lote_b_funding() {
        let mut f = simple_setup();
        let p0 = f.env.get_account(7);
        let p1 = f.env.get_account(8);
        open_lote(&mut f, 1, p0);
        open_lote(&mut f, 2, p1);

        // Poner bono solo en lote 1
        f.env.set_caller(p0);
        f.contract
            .with_tokens(U512::from(5 * ONE_CSPR))
            .post_bond(1);

        // lote_bond del lote 2 sigue siendo 0
        assert_eq!(f.contract.lote_bond(2), U512::zero());

        // lote_funded del lote 2 no se ve afectado
        assert_eq!(f.contract.lote_funded(2), U512::zero());
    }

    #[test]
    fn inv7_self_balance_is_not_used_for_lote_accounting() {
        // INV-7 explícito: la contabilidad por-lote vive en Mappings,
        // NUNCA en self_balance().
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(buyer);
        f.contract
            .with_tokens(U512::from(3 * ONE_CSPR))
            .deposit_to_lote(1);

        // El balance global del purse incluye los 100 CSPR del setup + 3 del depósito.
        // La contabilidad del lote es independiente.
        let global_balance = f.env.balance_of(&f.contract);
        assert!(global_balance > f.contract.lote_funded(1));
        // funded del lote = exactamente lo depositado al lote, no el balance global.
        assert_eq!(
            f.contract.lote_funded(1),
            U512::from(3 * ONE_CSPR)
        );
    }

    // ===============================================================
    // W1-2 — Settlement happy-path (release_to_producer)
    // ===============================================================

    /// Fundea un lote completo con comprador + bono y lo retorna en estado FUNDED.
    /// Usa el admin para abrir, un comprador (acct 8) para depositar, el producer para el bono.
    fn fund_lote(f: &mut Fixture, lote_id: u64, producer: Address, funded_amount: U512, bond_amount: U512) {
        open_lote(f, lote_id, producer);
        let buyer = f.env.get_account(8);
        f.env.set_caller(buyer);
        f.contract.with_tokens(funded_amount).deposit_to_lote(lote_id);
        f.env.set_caller(producer);
        f.contract.with_tokens(bond_amount).post_bond(lote_id);
        assert_eq!(f.contract.lote_state(lote_id), LOTE_STATE_FUNDED);
    }

    // ── Positivos: happy-path ──────────────────────────────────────

    #[test]
    fn release_to_producer_after_evaluate_lote_succeeds() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(3 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);
        let vault_before = f.env.balance_of(&f.contract);

        // Cerrar ventana de atestación
        f.env.advance_block_time(f.attestation_window_ms + 1);

        // Evaluar lote: silencio total → EVAL_OK
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);

        let producer_before = f.env.balance_of(&producer);

        // Release (admin + EVAL_OK)
        f.env.set_caller(f.admin);
        f.contract.release_to_producer(1);

        // Estado → SETTLED_OK
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_SETTLED_OK);
        // Productor recibe funded + bond
        assert_eq!(f.env.balance_of(&producer), producer_before + funded + bond);
        assert_eq!(f.env.balance_of(&f.contract), vault_before - (funded + bond));
        // Evento
        assert!(f.env.emitted_event(
            &f.contract,
            ReleasedToProducer {
                lote_id: 1,
                producer,
                funded,
                bond,
            }
        ));
    }

    #[test]
    fn release_events_proposed_and_approved_emitted() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(2 * ONE_CSPR);
        let bond = U512::from(3 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        // Proponer
        f.env.set_caller(f.admin);
        f.contract.propose_release(1);
        assert!(f.env.emitted_event(
            &f.contract,
            ReleaseProposed { lote_id: 1, proposer: f.admin }
        ));

        // Aprobar
        f.env.set_caller(f.approver0);
        f.contract.approve_release(1);
        assert!(f.env.emitted_event(
            &f.contract,
            ReleaseApproved { lote_id: 1, approver: f.approver0, count: 1 }
        ));
        f.env.set_caller(f.approver1);
        f.contract.approve_release(1);
        assert!(f.env.emitted_event(
            &f.contract,
            ReleaseApproved { lote_id: 1, approver: f.approver1, count: 2 }
        ));
    }

    #[test]
    fn release_with_eval_ok_succeeds() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        // Cerrar ventana + evaluar
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);

        f.env.set_caller(f.admin);
        f.contract.release_to_producer(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_SETTLED_OK);
    }

    #[test]
    fn propose_release_as_approver_succeeds() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        // Un approver propone el release
        f.env.set_caller(f.approver0);
        f.contract.propose_release(1);
        assert!(f.env.emitted_event(
            &f.contract,
            ReleaseProposed { lote_id: 1, proposer: f.approver0 }
        ));
    }

    // ── Negativos: release sin evaluate_lote ────────────────────────
    // TODO(Sem2: repurpose como emergency override): los tests de gate M-de-N
    // originales se recolocan aquí. El release normal ya no exige M-de-N; el
    // gate es EVAL_OK.

    #[test]
    fn release_on_funded_before_evaluate_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(3 * ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.set_caller(f.admin);
        let result = f.contract.try_release_to_producer(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotReleasable.into());
        // No se movió capital, estado sigue FUNDED
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);
    }

    #[test]
    fn release_on_eval_fail_reverts() {
        let mut f = setup_with_chain(
            U512::from(ONE_CSPR), 2, U512::from(5 * ONE_CSPR), 3_600_000, 1,
            6000,  // quorum_fail_bps = 60%
            60_000, // ventana = 60s
        );
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        open_lote(&mut f, 1, producer);
        f.env.set_caller(buyer);
        f.contract.with_tokens(funded).deposit_to_lote(1);
        f.env.set_caller(producer);
        f.contract.with_tokens(bond).post_bond(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);

        // Sin atestaciones → silencio=recibido → evaluate_lote da EVAL_OK.
        f.env.advance_block_time(60_001);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);

        // Release sobre EVAL_OK sí procede.
        f.contract.release_to_producer(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_SETTLED_OK);

        // Segundo release revierte (ya no es EVAL_OK).
        let result = f.contract.try_release_to_producer(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotReleasable.into());
    }

    // ── Negativos: doble settlement ────────────────────────────────

    #[test]
    fn release_twice_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(3 * ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);

        f.env.set_caller(f.admin);
        f.contract.release_to_producer(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_SETTLED_OK);

        // Segundo intento → LoteNotReleasable (ya no es EVAL_OK)
        let result = f.contract.try_release_to_producer(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotReleasable.into());
    }

    // ── Negativos: liquidar lote no-FUNDED ─────────────────────────

    #[test]
    fn release_to_open_lote_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        open_lote(&mut f, 1, producer);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_OPEN);

        f.env.set_caller(f.admin);
        let result = f.contract.try_release_to_producer(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotReleasable.into());
    }

    #[test]
    fn propose_release_on_open_lote_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(f.admin);
        let result = f.contract.try_propose_release(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotFunded.into());
    }

    #[test]
    fn approve_release_on_open_lote_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        open_lote(&mut f, 1, producer);

        f.env.set_caller(f.approver0);
        let result = f.contract.try_approve_release(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotFunded.into());
    }

    #[test]
    fn propose_release_on_nonexistent_lote_reverts() {
        let mut f = simple_setup();

        f.env.set_caller(f.admin);
        let result = f.contract.try_propose_release(99);
        assert_eq!(result.unwrap_err(), Error::LoteNotFunded.into());
    }

    #[test]
    fn release_to_nonexistent_lote_reverts() {
        let mut f = simple_setup();

        f.env.set_caller(f.admin);
        let result = f.contract.try_release_to_producer(99);
        assert_eq!(result.unwrap_err(), Error::LoteNotReleasable.into());
    }

    // ── Negativos: operator no puede mover capital ──────────────────

    #[test]
    fn operator_cannot_propose_release_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.set_caller(f.operator);
        let result = f.contract.try_propose_release(1);
        assert_eq!(result.unwrap_err(), Error::NotApprover.into());
    }

    #[test]
    fn operator_cannot_approve_release_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.set_caller(f.admin);
        f.contract.propose_release(1);

        f.env.set_caller(f.operator);
        let result = f.contract.try_approve_release(1);
        assert_eq!(result.unwrap_err(), Error::NotApprover.into());
        assert_eq!(f.contract.lote_release_approvals(1), 0);
    }

    #[test]
    fn operator_cannot_execute_release_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        // Evaluar (EVAL_OK) para que solo falle por NotAdmin
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);

        f.env.set_caller(f.operator);
        let result = f.contract.try_release_to_producer(1);
        assert_eq!(result.unwrap_err(), Error::NotAdmin.into());
        // Estado sigue EVAL_OK, capital no se movió
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);
    }

    #[test]
    fn random_caller_cannot_release_to_producer_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        // Evaluar (EVAL_OK) para que solo falle por NotAdmin
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);

        let random = f.env.get_account(9);
        f.env.set_caller(random);
        let result = f.contract.try_release_to_producer(1);
        assert_eq!(result.unwrap_err(), Error::NotAdmin.into());
    }

    // ── INV-7: dos lotes FUNDED, liquidar L1 no altera L2 ──────────

    #[test]
    fn inv7_release_l1_pays_only_producer_l1_and_does_not_alter_l2() {
        let mut f = simple_setup();
        let p0 = f.env.get_account(7);
        let p1 = f.env.get_account(8);
        let _buyer = f.env.get_account(9);
        let funded0 = U512::from(3 * ONE_CSPR);
        let bond0 = U512::from(5 * ONE_CSPR);
        let funded1 = U512::from(4 * ONE_CSPR);
        let bond1 = U512::from(6 * ONE_CSPR);

        // Fundea ambos lotes
        fund_lote(&mut f, 1, p0, funded0, bond0);
        fund_lote(&mut f, 2, p1, funded1, bond1);

        let vault_before = f.env.balance_of(&f.contract);
        let p0_before = f.env.balance_of(&p0);
        let p1_before = f.env.balance_of(&p1);

        // Cerrar ventana y evaluar solo lote 1
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);

        // Ejecuta release de lote 1 (eval_ok + admin)
        f.env.set_caller(f.admin);
        f.contract.release_to_producer(1);

        // Lote 1: SETTLED_OK, productor 0 recibe funded0 + bond0
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_SETTLED_OK);
        assert_eq!(f.env.balance_of(&p0), p0_before + funded0 + bond0);

        // Lote 2: NO se altera
        assert_eq!(f.contract.lote_state(2), LOTE_STATE_FUNDED);
        assert_eq!(f.contract.lote_funded(2), funded1);
        assert_eq!(f.contract.lote_bond(2), bond1);
        assert_eq!(f.contract.lote_producer(2), p1);
        // Productor 1 NO recibe nada
        assert_eq!(f.env.balance_of(&p1), p1_before);
        // El vault solo perdió lo de L1
        assert_eq!(
            f.env.balance_of(&f.contract),
            vault_before - (funded0 + bond0)
        );
    }

    #[test]
    fn inv7_after_release_l1_l2_can_still_be_released_independently() {
        let mut f = simple_setup();
        let p0 = f.env.get_account(7);
        let p1 = f.env.get_account(8);
        let funded0 = U512::from(3 * ONE_CSPR);
        let bond0 = U512::from(ONE_CSPR);
        let funded1 = U512::from(2 * ONE_CSPR);
        let bond1 = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, p0, funded0, bond0);
        fund_lote(&mut f, 2, p1, funded1, bond1);

        let p1_before = f.env.balance_of(&p1);

        // Cerrar ventana y evaluar ambos
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        f.contract.evaluate_lote(2);

        // Release L1
        f.env.set_caller(f.admin);
        f.contract.release_to_producer(1);

        // Release L2
        f.env.set_caller(f.admin);
        f.contract.release_to_producer(2);

        assert_eq!(f.contract.lote_state(1), LOTE_STATE_SETTLED_OK);
        assert_eq!(f.contract.lote_state(2), LOTE_STATE_SETTLED_OK);
        assert_eq!(f.env.balance_of(&p1), p1_before + funded1 + bond1);
    }

    // ── Negativos: doble aprobación ────────────────────────────────

    #[test]
    fn approver_cannot_approve_release_twice_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.set_caller(f.admin);
        f.contract.propose_release(1);
        f.env.set_caller(f.approver0);
        f.contract.approve_release(1);

        // Mismo approver, 2da vez
        let result = f.contract.try_approve_release(1);
        assert_eq!(result.unwrap_err(), Error::AlreadyApproved.into());
        assert_eq!(f.contract.lote_release_approvals(1), 1);
    }

    // ── Negativos: idempotencia de proposal ────────────────────────

    #[test]
    fn propose_release_twice_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.set_caller(f.admin);
        f.contract.propose_release(1);

        let result = f.contract.try_propose_release(1);
        assert_eq!(result.unwrap_err(), Error::ReleaseAlreadyProposed.into());
    }

    #[test]
    fn approve_release_without_proposal_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        // Aprobar sin propuesta previa
        f.env.set_caller(f.approver0);
        let result = f.contract.try_approve_release(1);
        assert_eq!(result.unwrap_err(), Error::ReleaseNotProposed.into());
    }

    // ── Getter ─────────────────────────────────────────────────────

    #[test]
    fn lote_release_approvals_getter() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(ONE_CSPR);
        let bond = U512::from(ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        assert_eq!(f.contract.lote_release_approvals(1), 0);

        f.env.set_caller(f.admin);
        f.contract.propose_release(1);
        f.env.set_caller(f.approver0);
        f.contract.approve_release(1);
        assert_eq!(f.contract.lote_release_approvals(1), 1);

        f.env.set_caller(f.approver1);
        f.contract.approve_release(1);
        assert_eq!(f.contract.lote_release_approvals(1), 2);

        // Lote no existente → 0
        assert_eq!(f.contract.lote_release_approvals(99), 0);
    }

    // ===============================================================
    // FIX crítico: aislamiento de escrow earmarked (audit GPT-5.5)
    // Tests de raideo — el escrow de lote es INTOCABLE por outflows
    // genéricos (route_micropayment, execute).
    // ===============================================================

    /// FIX-raid: el operator NO puede gastar CSPR reservado para lotes vía route_micropayment.
    #[test]
    fn raid_route_micropayment_cannot_spend_lote_escrow() {
        let mut f = setup_with(
            U512::from(ONE_CSPR),              // cap por llamada = 1 CSPR
            2,
            U512::from(10 * ONE_CSPR),         // epoch_cap holgado
            3_600_000,
        );
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        let lote_amount = U512::from(50 * ONE_CSPR);

        // Deposita 50 CSPR a un lote → reserved_lote_balance sube a 50 CSPR.
        open_lote(&mut f, 1, producer);
        f.env.set_caller(buyer);
        f.contract.with_tokens(lote_amount).deposit_to_lote(1);

        // self_balance ≈ 150 CSPR (100 setup + 50 lote), free ≈ 100 CSPR.
        let balance = f.env.balance_of(&f.contract);
        let reserved = f.contract.reserved_lote_balance();
        assert_eq!(reserved, lote_amount);
        assert!(balance > reserved);

        // Un micropago de 2 CSPR cabe en self_balance pero NO en free (solo hay ~100 libres).
        // Esperamos revert: el vault tiene 150, free son 100, pero el cap por llamada es 1 CSPR,
        // así que un micropago de 1 CSPR (cap exacto) cabe en free.
        // → intentamos uno de 1 CSPR y luego probamos que el exceso revierte.

        // Primero probamos: un micropago de 1 CSPR contra el free sí debe pasar.
        f.env.set_caller(f.operator);
        f.contract
            .route_micropayment(f.recipient, U512::from(ONE_CSPR));

        // Ahora drenamos el saldo libre completamente con execute genérico.
        // Pero antes: sin lote, execute también es genérico, puede consumir free.
        // Drena los ~99 CSPR libres que quedan via M-de-N.
        let free_before = f.env.balance_of(&f.contract)
            .checked_sub(f.contract.reserved_lote_balance())
            .unwrap();
        f.env.set_caller(f.admin);
        let id = f.contract.propose_withdraw(f.recipient, free_before);
        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);
        f.env.set_caller(f.admin);
        f.contract.execute(id);

        // Ahora self_balance ≈ reserved (solo queda escrow de lote), free ≈ 0.
        // route_micropayment debe revertir porque no hay saldo libre.
        f.env.set_caller(f.operator);
        let result = f
            .contract
            .try_route_micropayment(f.recipient, U512::from(ONE_CSPR));
        assert_eq!(result.unwrap_err(), Error::InsufficientBalance.into());

        // Verifica que el escrow del lote sigue intacto.
        assert_eq!(f.contract.reserved_lote_balance(), lote_amount);
        assert_eq!(f.contract.lote_funded(1), lote_amount);
    }

    /// FIX-raid: el admin NO puede drenar escrow de lote vía execute genérico.
    #[test]
    fn raid_execute_cannot_drain_lote_escrow() {
        let mut f = setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        let lote_amount = U512::from(40 * ONE_CSPR);

        // Deposita a un lote → reserved sube.
        open_lote(&mut f, 1, producer);
        f.env.set_caller(buyer);
        f.contract.with_tokens(lote_amount).deposit_to_lote(1);

        let reserved = f.contract.reserved_lote_balance();
        assert_eq!(reserved, lote_amount);

        // Propone y aprueba un execute genérico por un monto que excede `free`.
        // self_balance = 140, reserved = 40, free = 100.
        // Ejecutar 120 debería revertir.
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(120 * ONE_CSPR));
        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);

        f.env.set_caller(f.admin);
        let result = f.contract.try_execute(id);
        assert_eq!(result.unwrap_err(), Error::InsufficientBalance.into());
        assert!(!f.contract.request_executed(id));

        // El lote sigue intacto.
        assert_eq!(f.contract.lote_funded(1), lote_amount);
        assert_eq!(f.contract.reserved_lote_balance(), lote_amount);
    }

    /// FIX-raid: un retiro pre-aprobado no puede consumir depósitos de lote posteriores.
    #[test]
    fn raid_preapproved_withdraw_cannot_consume_future_lote_deposits() {
        let mut f = setup_with(
            U512::from(ONE_CSPR),
            2,
            U512::from(10 * ONE_CSPR),
            3_600_000,
        );
        // Drena casi todo el vault para que quede con poco saldo libre.
        f.env.set_caller(f.admin);
        let id_drain = f
            .contract
            .propose_withdraw(f.recipient, U512::from(95 * ONE_CSPR));
        f.env.set_caller(f.approver0);
        f.contract.approve(id_drain);
        f.env.set_caller(f.approver1);
        f.contract.approve(id_drain);
        f.env.set_caller(f.admin);
        f.contract.execute(id_drain);
        // Vault ≈ 5 CSPR libres.

        // Propone y aprueba un execute genérico de 3 CSPR (< 5 libre, ok por ahora).
        f.env.set_caller(f.admin);
        let id = f
            .contract
            .propose_withdraw(f.recipient, U512::from(3 * ONE_CSPR));
        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);

        // Luego un comprador deposita a un lote → reserved sube.
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        open_lote(&mut f, 1, producer);
        f.env.set_caller(buyer);
        f.contract
            .with_tokens(U512::from(10 * ONE_CSPR))
            .deposit_to_lote(1);

        // El execute genérico YA NO puede consumir esos 3 CSPR porque el balance
        // libre ahora es menor (parte está reservada para el lote).
        // self_balance ≈ 5 + 10 = 15, reserved = 10, free ≈ 5.
        // El retiro de 3 CSPR cabe en free → debería pasar.
        f.env.set_caller(f.admin);
        f.contract.execute(id);
        assert!(f.contract.request_executed(id));

        // Segundo retiro: ahora self_balance ≈ 12, reserved = 10, free ≈ 2.
        // Proponer 3 CSPR > free → revert en execute.
        f.env.set_caller(f.admin);
        let id2 = f
            .contract
            .propose_withdraw(f.recipient, U512::from(3 * ONE_CSPR));
        f.env.set_caller(f.approver0);
        f.contract.approve(id2);
        f.env.set_caller(f.approver1);
        f.contract.approve(id2);
        f.env.set_caller(f.admin);
        let result = f.contract.try_execute(id2);
        assert_eq!(result.unwrap_err(), Error::InsufficientBalance.into());

        // El escrow del lote sigue intacto.
        assert_eq!(f.contract.reserved_lote_balance(), U512::from(10 * ONE_CSPR));
        assert_eq!(f.contract.lote_funded(1), U512::from(10 * ONE_CSPR));
    }

    /// FIX-raid: reserved_lote_balance decrece correctamente tras release_to_producer.
    #[test]
    fn reserved_decreases_on_release() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(3 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        let reserved_before = f.contract.reserved_lote_balance();
        assert_eq!(reserved_before, funded + bond);

        // Cerrar ventana + evaluar
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);

        // Release
        f.env.set_caller(f.admin);
        f.contract.release_to_producer(1);

        assert_eq!(f.contract.lote_state(1), LOTE_STATE_SETTLED_OK);
        let reserved_after = f.contract.reserved_lote_balance();
        assert_eq!(reserved_after, reserved_before - (funded + bond));
        // Si no hay más lotes, reserved debe ser cero.
        assert_eq!(reserved_after, U512::zero());
    }

    /// FIX-raid: dos lotes FUNDED, execute genérico no puede dejarlos sin respaldo (INV-7).
    #[test]
    fn inv7_two_lotes_generic_withdraw_cannot_break_settlement() {
        let mut f = simple_setup();
        let p0 = f.env.get_account(7);
        let p1 = f.env.get_account(8);
        let _buyer = f.env.get_account(9);
        let funded0 = U512::from(30 * ONE_CSPR);
        let bond0 = U512::from(5 * ONE_CSPR);
        let funded1 = U512::from(40 * ONE_CSPR);
        let bond1 = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, p0, funded0, bond0);
        fund_lote(&mut f, 2, p1, funded1, bond1);

        let reserved = f.contract.reserved_lote_balance();
        assert_eq!(reserved, funded0 + bond0 + funded1 + bond1);

        // self_balance > reserved porque el setup puso 100 CSPR + depósitos de lote.
        // free = self_balance - reserved.
        // Intentamos un execute genérico que dejaría el vault sin respaldo para uno de los lotes.
        // free debe ser >= amount para que pase.
        let balance = f.env.balance_of(&f.contract);
        let free = balance.checked_sub(reserved).unwrap();
        assert!(free > U512::zero());

        // Ejecutar un retiro genérico por `free` — los dos lotes quedan respaldados
        // (self_balance después = reserved exacto).
        f.env.set_caller(f.admin);
        let id = f.contract.propose_withdraw(f.recipient, free);
        f.env.set_caller(f.approver0);
        f.contract.approve(id);
        f.env.set_caller(f.approver1);
        f.contract.approve(id);
        f.env.set_caller(f.admin);
        f.contract.execute(id);

        // Ahora self_balance == reserved; execute genérico adicional revierte.
        f.env.set_caller(f.admin);
        let id2 = f
            .contract
            .propose_withdraw(f.recipient, U512::from(ONE_CSPR));
        f.env.set_caller(f.approver0);
        f.contract.approve(id2);
        f.env.set_caller(f.approver1);
        f.contract.approve(id2);
        f.env.set_caller(f.admin);
        let result = f.contract.try_execute(id2);
        assert_eq!(result.unwrap_err(), Error::InsufficientBalance.into());

        // Ambos lotes siguen FUNDED y pueden liquidarse.
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);
        assert_eq!(f.contract.lote_state(2), LOTE_STATE_FUNDED);
        assert_eq!(f.contract.lote_funded(1), funded0);
        assert_eq!(f.contract.lote_bond(1), bond0);
        assert_eq!(f.contract.lote_funded(2), funded1);
        assert_eq!(f.contract.lote_bond(2), bond1);
    }

    /// FIX 4: el producer no puede ser el admin.
    #[test]
    fn open_lote_producer_cannot_be_admin() {
        let mut f = simple_setup();
        f.env.set_caller(f.admin);
        let result = f.contract.try_open_lote(1, f.admin);
        assert_eq!(result.unwrap_err(), Error::ProducerIsPrivileged.into());
    }

    /// FIX 4: el producer no puede ser el operator.
    #[test]
    fn open_lote_producer_cannot_be_operator() {
        let mut f = simple_setup();
        f.env.set_caller(f.admin);
        let result = f.contract.try_open_lote(1, f.operator);
        assert_eq!(result.unwrap_err(), Error::ProducerIsPrivileged.into());
    }

    /// FIX 4: el producer no puede ser un approver.
    #[test]
    fn open_lote_producer_cannot_be_approver() {
        let mut f = simple_setup();
        f.env.set_caller(f.admin);
        let result = f.contract.try_open_lote(1, f.approver0);
        assert_eq!(result.unwrap_err(), Error::ProducerIsPrivileged.into());
    }

    /// FIX: reserved_lote_balance getter devuelve 0 antes de cualquier lote.
    #[test]
    fn reserved_lote_balance_starts_at_zero() {
        let f = simple_setup();
        assert_eq!(f.contract.reserved_lote_balance(), U512::zero());
    }

    // ===============================================================
    // W2-1 — Disparador paramétrico (evaluate_lote + release gate)
    // ===============================================================

    /// Silencio total (cero atestaciones) → EVAL_OK (silencio=recibido).
    #[test]
    fn evaluate_lote_silence_returns_eval_ok() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);

        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);
        assert!(f.env.emitted_event(
            &f.contract,
            LoteEvaluated {
                lote_id: 1,
                result: LOTE_STATE_EVAL_OK,
                negative: U512::zero(),
                funded,
            }
        ));
    }

    /// Borde exacto: neg = 59% de funded con quorum_fail_bps=60% → EVAL_OK.
    #[test]
    fn evaluate_lote_59pct_returns_eval_ok() {
        let mut f = simple_setup(); // quorum_fail_bps=6000 (=60%)
        let producer = f.env.get_account(7);
        let bond = U512::from(5 * ONE_CSPR);
        open_lote(&mut f, 1, producer);

        // Comprador A: 59 CSPR — atestará no-recibido (usa sign_attestation)
        let (vc_addr, chain_id) = vault_domain(&f);
        let (_sk, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, false, vc_addr, chain_id, u64::MAX);
        ensure_buyer(&mut f, 1, signer, 59);

        // Comprador B: 41 CSPR — silencio (no atesta → recibido por default)
        let buyer_silent = f.env.get_account(9);
        f.env.set_caller(buyer_silent);
        f.contract
            .with_tokens(U512::from(41 * ONE_CSPR))
            .deposit_to_lote(1);

        let funded = f.contract.lote_funded(1);
        assert_eq!(funded, U512::from(100 * ONE_CSPR));

        f.env.set_caller(producer);
        f.contract.with_tokens(bond).post_bond(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);

        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(1, 1, false, u64::MAX, pk_bytes, sig_bytes));

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);

        // 59/100 = 59% < 60% → EVAL_OK
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);
    }

    /// Borde exacto: neg = 60% de funded con quorum_fail_bps=60% → EVAL_FAIL.
    #[test]
    fn evaluate_lote_60pct_returns_eval_fail() {
        let mut f = simple_setup(); // quorum_fail_bps=6000 (=60%)
        let producer = f.env.get_account(7);
        let bond = U512::from(5 * ONE_CSPR);
        open_lote(&mut f, 1, producer);

        // Comprador A: 60 CSPR — atestará no-recibido (usa sign_attestation)
        let (vc_addr, chain_id) = vault_domain(&f);
        let (_sk, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, false, vc_addr, chain_id, u64::MAX);
        ensure_buyer(&mut f, 1, signer, 60);

        // Comprador B: 40 CSPR — silencio (no atesta → recibido por default)
        let buyer_silent = f.env.get_account(9);
        f.env.set_caller(buyer_silent);
        f.contract
            .with_tokens(U512::from(40 * ONE_CSPR))
            .deposit_to_lote(1);

        let funded = f.contract.lote_funded(1);
        assert_eq!(funded, U512::from(100 * ONE_CSPR));

        f.env.set_caller(producer);
        f.contract.with_tokens(bond).post_bond(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);

        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(1, 1, false, u64::MAX, pk_bytes, sig_bytes));

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);

        // 60/100 = 60% ≥ 60% → EVAL_FAIL
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_FAIL);
        assert!(f.env.emitted_event(
            &f.contract,
            LoteEvaluated {
                lote_id: 1,
                result: LOTE_STATE_EVAL_FAIL,
                negative: U512::from(60 * ONE_CSPR),
                funded,
            }
        ));
    }

    /// evaluate_lote cuando la ventana NO ha cerrado → WindowNotClosed.
    #[test]
    fn evaluate_lote_before_window_closes_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        // Avanzar menos que la ventana
        f.env.advance_block_time(1_000); // solo 1 segundo
        f.env.set_caller(f.admin);
        let result = f.contract.try_evaluate_lote(1);

        assert_eq!(result.unwrap_err(), Error::WindowNotClosed.into());
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);
    }

    /// evaluate_lote por caller no admin/operator → NotAdminNorOperator.
    #[test]
    fn evaluate_lote_by_non_privileged_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        let random = f.env.get_account(9);
        f.env.set_caller(random);
        let result = f.contract.try_evaluate_lote(1);

        assert_eq!(result.unwrap_err(), Error::NotAdminNorOperator.into());
    }

    /// evaluate_lote sobre lote que no existe → LoteNotFunded.
    #[test]
    fn evaluate_lote_nonexistent_reverts() {
        let mut f = simple_setup();

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        let result = f.contract.try_evaluate_lote(99);

        assert_eq!(result.unwrap_err(), Error::LoteNotFunded.into());
    }

    /// evaluate_lote sobre lote OPEN (no FUNDED) → LoteNotFunded.
    #[test]
    fn evaluate_lote_on_open_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        open_lote(&mut f, 1, producer);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        let result = f.contract.try_evaluate_lote(1);

        assert_eq!(result.unwrap_err(), Error::LoteNotFunded.into());
    }

    /// evaluate_lote por el operator también funciona (puede gatillarlo el agente).
    #[test]
    fn evaluate_lote_by_operator_succeeds() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.operator);
        f.contract.evaluate_lote(1);

        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);
    }

    /// evaluate_lote dos veces no revierte: la 2da evaluación encuentra
    /// el lote en EVAL_OK/EVAL_FAIL (no FUNDED) → LoteNotFunded.
    #[test]
    fn evaluate_lote_twice_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);

        // Segunda evaluación → ya no está FUNDED
        let result = f.contract.try_evaluate_lote(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotFunded.into());
    }

    /// Release sobre EVAL_OK con admin → paga al producer, estado SETTLED_OK (test integral).
    #[test]
    fn evaluate_then_release_full_flow() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        let vault_before = f.env.balance_of(&f.contract);
        let producer_before = f.env.balance_of(&producer);

        // Cerrar ventana + evaluar → EVAL_OK (silencio)
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);

        // Release
        f.contract.release_to_producer(1);

        assert_eq!(f.contract.lote_state(1), LOTE_STATE_SETTLED_OK);
        assert_eq!(f.env.balance_of(&producer), producer_before + funded + bond);
        assert_eq!(f.env.balance_of(&f.contract), vault_before - (funded + bond));
        assert!(f.env.emitted_event(
            &f.contract,
            ReleasedToProducer { lote_id: 1, producer, funded, bond }
        ));
    }

    /// Release sobre lote FUNDED sin evaluate → LoteNotReleasable.
    #[test]
    fn release_on_funded_without_evaluate_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.set_caller(f.admin);
        let result = f.contract.try_release_to_producer(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotReleasable.into());
    }

    /// Release sobre EVAL_FAIL → LoteNotReleasable (W2-2 implementara settle_failure).
    #[test]
    fn release_on_eval_fail_reverts_lote_not_releasable() {
        let mut f = simple_setup(); // quorum_fail_bps = 60%
        let producer = f.env.get_account(7);
        let bond = U512::from(5 * ONE_CSPR);
        open_lote(&mut f, 1, producer);

        // 65 CSPR de no-recibido (65% > 60%) → EVAL_FAIL
        let (vc_addr, chain_id) = vault_domain(&f);
        let (_sk, pk_bytes, sig_bytes, signer) =
            sign_attestation(1, 1, false, vc_addr, chain_id, u64::MAX);
        ensure_buyer(&mut f, 1, signer, 65);
        let buyer_silent = f.env.get_account(9);
        f.env.set_caller(buyer_silent);
        f.contract
            .with_tokens(U512::from(35 * ONE_CSPR))
            .deposit_to_lote(1);

        f.env.set_caller(producer);
        f.contract.with_tokens(bond).post_bond(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);

        f.env.set_caller(f.operator);
        assert!(f.contract.verify_attestation(1, 1, false, u64::MAX, pk_bytes, sig_bytes));

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_FAIL);

        // Release sobre EVAL_FAIL → revierte
        f.env.set_caller(f.admin);
        let result = f.contract.try_release_to_producer(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotReleasable.into());
    }

    /// lote_funded_at se registra correctamente al transicionar a FUNDED.
    #[test]
    fn lote_funded_at_is_set_on_transition() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        open_lote(&mut f, 1, producer);

        f.env.advance_block_time(1_000);
        f.env.set_caller(buyer);
        f.contract
            .with_tokens(U512::from(ONE_CSPR))
            .deposit_to_lote(1);
        f.env.advance_block_time(2_000);
        f.env.set_caller(producer);
        f.contract
            .with_tokens(U512::from(ONE_CSPR))
            .post_bond(1);

        let funded_at = f.contract.lote_funded_at(1);
        assert!(funded_at >= 2_000, "funded_at should be at least 2000, got {}", funded_at);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_FUNDED);
    }

    /// validate init: quorum_fail_bps=0 → InvalidSetup.
    #[test]
    fn init_reverts_when_zero_quorum_fail_bps() {
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
                epoch_window_ms: 3_600_000,
                chain_id: 1,
                quorum_fail_bps: 0,
                attestation_window_ms: 86_400_000,
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    /// validate init: quorum_fail_bps > 10000 → InvalidSetup.
    #[test]
    fn init_reverts_when_quorum_fail_bps_exceeds_max() {
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
                epoch_window_ms: 3_600_000,
                chain_id: 1,
                quorum_fail_bps: 10001,
                attestation_window_ms: 86_400_000,
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    /// validate init: attestation_window_ms=0 → InvalidSetup.
    #[test]
    fn init_reverts_when_zero_attestation_window() {
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
                epoch_window_ms: 3_600_000,
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms: 0,
            },
        );
        assert_eq!(result.err().unwrap(), Error::InvalidSetup.into());
    }

    /// Getters para los nuevos parámetros devuelven lo configurado en init.
    #[test]
    fn getters_new_params() {
        let f = setup_with_chain(
            U512::from(ONE_CSPR), 2, U512::from(5 * ONE_CSPR), 3_600_000, 1,
            7000, 120_000,
        );
        assert_eq!(f.contract.quorum_fail_bps(), 7000);
        assert_eq!(f.contract.attestation_window_ms(), 120_000);
    }

    // ===============================================================
    // W2-2 — SETTLED_FAIL (settle_failure + withdraw_settlement)
    // ===============================================================

    /// Flujo completo hasta EVAL_FAIL: open → deposit (2 buyers 60/40)
    /// → post_bond (50) → FUNDED → atestación negativa ≥60% → evaluate
    /// → EVAL_FAIL. Retorna (producer, [buyer_a, buyer_b], funded, bond).
    fn setup_eval_fail(f: &mut Fixture, lote_id: u64) -> (Address, Address, Address, U512, U512) {
        let producer = f.env.get_account(7);
        let bond = U512::from(50 * ONE_CSPR);

        open_lote(f, lote_id, producer);

        // Buyer A: 60 CSPR — atestará no-recibido (≥60% para disparar EVAL_FAIL)
        let (vc_addr, chain_id) = vault_domain(f);
        let (_sk_a, pk_a, sig_a, signer_a) =
            sign_attestation(lote_id, 1, false, vc_addr, chain_id, u64::MAX);
        ensure_buyer(f, lote_id, signer_a, 60);

        // Buyer B: 40 CSPR — silencio (recibido por default)
        let buyer_b = f.env.get_account(9);
        f.env.set_caller(buyer_b);
        f.contract
            .with_tokens(U512::from(40 * ONE_CSPR))
            .deposit_to_lote(lote_id);

        let funded = f.contract.lote_funded(lote_id);
        assert_eq!(funded, U512::from(100 * ONE_CSPR));

        f.env.set_caller(producer);
        f.contract.with_tokens(bond).post_bond(lote_id);
        assert_eq!(f.contract.lote_state(lote_id), LOTE_STATE_FUNDED);

        // Submit atestación negativa de A (60% → ≥ quorum 60%)
        f.env.set_caller(f.operator);
        f.contract.verify_attestation(lote_id, 1, false, u64::MAX, pk_a, sig_a);

        // Cerrar ventana + evaluar → EVAL_FAIL
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(lote_id);
        assert_eq!(f.contract.lote_state(lote_id), LOTE_STATE_EVAL_FAIL);

        (producer, signer_a, buyer_b, funded, bond)
    }

    // ── settle_failure: positivos ───────────────────────────────────

    #[test]
    fn settle_failure_by_admin_on_eval_fail_succeeds() {
        let mut f = simple_setup();
        let (producer, _a, _b, funded, bond) = setup_eval_fail(&mut f, 1);

        let vault_before = f.env.balance_of(&f.contract);
        let reserved_before = f.contract.reserved_lote_balance();

        f.env.set_caller(f.admin);
        f.contract.settle_failure(1);

        assert_eq!(f.contract.lote_state(1), LOTE_STATE_SETTLED_FAIL);
        // Los fondos NO se movieron.
        assert_eq!(f.env.balance_of(&f.contract), vault_before);
        assert_eq!(f.contract.reserved_lote_balance(), reserved_before);
        // Evento
        assert!(f.env.emitted_event(
            &f.contract,
            LoteSettledFail {
                lote_id: 1,
                funded,
                bond,
                producer,
            }
        ));
    }

    // ── settle_failure: negativos ──────────────────────────────────

    #[test]
    fn settle_failure_on_eval_ok_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        // Silencio total → EVAL_OK
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);

        f.env.set_caller(f.admin);
        let result = f.contract.try_settle_failure(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotFailable.into());
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);
    }

    #[test]
    fn settle_failure_on_funded_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.set_caller(f.admin);
        let result = f.contract.try_settle_failure(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotFailable.into());
    }

    #[test]
    fn settle_failure_by_non_admin_reverts() {
        let mut f = simple_setup();
        setup_eval_fail(&mut f, 1);

        f.env.set_caller(f.operator);
        let result = f.contract.try_settle_failure(1);
        assert_eq!(result.unwrap_err(), Error::NotAdmin.into());

        f.env.set_caller(f.approver0);
        let result = f.contract.try_settle_failure(1);
        assert_eq!(result.unwrap_err(), Error::NotAdmin.into());

        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_FAIL);
    }

    // ── withdraw_settlement: positivos ──────────────────────────────

    /// Dos compradores 60/40 de 100, bono 50.
    /// A: share=60, refund=60, indemnity=50*60/100=30, amount=90
    /// B: share=40, refund=40, indemnity=50*40/100=20, amount=60
    #[test]
    fn withdraw_settlement_exact_arithmetic() {
        let mut f = simple_setup();
        let (_producer, buyer_a, buyer_b, funded, bond) = setup_eval_fail(&mut f, 1);

        // settle_failure antes de reclamar
        f.env.set_caller(f.admin);
        f.contract.settle_failure(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_SETTLED_FAIL);

        let balance_before = f.env.balance_of(&f.contract);
        let reserved_before = f.contract.reserved_lote_balance();

        // Buyer A reclama
        let a_before = f.env.balance_of(&buyer_a);
        f.env.set_caller(buyer_a);
        f.contract.withdraw_settlement(1);
        assert!(f.contract.lote_settlement_claimed(1, buyer_a));

        let expected_a = U512::from(60 * ONE_CSPR) + U512::from(30 * ONE_CSPR);
        assert_eq!(f.env.balance_of(&buyer_a), a_before + expected_a);
        assert!(f.env.emitted_event(
            &f.contract,
            SettlementWithdrawn {
                lote_id: 1,
                buyer: buyer_a,
                refund: U512::from(60 * ONE_CSPR),
                indemnity: U512::from(30 * ONE_CSPR),
                amount: expected_a,
            }
        ));

        // Buyer B reclama
        let b_before = f.env.balance_of(&buyer_b);
        f.env.set_caller(buyer_b);
        f.contract.withdraw_settlement(1);
        assert!(f.contract.lote_settlement_claimed(1, buyer_b));

        let expected_b = U512::from(40 * ONE_CSPR) + U512::from(20 * ONE_CSPR);
        assert_eq!(f.env.balance_of(&buyer_b), b_before + expected_b);
        assert!(f.env.emitted_event(
            &f.contract,
            SettlementWithdrawn {
                lote_id: 1,
                buyer: buyer_b,
                refund: U512::from(40 * ONE_CSPR),
                indemnity: U512::from(20 * ONE_CSPR),
                amount: expected_b,
            }
        ));

        // Vault: se fue funded + bond (= 150) en total
        assert_eq!(
            f.env.balance_of(&f.contract),
            balance_before - (funded + bond)
        );
        // Reserved bajó exactamente en lo retirado (funded + bond).
        assert_eq!(
            f.contract.reserved_lote_balance(),
            reserved_before - (funded + bond)
        );
    }

    /// Propiedad semántica (observación audit Gemini W2-2): en un lote FALLIDO,
    /// un comprador que atestó POSITIVO (received=true) IGUAL recupera su refund +
    /// indemnización. El derecho a withdraw depende SOLO de lote_share, no del
    /// veredicto que firmó el comprador (cuando el lote falla colectivamente,
    /// todos recuperan su escrow).
    #[test]
    fn withdraw_settlement_positive_attestor_still_refunded() {
        let mut f = simple_setup();
        let lote_id = 1u64;
        let producer = f.env.get_account(7);
        let bond = U512::from(50 * ONE_CSPR);
        open_lote(&mut f, lote_id, producer);
        let (vc_addr, chain_id) = vault_domain(&f);

        // Buyer A: 60 CSPR, atesta NO-recibido (dispara EVAL_FAIL: 60% ≥ quorum 60%).
        let (_sk_a, pk_a, sig_a, signer_a) =
            sign_attestation(lote_id, 1, false, vc_addr, chain_id, u64::MAX);
        ensure_buyer(&mut f, lote_id, signer_a, 60);

        // Buyer C: 40 CSPR, atesta RECIBIDO (positivo) — el caso que faltaba.
        let (_sk_c, pk_c, sig_c, signer_c) =
            sign_attestation(lote_id, 2, true, vc_addr, chain_id, u64::MAX);
        ensure_buyer(&mut f, lote_id, signer_c, 40);

        f.env.set_caller(producer);
        f.contract.with_tokens(bond).post_bond(lote_id);

        // Ambas atestaciones on-chain (la positiva de C NO reduce el tally negativo).
        f.env.set_caller(f.operator);
        f.contract
            .verify_attestation(lote_id, 1, false, u64::MAX, pk_a, sig_a);
        f.contract
            .verify_attestation(lote_id, 2, true, u64::MAX, pk_c, sig_c);

        // Cerrar ventana → evaluar → EVAL_FAIL → settle.
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(lote_id);
        assert_eq!(f.contract.lote_state(lote_id), LOTE_STATE_EVAL_FAIL);
        f.contract.settle_failure(lote_id);

        // C (atestó POSITIVO) IGUAL reclama: refund 40 + indemnity 50*40/100 = 20 = 60.
        let c_before = f.env.balance_of(&signer_c);
        f.env.set_caller(signer_c);
        f.contract.withdraw_settlement(lote_id);
        assert!(f.contract.lote_settlement_claimed(lote_id, signer_c));
        let expected_c = U512::from(40 * ONE_CSPR) + U512::from(20 * ONE_CSPR);
        assert_eq!(f.env.balance_of(&signer_c), c_before + expected_c);
    }

    /// Verifica que reserved_lote_balance baja exactamente en cada
    /// withdraw individual, no de golpe.
    #[test]
    fn withdraw_settlement_reserved_decrements_per_withdraw() {
        let mut f = simple_setup();
        let (_producer, buyer_a, buyer_b, funded, bond) = setup_eval_fail(&mut f, 1);

        f.env.set_caller(f.admin);
        f.contract.settle_failure(1);

        let reserved_before = f.contract.reserved_lote_balance();
        assert_eq!(reserved_before, funded + bond);

        // Buyer A → reserved baja en 90
        f.env.set_caller(buyer_a);
        f.contract.withdraw_settlement(1);
        let expected_a = U512::from(90 * ONE_CSPR);
        assert_eq!(
            f.contract.reserved_lote_balance(),
            reserved_before - expected_a
        );

        // Buyer B → reserved baja en 60 más, total = 150 (funded + bond)
        f.env.set_caller(buyer_b);
        f.contract.withdraw_settlement(1);
        let expected_b = U512::from(60 * ONE_CSPR);
        assert_eq!(
            f.contract.reserved_lote_balance(),
            reserved_before - (expected_a + expected_b)
        );
    }

    // ── withdraw_settlement: negativos ──────────────────────────────

    #[test]
    fn withdraw_settlement_before_settle_failure_reverts() {
        let mut f = simple_setup();
        let (_producer, buyer_a, _b, _funded, _bond) = setup_eval_fail(&mut f, 1);

        f.env.set_caller(buyer_a);
        let result = f.contract.try_withdraw_settlement(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotSettledFail.into());
    }

    #[test]
    fn withdraw_settlement_double_claim_reverts() {
        let mut f = simple_setup();
        let (_producer, buyer_a, _b, _funded, _bond) = setup_eval_fail(&mut f, 1);

        f.env.set_caller(f.admin);
        f.contract.settle_failure(1);

        f.env.set_caller(buyer_a);
        f.contract.withdraw_settlement(1);

        let result = f.contract.try_withdraw_settlement(1);
        assert_eq!(result.unwrap_err(), Error::SettlementAlreadyClaimed.into());
    }

    #[test]
    fn withdraw_settlement_non_buyer_reverts() {
        let mut f = simple_setup();
        let (_producer, _a, _b, _funded, _bond) = setup_eval_fail(&mut f, 1);

        f.env.set_caller(f.admin);
        f.contract.settle_failure(1);

        let random = f.env.get_account(10);
        f.env.set_caller(random);
        let result = f.contract.try_withdraw_settlement(1);
        assert_eq!(result.unwrap_err(), Error::NotABuyer.into());
    }

    #[test]
    fn withdraw_settlement_on_eval_ok_reverts() {
        let mut f = simple_setup();
        let producer = f.env.get_account(7);
        let funded = U512::from(10 * ONE_CSPR);
        let bond = U512::from(5 * ONE_CSPR);
        fund_lote(&mut f, 1, producer, funded, bond);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.contract.evaluate_lote(1);
        assert_eq!(f.contract.lote_state(1), LOTE_STATE_EVAL_OK);

        let buyer = f.env.get_account(8);
        f.env.set_caller(buyer);
        let result = f.contract.try_withdraw_settlement(1);
        assert_eq!(result.unwrap_err(), Error::LoteNotSettledFail.into());
    }

    // ── INV-7: aislamiento entre lotes fallidos ─────────────────────

    #[test]
    fn inv7_two_failed_lotes_withdraw_isolated() {
        let mut f = simple_setup();

        // Lote 1: EVAL_FAIL con buyers A(60) + B(40), bond=50
        let (_p0, a1, b1, f1, bond1) = setup_eval_fail(&mut f, 1);
        // Lote 2: EVAL_FAIL con buyers C(60) + D(40), bond=50
        let (_p1, a2, b2, f2, bond2) = setup_eval_fail(&mut f, 2);

        // settle_failure ambos
        f.env.set_caller(f.admin);
        f.contract.settle_failure(1);
        f.contract.settle_failure(2);

        let reserved_before = f.contract.reserved_lote_balance();
        assert_eq!(reserved_before, f1 + bond1 + f2 + bond2);

        // Withdraw de Lote 1 A
        let a1_before = f.env.balance_of(&a1);
        f.env.set_caller(a1);
        f.contract.withdraw_settlement(1);
        let expected_a1 = U512::from(90 * ONE_CSPR);
        assert_eq!(f.env.balance_of(&a1), a1_before + expected_a1);

        // Lote 2 intacto
        assert_eq!(f.contract.lote_state(2), LOTE_STATE_SETTLED_FAIL);
        assert_eq!(f.contract.lote_funded(2), f2);
        assert_eq!(f.contract.lote_bond(2), bond2);

        // Withdraw de Lote 2 A — verifica que no roba fondos del lote 1
        let a2_before = f.env.balance_of(&a2);
        f.env.set_caller(a2);
        f.contract.withdraw_settlement(2);
        assert_eq!(f.env.balance_of(&a2), a2_before + expected_a1); // misma aritmética

        // Ambos lotes pueden liquidar sus buyers sin interferencia
        f.env.set_caller(b1);
        f.contract.withdraw_settlement(1);
        f.env.set_caller(b2);
        f.contract.withdraw_settlement(2);

        // reserved bajó exactamente en funded+bond de ambos lotes
        assert_eq!(
            f.contract.reserved_lote_balance(),
            reserved_before - (f1 + bond1 + f2 + bond2)
        );
    }

    #[test]
    fn inv7_lote_a_withdraw_does_not_alter_lote_b_reserved() {
        let mut f = simple_setup();

        let (_p0, a1, _b1, _f1, _bond1) = setup_eval_fail(&mut f, 1);
        let (_p1, _a2, _b2, _f2, _bond2) = setup_eval_fail(&mut f, 2);

        f.env.set_caller(f.admin);
        f.contract.settle_failure(1);
        f.contract.settle_failure(2);

        let reserved_before = f.contract.reserved_lote_balance();
        let lote1_reserved = U512::from(150 * ONE_CSPR); // 100 funded + 50 bond

        // Withdraw full de lote 1 buyer A (60 CSPR refund + 30 indemnity = 90)
        f.env.set_caller(a1);
        f.contract.withdraw_settlement(1);

        // Lote 2 sigue con su reserved intacto
        assert_eq!(
            f.contract.reserved_lote_balance(),
            reserved_before - U512::from(90 * ONE_CSPR)
        );
        // El remanente del lote 1 = 150 - 90 = 60 CSPR (lo de buyer B)
        // Lote 2 = 150 CSPR → total reserved = 60 + 150 = 210
        assert_eq!(
            f.contract.reserved_lote_balance(),
            lote1_reserved - U512::from(90 * ONE_CSPR) + U512::from(150 * ONE_CSPR)
        );
    }

    /// Opcional: la suma de todos los withdraws de un lote
    /// ≈ funded + bond (salvo dust de división entera).
    #[test]
    fn withdraw_settlement_total_approx_funded_plus_bond() {
        let mut f = simple_setup();
        let (_producer, buyer_a, buyer_b, funded, bond) = setup_eval_fail(&mut f, 1);

        f.env.set_caller(f.admin);
        f.contract.settle_failure(1);

        let vault_before = f.env.balance_of(&f.contract);

        f.env.set_caller(buyer_a);
        f.contract.withdraw_settlement(1);
        f.env.set_caller(buyer_b);
        f.contract.withdraw_settlement(1);

        let total_withdrawn = vault_before - f.env.balance_of(&f.contract);
        assert_eq!(total_withdrawn, funded + bond);
    }

    // ===============================================================
    // W2-3 — Integración MutualPool ↔ OhuVault
    // ===============================================================

    /// Fixture with both OhuVault and MutualPool deployed.
    struct IntegrationFixture {
        vault: OhuVaultHostRef,
        pool: MutualPoolHostRef,
        env: HostEnv,
        admin: Address,
        operator: Address,
        #[allow(dead_code)]
        approver0: Address,
        #[allow(dead_code)]
        approver1: Address,
        attestation_window_ms: u64,
    }

    fn setup_integration(
        premium_bps: u64,
        indemnity_target_bps: u64,
    ) -> IntegrationFixture {
        let env = odra_test::env();
        let admin = env.get_account(0);
        let operator = env.get_account(1);
        let approver0 = env.get_account(2);
        let approver1 = env.get_account(3);
        let attestation_window_ms = 86_400_000u64;

        // Deploy vault first
        let mut vault = OhuVault::deploy(
            &env,
            OhuVaultInitArgs {
                admin,
                operator,
                approvers: vec![approver0, approver1, env.get_account(4)],
                required_approvals: 2,
                micropayment_cap: U512::from(ONE_CSPR),
                epoch_cap: U512::from(100 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms,
            },
        );

        let vault_addr = vault.contract_address();

        // Deploy pool with authorized_vault = vault address
        let pool = MutualPool::deploy(
            &env,
            MutualPoolInitArgs {
                admin,
                authorized_vault: vault_addr,
            },
        );

        let pool_addr = pool.contract_address();

        // Configure vault to use the pool
        env.set_caller(admin);
        vault.set_mutual_pool(pool_addr);
        if premium_bps > 0 {
            vault.set_premium_bps(premium_bps);
        }
        if indemnity_target_bps > 0 {
            vault.set_indemnity_target_bps(indemnity_target_bps);
        }

        // Fund vault with 200 CSPR from depositor
        let depositor = env.get_account(5);
        env.set_caller(depositor);
        vault.with_tokens(U512::from(200 * ONE_CSPR)).deposit();

        IntegrationFixture {
            vault,
            pool,
            env,
            admin,
            operator,
            approver0,
            approver1,
            attestation_window_ms,
        }
    }

    /// Fundea un lote completo (open → deposit → bond → FUNDED).
    fn fund_lote_integration(
        f: &mut IntegrationFixture,
        lote_id: u64,
        producer: Address,
        buyer: Address,
        funded_amount: U512,
        bond_amount: U512,
    ) {
        f.env.set_caller(f.admin);
        f.vault.open_lote(lote_id, producer);
        f.env.set_caller(buyer);
        f.vault.with_tokens(funded_amount).deposit_to_lote(lote_id);
        f.env.set_caller(producer);
        f.vault.with_tokens(bond_amount).post_bond(lote_id);
        assert_eq!(f.vault.lote_state(lote_id), LOTE_STATE_FUNDED);
    }

    // ── W2-3a: premium en release feliz ──────────────────────────────

    #[test]
    fn integration_release_with_premium_deducts_and_capitalizes_pool() {
        let mut f = setup_integration(50, 0); // premium_bps=50 (0.5%), sin indemnity
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        let funded = U512::from(100 * ONE_CSPR);
        let bond = U512::from(10 * ONE_CSPR);

        fund_lote_integration(&mut f, 1, producer, buyer, funded, bond);

        let producer_before = f.env.balance_of(&producer);

        // evaluate → EVAL_OK → release
        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.vault.evaluate_lote(1);
        assert_eq!(f.vault.lote_state(1), LOTE_STATE_EVAL_OK);

        let pool_before = f.env.balance_of(&f.pool);

        f.env.set_caller(f.admin);
        f.vault.release_to_producer(1);

        assert_eq!(f.vault.lote_state(1), LOTE_STATE_SETTLED_OK);

        // Premium = 100 * 50 / 10000 = 0.5 CSPR
        let premium = U512::from(500_000_000u64); // 0.5 CSPR
        let expected_payout = funded.checked_add(bond).unwrap().checked_sub(premium).unwrap();
        assert_eq!(
            f.env.balance_of(&producer),
            producer_before + expected_payout
        );

        // MutualPool recibió la prima
        assert_eq!(f.pool.reserve(), pool_before + premium);
    }

    #[test]
    fn integration_release_with_zero_premium_bps_no_deduction() {
        // premium_bps=0 → same as W2-1/W2-2 behavior
        let mut f = setup_integration(0, 0);
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        let funded = U512::from(100 * ONE_CSPR);
        let bond = U512::from(10 * ONE_CSPR);

        fund_lote_integration(&mut f, 1, producer, buyer, funded, bond);

        let producer_before = f.env.balance_of(&producer);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.vault.evaluate_lote(1);
        f.env.set_caller(f.admin);
        f.vault.release_to_producer(1);

        assert_eq!(f.vault.lote_state(1), LOTE_STATE_SETTLED_OK);
        assert_eq!(f.pool.reserve(), U512::zero());
        assert_eq!(
            f.env.balance_of(&producer),
            producer_before + funded + bond
        );
    }

    // ── W2-3b: cola en settle_failure (flujo EVAL_FAIL manual) ──────

    /// Helper: set up a lote to EVAL_FAIL with a negative attestation.
    #[allow(clippy::too_many_arguments)]
    fn setup_eval_fail_integration(
        f: &mut IntegrationFixture,
        lote_id: u64,
        producer: Address,
        signer_a: Address,
        buyer_b: Address,
        share_a: U512,
        share_b: U512,
        bond: U512,
    ) {
        f.env.set_caller(f.admin);
        f.vault.open_lote(lote_id, producer);

        // Buyer A deposit
        f.env.set_caller(signer_a);
        f.vault.with_tokens(share_a).deposit_to_lote(lote_id);

        // Buyer B deposit (silent)
        f.env.set_caller(buyer_b);
        f.vault.with_tokens(share_b).deposit_to_lote(lote_id);

        // Producer bond
        f.env.set_caller(producer);
        f.vault.with_tokens(bond).post_bond(lote_id);
        assert_eq!(f.vault.lote_state(lote_id), LOTE_STATE_FUNDED);

        let funded = share_a.checked_add(share_b).unwrap();
        assert_eq!(f.vault.lote_funded(lote_id), funded);
    }

    #[test]
    fn integration_settle_failure_draws_tail_when_bond_short() {
        // indemnity_target_bps=8000 → target = 80% of funded
        // funded=100, bond=10, target=80, deficit=70
        let mut f = setup_integration(0, 8000);

        // Capitalizar el pool: 100 CSPR
        let funder = f.env.get_account(5);
        f.env.set_caller(funder);
        f.pool.with_tokens(U512::from(100 * ONE_CSPR)).collect_premium();
        let pool_before = f.pool.reserve();
        assert_eq!(pool_before, U512::from(100 * ONE_CSPR));

        let lote_id = 1u64;
        let producer = f.env.get_account(7);
        let (vc_addr, chain_id) = vault_domain_integration(&f);
        // Buyer A = signer, 60 CSPR, will attest negative
        let (_sk, pk, sig, signer_a) =
            sign_attestation(lote_id, 1, false, vc_addr, chain_id, u64::MAX);
        let buyer_b = f.env.get_account(9);
        let bond = U512::from(10 * ONE_CSPR);

        setup_eval_fail_integration(
            &mut f, lote_id, producer,
            signer_a, buyer_b,
            U512::from(60 * ONE_CSPR), U512::from(40 * ONE_CSPR),
            bond,
        );

        // Submit negative attestation (60% → EVAL_FAIL)
        f.env.set_caller(f.operator);
        f.vault.verify_attestation(lote_id, 1, false, u64::MAX, pk, sig);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.vault.evaluate_lote(lote_id);
        assert_eq!(f.vault.lote_state(lote_id), LOTE_STATE_EVAL_FAIL);

        let vault_balance_before = f.env.balance_of(&f.vault);
        let reserved_before = f.vault.reserved_lote_balance();

        // settle_failure — NO mueve fondos, NO toca el pool
        f.env.set_caller(f.admin);
        f.vault.settle_failure(lote_id);
        assert_eq!(f.vault.lote_state(lote_id), LOTE_STATE_SETTLED_FAIL);

        // target = 100 * 8000 / 10000 = 80 CSPR, deficit = 80 - 10 = 70
        let expected_tail = U512::from(70 * ONE_CSPR);
        // lote_indemnity_pool = bond only (no tail)
        assert_eq!(f.vault.lote_indemnity_pool(lote_id), bond);
        // lote_tail almacena la cola
        assert_eq!(f.vault.lote_tail(lote_id), expected_tail);
        // Pool NO se tocó en settle
        assert_eq!(f.pool.reserve(), pool_before);
        // Vault balance y reserved NO cambiaron
        assert_eq!(f.env.balance_of(&f.vault), vault_balance_before);
        assert_eq!(f.vault.reserved_lote_balance(), reserved_before);

        // Withdraw buyer A: recibe de DOS fuentes
        //  VAULT: refund=60 + bond_indemnity=10*60/100=6 = 66
        //  POOL: tail_share=70*60/100=42
        //  total = 66 + 42 = 108
        let buyer_before = f.env.balance_of(&signer_a);
        let pool_before_withdraw = f.pool.reserve();
        f.env.set_caller(signer_a);
        f.vault.withdraw_settlement(lote_id);

        let expected_vault_amount = U512::from(60 * ONE_CSPR) + U512::from(6 * ONE_CSPR); // 66
        let expected_tail_share = U512::from(42 * ONE_CSPR);
        let expected_total = expected_vault_amount + expected_tail_share; // 108
        assert_eq!(
            f.env.balance_of(&signer_a),
            buyer_before + expected_total
        );
        // Pool bajó SOLO en tail_share de este comprador (no el tail entero)
        assert_eq!(f.pool.reserve(), pool_before_withdraw - expected_tail_share);

        // Evento: indemnity = bond_indemnity + tail_share = 6 + 42 = 48
        assert!(f.env.emitted_event(
            &f.vault,
            SettlementWithdrawn {
                lote_id,
                buyer: signer_a,
                refund: U512::from(60 * ONE_CSPR),
                indemnity: U512::from(48 * ONE_CSPR),
                amount: expected_total,
            }
        ));
    }

    #[test]
    fn integration_settle_failure_no_tail_when_bond_covers_target() {
        let mut f = setup_integration(0, 500); // target=5%, bond=10 > target=5

        let lote_id = 1u64;
        let producer = f.env.get_account(7);
        let (vc_addr, chain_id) = vault_domain_integration(&f);
        let (_sk, pk, sig, signer_a) =
            sign_attestation(lote_id, 1, false, vc_addr, chain_id, u64::MAX);
        let buyer_b = f.env.get_account(9);
        let bond = U512::from(10 * ONE_CSPR);

        setup_eval_fail_integration(
            &mut f, lote_id, producer,
            signer_a, buyer_b,
            U512::from(60 * ONE_CSPR), U512::from(40 * ONE_CSPR),
            bond,
        );

        f.env.set_caller(f.operator);
        f.vault.verify_attestation(lote_id, 1, false, u64::MAX, pk, sig);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.vault.evaluate_lote(lote_id);
        assert_eq!(f.vault.lote_state(lote_id), LOTE_STATE_EVAL_FAIL);

        f.env.set_caller(f.admin);
        f.vault.settle_failure(lote_id);
        assert_eq!(f.vault.lote_state(lote_id), LOTE_STATE_SETTLED_FAIL);

        // No tail: bond >= target
        assert_eq!(f.vault.lote_indemnity_pool(lote_id), bond);
        assert_eq!(f.pool.reserve(), U512::zero());
    }

    #[test]
    fn integration_tail_bounded_by_pool_reserve() {
        let mut f = setup_integration(0, 8000); // target=80, bond=10, deficit=70

        // Pool solo tiene 5 CSPR
        let funder = f.env.get_account(5);
        f.env.set_caller(funder);
        f.pool.with_tokens(U512::from(5 * ONE_CSPR)).collect_premium();
        assert_eq!(f.pool.reserve(), U512::from(5 * ONE_CSPR));

        let lote_id = 1u64;
        let producer = f.env.get_account(7);
        let (vc_addr, chain_id) = vault_domain_integration(&f);
        let (_sk, pk, sig, signer_a) =
            sign_attestation(lote_id, 1, false, vc_addr, chain_id, u64::MAX);
        let buyer_b = f.env.get_account(9);
        let bond = U512::from(10 * ONE_CSPR);

        setup_eval_fail_integration(
            &mut f, lote_id, producer,
            signer_a, buyer_b,
            U512::from(60 * ONE_CSPR), U512::from(40 * ONE_CSPR),
            bond,
        );

        f.env.set_caller(f.operator);
        f.vault.verify_attestation(lote_id, 1, false, u64::MAX, pk, sig);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.vault.evaluate_lote(lote_id);
        f.env.set_caller(f.admin);
        f.vault.settle_failure(lote_id);

        // Tail = min(deficit=70, reserve=5) = 5
        // lote_indemnity_pool = bond only (10)
        // lote_tail = 5
        // Pool NO se tocó en settle
        assert_eq!(f.vault.lote_indemnity_pool(lote_id), bond);
        assert_eq!(f.vault.lote_tail(lote_id), U512::from(5 * ONE_CSPR));
        assert_eq!(f.pool.reserve(), U512::from(5 * ONE_CSPR));

        // Withdraw buyer A (60 CSPR):
        //  VAULT: refund=60 + bond*60/100=6 = 66
        //  POOL: tail*60/100 = 5*60/100 = 3
        let buyer_before = f.env.balance_of(&signer_a);
        f.env.set_caller(signer_a);
        f.vault.withdraw_settlement(lote_id);

        assert_eq!(
            f.env.balance_of(&signer_a),
            buyer_before + U512::from(69 * ONE_CSPR) // 66 + 3
        );
        // Pool: 5 - 3 = 2
        assert_eq!(f.pool.reserve(), U512::from(2 * ONE_CSPR));
    }

    #[test]
    fn integration_pay_tail_by_non_vault_call_reverts() {
        let mut f = setup_integration(0, 0);
        let recipient = f.env.get_account(5);

        let funder = f.env.get_account(5);
        f.env.set_caller(funder);
        f.pool.with_tokens(U512::from(10 * ONE_CSPR)).collect_premium();

        let random = f.env.get_account(9);
        f.env.set_caller(random);
        let result = f.pool.try_pay_tail(recipient, U512::from(ONE_CSPR));

        assert_eq!(
            result.unwrap_err(),
            crate::mutual_pool::Error::NotAuthorizedVault.into()
        );
        assert_eq!(f.pool.reserve(), U512::from(10 * ONE_CSPR));
    }

    // ── W2-3c: backward-compatibility ──────────────────────────────

    /// Variante REAL sin pool configurado: nunca se llama set_mutual_pool.
    fn setup_integration_no_pool() -> IntegrationFixture {
        let env = odra_test::env();
        let admin = env.get_account(0);
        let operator = env.get_account(1);
        let approver0 = env.get_account(2);
        let approver1 = env.get_account(3);
        let attestation_window_ms = 86_400_000u64;

        let vault = OhuVault::deploy(
            &env,
            OhuVaultInitArgs {
                admin,
                operator,
                approvers: vec![approver0, approver1, env.get_account(4)],
                required_approvals: 2,
                micropayment_cap: U512::from(ONE_CSPR),
                epoch_cap: U512::from(100 * ONE_CSPR),
                epoch_window_ms: 3_600_000,
                chain_id: 1,
                quorum_fail_bps: 6000,
                attestation_window_ms,
            },
        );

        let vault_addr = vault.contract_address();

        // Deploy pool but NEVER configure it in vault
        let pool = MutualPool::deploy(
            &env,
            MutualPoolInitArgs {
                admin,
                authorized_vault: vault_addr,
            },
        );

        // Fund vault with 200 CSPR from depositor
        let depositor = env.get_account(5);
        env.set_caller(depositor);
        vault.with_tokens(U512::from(200 * ONE_CSPR)).deposit();

        IntegrationFixture {
            vault,
            pool,
            env,
            admin,
            operator,
            approver0,
            approver1,
            attestation_window_ms,
        }
    }

    #[test]
    fn integration_backward_compat_release_without_mutual_pool_config() {
        // W2-2 behavior: no pool, no premium, release identical to W2-1.
        let mut f = setup_integration_no_pool();
        let producer = f.env.get_account(7);
        let buyer = f.env.get_account(8);
        let funded = U512::from(100 * ONE_CSPR);
        let bond = U512::from(10 * ONE_CSPR);

        fund_lote_integration(&mut f, 1, producer, buyer, funded, bond);

        let producer_before = f.env.balance_of(&producer);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.vault.evaluate_lote(1);
        f.env.set_caller(f.admin);
        f.vault.release_to_producer(1);

        // Full payout (no premium deduction) = W2-2 behavior
        assert_eq!(
            f.env.balance_of(&producer),
            producer_before + funded + bond
        );
        // Pool untouched (never linked)
        assert_eq!(f.pool.reserve(), U512::zero());
    }

    #[test]
    fn integration_backward_compat_settle_failure_without_mutual_pool_config() {
        // W2-2 behavior: no pool, no tail, settlement identical to W2-2.
        let mut f = setup_integration_no_pool();

        let lote_id = 1u64;
        let producer = f.env.get_account(7);
        let (vc_addr, chain_id) = vault_domain_integration(&f);
        let (_sk, pk, sig, signer_a) =
            sign_attestation(lote_id, 1, false, vc_addr, chain_id, u64::MAX);
        let buyer_b = f.env.get_account(9);
        let bond = U512::from(10 * ONE_CSPR);

        setup_eval_fail_integration(
            &mut f, lote_id, producer,
            signer_a, buyer_b,
            U512::from(60 * ONE_CSPR), U512::from(40 * ONE_CSPR),
            bond,
        );

        f.env.set_caller(f.operator);
        f.vault.verify_attestation(lote_id, 1, false, u64::MAX, pk, sig);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.vault.evaluate_lote(lote_id);
        assert_eq!(f.vault.lote_state(lote_id), LOTE_STATE_EVAL_FAIL);

        f.env.set_caller(f.admin);
        f.vault.settle_failure(lote_id);
        assert_eq!(f.vault.lote_state(lote_id), LOTE_STATE_SETTLED_FAIL);

        // No tail: pool not linked → lote_indemnity_pool = bond, lote_tail = 0
        assert_eq!(f.vault.lote_indemnity_pool(lote_id), bond);
        assert_eq!(f.vault.lote_tail(lote_id), U512::zero());
        // Pool untouched
        assert_eq!(f.pool.reserve(), U512::zero());

        // Buyer withdraw: indemnity = bond * share / funded (W2-2 behavior)
        let buyer_before = f.env.balance_of(&signer_a);
        f.env.set_caller(signer_a);
        f.vault.withdraw_settlement(lote_id);
        // refund=60, indemnity=10*60/100=6, amount=66
        let expected_indemnity = U512::from(6 * ONE_CSPR);
        assert_eq!(
            f.env.balance_of(&signer_a),
            buyer_before + U512::from(60 * ONE_CSPR) + expected_indemnity
        );
    }

    // ── W2-3d: dos lotes compitiendo por reserva del pool ───────────

    #[test]
    fn integration_two_failed_lotes_competing_for_pool_reserve() {
        // indemnity_target_bps=8000 → target=80% funded
        // Pool: solo 10 CSPR.
        // Lote 1: bond=10, deficit=70 → tail_l1 = min(70, 10) = 10
        // Lote 2: se abre DESPUÉS de que lote 1 ya fijó su tail;
        //   bond=10, deficit=70, pero pool reserve=10 → tail_l2 = 10 también
        //   (settle failure solo LEE la reserva, no la consume).
        // En los withdraws, ambos lotes compiten por la misma reserva:
        //   buyer A lote 1 (60%) → tail_share_l1 = 10*60/100=6 → pool: 4
        //   buyer A lote 2 (60%) → tail_share_l2 = 10*60/100=6 → pool: revierte? No, pool tiene 4 → intenta 6 → InsufficientReserve
        let mut f = setup_integration(0, 8000);

        // Capitalizar el pool: solo 10 CSPR
        let funder = f.env.get_account(5);
        f.env.set_caller(funder);
        f.pool.with_tokens(U512::from(10 * ONE_CSPR)).collect_premium();
        assert_eq!(f.pool.reserve(), U512::from(10 * ONE_CSPR));

        let producer = f.env.get_account(7);
        let bond = U512::from(10 * ONE_CSPR);
        let (vc_addr, chain_id) = vault_domain_integration(&f);

        // ── Lote 1 ──
        let l1_id = 1u64;
        let (_sk1, pk1, sig1, signer_a1) =
            sign_attestation(l1_id, 1, false, vc_addr, chain_id, u64::MAX);
        let buyer_b1 = f.env.get_account(9);
        setup_eval_fail_integration(
            &mut f, l1_id, producer,
            signer_a1, buyer_b1,
            U512::from(60 * ONE_CSPR), U512::from(40 * ONE_CSPR),
            bond,
        );
        f.env.set_caller(f.operator);
        f.vault.verify_attestation(l1_id, 1, false, u64::MAX, pk1, sig1);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.vault.evaluate_lote(l1_id);
        f.vault.settle_failure(l1_id);

        // Lote 1: lote_indemnity_pool=bond=10, lote_tail=10 (pool tenía 10)
        assert_eq!(f.vault.lote_indemnity_pool(l1_id), bond);
        assert_eq!(f.vault.lote_tail(l1_id), U512::from(10 * ONE_CSPR));
        // Pool NO bajó
        assert_eq!(f.pool.reserve(), U512::from(10 * ONE_CSPR));

        // ── Lote 2 ──
        let l2_id = 2u64;
        let (_sk2, pk2, sig2, signer_a2) =
            sign_attestation(l2_id, 1, false, vc_addr, chain_id, u64::MAX);
        let buyer_b2 = f.env.get_account(10);
        setup_eval_fail_integration(
            &mut f, l2_id, producer,
            signer_a2, buyer_b2,
            U512::from(60 * ONE_CSPR), U512::from(40 * ONE_CSPR),
            bond,
        );
        f.env.set_caller(f.operator);
        f.vault.verify_attestation(l2_id, 1, false, u64::MAX, pk2, sig2);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.vault.evaluate_lote(l2_id);
        f.vault.settle_failure(l2_id);

        // Lote 2: también tail=10 (pool aún tiene 10 en settle)
        assert_eq!(f.vault.lote_indemnity_pool(l2_id), bond);
        assert_eq!(f.vault.lote_tail(l2_id), U512::from(10 * ONE_CSPR));
        // Pool sigue en 10
        assert_eq!(f.pool.reserve(), U512::from(10 * ONE_CSPR));

        // ── Withdraw L1 buyer A (60 CSPR) ──
        let a1_before = f.env.balance_of(&signer_a1);
        f.env.set_caller(signer_a1);
        f.vault.withdraw_settlement(l1_id);
        // VAULT: refund=60 + bond*60/100=6 = 66
        // POOL: tail*60/100 = 10*60/100 = 6
        // total = 72
        assert_eq!(f.env.balance_of(&signer_a1), a1_before + U512::from(72 * ONE_CSPR));
        // Pool: 10 - 6 = 4
        assert_eq!(f.pool.reserve(), U512::from(4 * ONE_CSPR));

        // ── Withdraw L2 buyer A (60 CSPR) ──
        // tail_share = 10*60/100 = 6, pero pool solo tiene 4 → revierte InsufficientReserve
        f.env.set_caller(signer_a2);
        let result = f.vault.try_withdraw_settlement(l2_id);
        assert_eq!(
            result.unwrap_err(),
            crate::mutual_pool::Error::InsufficientReserve.into()
        );
        // El vault NO perdió fondos: ni vault balance ni reserved cambiaron por este revert
        // (El estado del claim no se marcó porque el revert deshace todo.)
        assert!(!f.vault.lote_settlement_claimed(l2_id, signer_a2));

        // ── Withdraw L2 buyer B (40 CSPR) desde el pool residual ──
        // tail_share = 10*40/100 = 4, pool tiene 4 → justo alcanza
        let b2_before = f.env.balance_of(&buyer_b2);
        f.env.set_caller(buyer_b2);
        f.vault.withdraw_settlement(l2_id);
        // VAULT: refund=40 + 10*40/100=4 = 44
        // POOL: 10*40/100=4
        assert_eq!(f.env.balance_of(&buyer_b2), b2_before + U512::from(48 * ONE_CSPR));
        assert_eq!(f.pool.reserve(), U512::zero());

        // ── L2 buyer A retry ahora revierte porque pool quedó vacío ──
        f.env.set_caller(signer_a2);
        let result2 = f.vault.try_withdraw_settlement(l2_id);
        assert_eq!(
            result2.unwrap_err(),
            crate::mutual_pool::Error::InsufficientReserve.into()
        );
    }

    /// Después de todos los withdraws de un lote con cola, self_balance(vault)
    /// >= reserved_lote_balance (el tail nunca entró al vault).
    #[test]
    fn integration_after_full_withdraw_reserved_leq_balance() {
        let mut f = setup_integration(0, 8000); // target=80%, funded=100, bond=10

        // Capitalizar el pool: 100 CSPR
        let funder = f.env.get_account(5);
        f.env.set_caller(funder);
        f.pool.with_tokens(U512::from(100 * ONE_CSPR)).collect_premium();

        let lote_id = 1u64;
        let producer = f.env.get_account(7);
        let (vc_addr, chain_id) = vault_domain_integration(&f);
        let (_sk, pk, sig, signer_a) =
            sign_attestation(lote_id, 1, false, vc_addr, chain_id, u64::MAX);
        let buyer_b = f.env.get_account(9);
        let bond = U512::from(10 * ONE_CSPR);

        setup_eval_fail_integration(
            &mut f, lote_id, producer,
            signer_a, buyer_b,
            U512::from(60 * ONE_CSPR), U512::from(40 * ONE_CSPR),
            bond,
        );

        f.env.set_caller(f.operator);
        f.vault.verify_attestation(lote_id, 1, false, u64::MAX, pk, sig);

        f.env.advance_block_time(f.attestation_window_ms + 1);
        f.env.set_caller(f.admin);
        f.vault.evaluate_lote(lote_id);
        f.vault.settle_failure(lote_id);

        // lote_tail=70, lote_indemnity_pool=10
        assert_eq!(f.vault.lote_tail(lote_id), U512::from(70 * ONE_CSPR));
        assert_eq!(f.vault.lote_indemnity_pool(lote_id), bond);

        // Withdraw buyer A
        f.env.set_caller(signer_a);
        f.vault.withdraw_settlement(lote_id);
        // Withdraw buyer B
        f.env.set_caller(buyer_b);
        f.vault.withdraw_settlement(lote_id);

        // Ambos reclamaron
        assert!(f.vault.lote_settlement_claimed(lote_id, signer_a));
        assert!(f.vault.lote_settlement_claimed(lote_id, buyer_b));

        // INVARIANTE: self_balance(vault) >= reserved_lote_balance
        let vault_balance = f.env.balance_of(&f.vault);
        let reserved = f.vault.reserved_lote_balance();
        assert!(
            vault_balance >= reserved,
            "self_balance({}) < reserved({})",
            vault_balance,
            reserved
        );
    }

    // ── Helpers for integration tests ───────────────────────────────

    fn vault_domain_integration(f: &IntegrationFixture) -> (Address, u64) {
        (f.vault.contract_address(), 1)
    }
}

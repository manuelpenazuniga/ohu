//! Atestaciones gasless (INV-5) — verificación Ed25519 on-chain + plan EIP-712.
//!
//! ## Ruta actual: Ed25519
//!
//! El firmante (comprador) produce una firma Ed25519 off-chain sobre el mensaje
//! `"OhuAttestation:" || lote_id || nonce || received || verifying_contract ||
//! chain_id || valid_before` (sin gas). El agente la retransmite on-chain llamando
//! [`OhuVault::verify_attestation`].
//!
//! La verificación on-chain usa `casper_types::crypto::verify` (pura, sin host
//! function) y deriva la identidad del firmante del `PublicKey → AccountHash`.
//!
//! ### Anti-replay (fix #3)
//!
//! Scoped a (signer, lote_id): una atestación por comprador por lote.
//! `nonce` va DENTRO del mensaje firmado (binding) pero NO se impone monotonicidad
//! global sobre el nonce. El replay se bloquea vía `attestation_recorded[(lote_id,
//! signer)]`. Un comprador que atesta en lote B no bloquea su atestación en lote A.
//!
//! ### Domain separation (fix #4)
//!
//! El mensaje incluye `verifyingContract` (la dirección del propio vault) y
//! `chain_id` (fijado por el deployer en init). Esto impide que una firma
//! válida en una instancia de OhuVault se reutilice en otra instancia o en
//! otra cadena.
//!
//! ## Ruta target: EIP-712 (verificación ECDSA/Secp256k1)
//!
//! El diseño EIP-712 sigue el patrón `permit` de `casper-ecosystem/casper-eip-712`:
//!
//! ```text
//! domain = EIP712Domain(
//!     name:              "OhuVault",
//!     version:           "1",
//!     chainId:           <testnet-chain-id>,
//!     verifyingContract: <vault-address>
//! )
//! struct Attestation {
//!     uint256 loteId;
//!     uint256 nonce;
//!     bool    received;
//! }
//! ```
//!
//! Los typehash están copiados del repo oficial:
//! - `DOMAIN_TYPEHASH` = keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)")
//! - `ATTESTATION_TYPEHASH` = keccak256("Attestation(uint256 loteId,uint256 nonce,bool received)")
//!
//! TODO(audit): migrar a EIP-712 completo cuando se confirme que
//! `casper_ecdsa_recover` está disponible vía Odra y que el crate
//! `casper-eip-712` (v1.2.0+) es compatible con Odra 2.8.2.
//! Ver <https://github.com/casper-ecosystem/casper-eip-712> y
//! <https://odra.dev/docs/>.

use odra::casper_types::crypto::{self, PublicKey, Signature};
use odra::casper_types::AsymmetricType;
use odra::prelude::*;

// ─── EIP-712 typehash (copiados de casper-ecosystem/casper-eip-712) ────────

/// `keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)")`
/// TODO(audit): verificar contra el repo oficial de casper-eip-712.
#[allow(dead_code)]
const EIP712_DOMAIN_TYPEHASH: [u8; 32] = [
    0x8b, 0x73, 0xc3, 0xc6, 0x9b, 0xb8, 0xfe, 0x3d, 0x51, 0x2e, 0xcc, 0x4c, 0xf7, 0x59, 0xcc, 0x79,
    0x23, 0x9f, 0x7b, 0x17, 0x9b, 0x0f, 0xfa, 0xca, 0xa9, 0xa7, 0x5d, 0x52, 0x2b, 0x39, 0x40, 0x0f,
];

/// `keccak256("Attestation(uint256 loteId,uint256 nonce,bool received)")`
/// TODO(audit): verificar contra el repo oficial de casper-eip-712.
#[allow(dead_code)]
const ATTESTATION_TYPEHASH: [u8; 32] = [
    0xac, 0x32, 0x30, 0x0b, 0x51, 0x35, 0x65, 0x6e, 0x5c, 0x22, 0x8b, 0x11, 0x8f, 0xc2, 0x45, 0x2f,
    0xe1, 0xe0, 0x27, 0x35, 0x9b, 0x53, 0x15, 0x05, 0x98, 0xa5, 0x36, 0x37, 0x9f, 0xbc, 0x5f, 0xee,
];

// ─── Ed25519 attestation (ruta activa) ─────────────────────────────────────

/// Largo de una clave pública Ed25519 (sin el tag de algoritmo).
pub const ED25519_PK_LEN: usize = PublicKey::ED25519_LENGTH; // 32

/// Largo de una firma Ed25519 (sin el tag de algoritmo).
pub const ED25519_SIG_LEN: usize = Signature::ED25519_LENGTH; // 64

/// Prefijo de dominio para el mensaje de atestación (anti-colisión).
const ATTESTATION_MSG_PREFIX: &[u8] = b"OhuAttestation:";

/// Errores específicos del módulo de atestación.
#[odra::odra_error]
pub enum AttestationError {
    /// Clave pública inválida (no se pudo decodificar como Ed25519).
    InvalidPublicKey = 30,
    /// Firma inválida (no se pudo decodificar como Ed25519).
    InvalidSignatureBytes = 31,
    /// La firma Ed25519 no es válida para este mensaje y clave pública.
    InvalidSignature = 32,
}

/// Evento emitido cuando se registra una atestación válida.
#[odra::event]
pub struct AttestationRecorded {
    pub lote_id: u64,
    pub signer: Address,
    pub received: bool,
    pub nonce: u64,
}

// ─── Funciones de utilidad (no mutan storage) ──────────────────────────────

/// Extrae el hash subyacente de 32 bytes de un `Address`.
///
/// TODO(audit): confirmar que `AccountHash::value()` y
/// `ContractPackageHash::value()` devuelven `HashAddr` ([u8; 32]) en
/// casper-types 6.1.0. Ver <https://docs.cspr.cloud/>.
fn address_hash_bytes(addr: &Address) -> [u8; 32] {
    match addr {
        Address::Account(hash) => hash.value(),
        Address::Contract(hash) => hash.value(),
    }
}

/// Construye el mensaje que el firmante debe firmar con su clave Ed25519.
///
/// Formato:
/// `"OhuAttestation:" || lote_id (BE u64) || nonce (BE u64) || received (1 byte)
///  || verifying_contract_hash (32 bytes) || chain_id (BE u64) || valid_before (BE u64)`
///
/// `verifying_contract` es la dirección del propio vault (self_address()).
/// `chain_id` lo fija el deployer en init y se guarda en el contrato.
/// `valid_before` es la marca de tiempo (ms, `get_block_time()`) tras la cual
/// la atestación expira (W2-0, S3 #2). Va DENTRO del mensaje firmado (binding).
/// Ambos se incluyen para prevenir replay cross-contract/cross-chain (fix #4).
pub fn build_attestation_message(
    lote_id: u64,
    nonce: u64,
    received: bool,
    verifying_contract: Address,
    chain_id: u64,
    valid_before: u64,
) -> Vec<u8> {
    let mut msg = Vec::from(ATTESTATION_MSG_PREFIX);
    msg.extend_from_slice(&lote_id.to_be_bytes());
    msg.extend_from_slice(&nonce.to_be_bytes());
    msg.push(received as u8);
    msg.extend_from_slice(&address_hash_bytes(&verifying_contract));
    msg.extend_from_slice(&chain_id.to_be_bytes());
    msg.extend_from_slice(&valid_before.to_be_bytes());
    msg
}

/// Verifica una firma Ed25519 y devuelve la dirección del firmante (AccountHash → Address).
///
/// # Argumentos
/// - `lote_id`, `nonce`, `received`: el payload de la atestación.
/// - `verifying_contract`: dirección del vault (incluida en el mensaje, fix #4).
/// - `chain_id`: identificador de cadena (incluido en el mensaje, fix #4).
/// - `valid_before`: marca de tiempo de expiración (ms, incluido en el mensaje, W2-0).
/// - `public_key_bytes`: 32 bytes de la clave pública Ed25519 (raw, sin tag).
/// - `signature_bytes`: 64 bytes de la firma Ed25519 (raw, sin tag).
///
/// # Retorna
/// - `Ok(signer)`: la dirección (AccountHash) del firmante derivada de la clave pública.
/// - `Err`: si la clave, firma o derivación fallan.
///
/// TODO(audit): cuando EIP-712 esté disponible, esta función se reemplazará
/// por `verify_eip712_attestation` que usará `recover_secp256k1` del crate
/// `casper-eip-712`. El mensaje a firmar será el digest EIP-712:
/// `keccak256("\x19\x01" || domainSeparator || hashStruct(attestation))`.
#[allow(clippy::too_many_arguments)]
pub fn verify_attestation_signature(
    lote_id: u64,
    nonce: u64,
    received: bool,
    verifying_contract: Address,
    chain_id: u64,
    valid_before: u64,
    public_key_bytes: [u8; ED25519_PK_LEN],
    signature_bytes: [u8; ED25519_SIG_LEN],
) -> Result<Address, AttestationError> {
    let public_key = PublicKey::ed25519_from_bytes(&public_key_bytes[..])
        .map_err(|_| AttestationError::InvalidPublicKey)?;

    let signature =
        Signature::ed25519(signature_bytes).map_err(|_| AttestationError::InvalidSignatureBytes)?;

    let message = build_attestation_message(
        lote_id,
        nonce,
        received,
        verifying_contract,
        chain_id,
        valid_before,
    );

    crypto::verify(&message, &signature, &public_key)
        .map_err(|_| AttestationError::InvalidSignature)?;

    let account_hash = public_key.to_account_hash();
    let signer = Address::Account(account_hash);

    Ok(signer)
}

/// EIP-712: construye el mensaje EIP-712 para la atestación (NO implementado — ruta target).
///
/// El digest EIP-712 se calcula como:
/// ```text
/// encodeType  = "Attestation(uint256 loteId,uint256 nonce,bool received)"
/// typeHash    = keccak256(encodeType)
/// encodeData  = abi.encode(typeHash, loteId, nonce, received ? 1 : 0)
/// hashStruct  = keccak256(encodeData)
/// domainSeparator = keccak256(
///     abi.encode(
///         EIP712_DOMAIN_TYPEHASH,
///         keccak256("OhuVault"),
///         keccak256("1"),
///         chainId,
///         verifyingContract
///     )
/// )
/// finalDigest = keccak256(abi.encodePacked("\x19\x01", domainSeparator, hashStruct))
/// ```
/// TODO(audit): implementar cuando `casper-eip-712` sea compatible con Odra 2.8.2.
/// Usar `hash_typed_data(domain, attestation)` del crate.
#[allow(dead_code)]
fn build_eip712_digest(
    _domain_separator: &[u8; 32],
    _lote_id: u64,
    _nonce: u64,
    _received: bool,
) -> [u8; 32] {
    // TODO(audit): implementar con hash_typed_data de casper-eip-712.
    // let attestation = Attestation { lote_id: _lote_id.into(), nonce: _nonce.into(), received: _received };
    // hash_typed_data(domain, &attestation)
    [0u8; 32]
}

//! Deterministic TLT covenant script (version 2 library).
//!
//! The live UTXO lane still spends address-locked v1 [`crate::Transaction`] values.
//! This module does not change that encoding. It defines a closed opcode set for
//! P2PKH-style, P2SH, M-of-N, absolute and relative timelocks, and hashlocks.
//! There is no loop, jump, or concatenation opcode, so scripts are not
//! Turing-complete.
//!
//! Opcode numbers are Agora TLT script v1 assignments. They are not a claim of
//! Bitcoin script byte compatibility. CHECKMULTISIG does not consume Bitcoin's
//! historical dummy stack element. Script integers are unsigned little-endian
//! with no sign bit.

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

use crate::{Address, Amount, Hash, OutPoint};

/// Covenant transactions use this version. v1 [`crate::Transaction`] stays version 1.
pub const TLT_COVENANT_TX_VERSION: u32 = 2;
/// Domain separator for covenant transaction identifiers.
pub const TLT_COVENANT_TX_DOMAIN: &[u8] = b"agora-tlt-covenant-v1";
const TLT_COVENANT_SIGHASH_DOMAIN: &[u8] = b"agora-tlt-covenant-sighash-v1";

/// Working script limits for the covenant library. Not ceremony-frozen, and not
/// copied from Bitcoin's script budget.
pub const TLT_MAX_SCRIPT_LEN: usize = 512;
pub const TLT_MAX_STACK: usize = 32;
pub const TLT_MAX_OPS: usize = 64;
pub const TLT_MAX_PUSH: usize = 80;
pub const TLT_MAX_MULTISIG: u8 = 15;
const TLT_MAX_IF_DEPTH: usize = 8;

/// Implicit v1 sequence, and the value that disables relative lock and RBF.
pub const TLT_SEQUENCE_FINAL: u32 = 0xFFFF_FFFF;
/// When set on an input sequence, CHECKSEQUENCEVERIFY fails.
pub const TLT_SEQUENCE_DISABLE_FLAG: u32 = 1 << 31;
/// When set, the masked sequence is a relative time lock. Otherwise it is blue score.
pub const TLT_SEQUENCE_TIME_FLAG: u32 = 1 << 22;
/// Low 16 bits are the relative-lock magnitude.
pub const TLT_SEQUENCE_LOCK_MASK: u32 = 0x0000_FFFF;
/// Relative time locks step by this many unix seconds.
///
/// Working parameter for the covenant library. Header timestamps are milliseconds
/// and must be converted to seconds before they are passed in.
pub const TLT_CSV_TIME_STEP_SECS: u64 = 512;
/// Locktimes below this value are blue scores. Locktimes at or above it are unix seconds.
pub const TLT_LOCKTIME_TIME_THRESHOLD: u64 = 500_000_000;

const OP_0: u8 = 0x00;
const OP_PUSH: u8 = 0x01;
const OP_1: u8 = 0x51;
const OP_16: u8 = 0x60;
const OP_NOP: u8 = 0x61;
const OP_IF: u8 = 0x63;
const OP_ELSE: u8 = 0x67;
const OP_ENDIF: u8 = 0x68;
const OP_VERIFY: u8 = 0x69;
const OP_RETURN: u8 = 0x6a;
const OP_DROP: u8 = 0x75;
const OP_DUP: u8 = 0x76;
const OP_EQUAL: u8 = 0x87;
const OP_EQUALVERIFY: u8 = 0x88;
const OP_SHA256: u8 = 0xa8;
const OP_PUBKEYHASH: u8 = 0xa9;
const OP_CHECKSIG: u8 = 0xac;
const OP_CHECKMULTISIG: u8 = 0xae;
const OP_CHECKLOCKTIMEVERIFY: u8 = 0xb1;
const OP_CHECKSEQUENCEVERIFY: u8 = 0xb2;

/// True when a covenant input sequence asks relays to allow replacement.
///
/// Live v1 transactions have no sequence byte and do not signal this.
pub const fn sequence_signals_rbf(sequence: u32) -> bool {
    sequence < TLT_SEQUENCE_FINAL
}

/// Failure of the deterministic script interpreter.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TltScriptError {
    #[error("script exceeds the working length limit")]
    ScriptTooLarge,
    #[error("script exceeds the working opcode limit")]
    TooManyOps,
    #[error("script stack exceeds the working depth")]
    StackOverflow,
    #[error("script stack underflow")]
    StackUnderflow,
    #[error("script push is truncated or too large")]
    InvalidPush,
    #[error("script verification failed")]
    VerifyFailed,
    #[error("script IF/ELSE/ENDIF is unbalanced")]
    UnbalancedIf,
    #[error("OP_RETURN is unspendable")]
    OpReturn,
    #[error("script integer is empty or wider than 8 bytes")]
    InvalidNumber,
    #[error("locktime type does not match the transaction locktime")]
    LocktimeTypeMismatch,
    #[error("absolute locktime is not satisfied")]
    LocktimeNotMet,
    #[error("input sequence disables the relative lock")]
    SequenceDisabled,
    #[error("relative lock type does not match the input sequence")]
    SequenceTypeMismatch,
    #[error("relative lock is not satisfied")]
    SequenceNotMet,
    #[error("multisig key or signature count is outside the working limit")]
    InvalidMultisig,
    #[error("unknown opcode {0}")]
    UnknownOpcode(u8),
    #[error("covenant transaction version is not {TLT_COVENANT_TX_VERSION}")]
    BadVersion,
    #[error("covenant input index is missing")]
    BadInput,
    #[error("P2SH scriptSig must be push-only")]
    P2shNotPushOnly,
    #[error("empty scriptPubKey")]
    EmptyScript,
}

/// secp256k1 check implemented by `agora-crypto`. The interpreter stays free of keys.
pub trait SigChecker {
    fn check_sig(&self, pubkey: &[u8], signature: &[u8], message: &[u8]) -> bool;
}

/// Where the spent output was created, in blue score and median time (unix seconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TltOutputOrigin {
    pub blue_score: u64,
    pub median_time_secs: u64,
}

/// Absolute and relative lock context for one covenant input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TltSpendContext {
    pub sighash_preimage: Vec<u8>,
    pub lock_time: u64,
    pub sequence: u32,
    pub spend_blue_score: u64,
    pub median_time_past_secs: u64,
    pub origin: TltOutputOrigin,
}

/// One covenant input. Sequence is on the wire here; v1 [`crate::TxIn`] has none.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize)]
pub struct TltCovenantInput {
    pub previous_outpoint: OutPoint,
    pub sequence: u32,
    pub script_sig: Vec<u8>,
}

/// One covenant output. The live UTXO [`crate::TxOut`] remains value plus address.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize)]
pub struct TltCovenantOutput {
    pub value: Amount,
    pub script_pubkey: Vec<u8>,
}

/// Versioned TLT covenant transaction.
///
/// Not accepted by the live block body. `nonce` is retained so Agora replay
/// binding is available even though Bitcoin transactions do not carry one.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize)]
pub struct TltCovenantTx {
    pub version: u32,
    pub inputs: Vec<TltCovenantInput>,
    pub outputs: Vec<TltCovenantOutput>,
    pub lock_time: u64,
    pub nonce: u64,
}

impl TltCovenantTx {
    /// Identifier commits to the full encoding, including script signatures.
    pub fn tx_id(&self) -> Hash {
        Hash::hash_borsh(&(TLT_COVENANT_TX_DOMAIN, self))
    }

    /// Signature preimage covers every field except `script_sig` bytes.
    pub fn sighash_preimage(&self) -> Vec<u8> {
        let inputs: Vec<(OutPoint, u32)> = self
            .inputs
            .iter()
            .map(|input| (input.previous_outpoint, input.sequence))
            .collect();
        borsh::to_vec(&(
            TLT_COVENANT_SIGHASH_DOMAIN,
            self.version,
            inputs,
            &self.outputs,
            self.lock_time,
            self.nonce,
        ))
        .expect("covenant sighash serialization")
    }
}

/// Agora P2PKH-style lock: DUP, SHA-256 prefix address, CHECKSIG.
///
/// The 20-byte program is [`crate::Address`] (first 20 bytes of SHA-256 of the
/// compressed pubkey), not Bitcoin HASH160.
pub fn script_p2pkh(address: &Address) -> Vec<u8> {
    let mut script = vec![OP_DUP, OP_PUBKEYHASH];
    push_data(&mut script, &address.0).expect("20-byte address push");
    script.push(OP_EQUALVERIFY);
    script.push(OP_CHECKSIG);
    script
}

/// P2SH program that commits to SHA-256(redeem script). Returns `(script_pubkey, hash)`.
pub fn script_p2sh(redeem_script: &[u8]) -> (Vec<u8>, Hash) {
    let digest = sha256(redeem_script);
    let mut script = vec![OP_SHA256];
    push_data(&mut script, &digest.0).expect("32-byte hash push");
    script.push(OP_EQUAL);
    (script, digest)
}

/// True when `script` is exactly the P2SH template from [`script_p2sh`].
pub fn is_p2sh_script(script: &[u8]) -> bool {
    script.len() == 36
        && script[0] == OP_SHA256
        && script[1] == OP_PUSH
        && script[2] == 32
        && script[35] == OP_EQUAL
}

/// M-of-N CHECKMULTISIG program. `m` and each pubkey count must be in `1..=15`.
pub fn script_multisig(m: u8, pubkeys: &[[u8; 33]]) -> Result<Vec<u8>, TltScriptError> {
    let n = u8::try_from(pubkeys.len()).unwrap_or(255);
    if m == 0 || m > n || n == 0 || n > TLT_MAX_MULTISIG {
        return Err(TltScriptError::InvalidMultisig);
    }
    let mut script = Vec::new();
    push_data(&mut script, &[m])?;
    for pubkey in pubkeys {
        push_data(&mut script, pubkey)?;
    }
    push_data(&mut script, &[n])?;
    script.push(OP_CHECKMULTISIG);
    Ok(script)
}

/// Hashlock claim path or absolute-locktime refund path.
///
/// Claim succeeds with the SHA-256 preimage and the recipient signature.
/// Refund succeeds with the sender signature once `lock_time` is met.
pub fn script_htlc(
    hash256: &Hash,
    recipient_pubkey: &[u8; 33],
    refund_pubkey: &[u8; 33],
    lock_time: u64,
) -> Result<Vec<u8>, TltScriptError> {
    let mut script = vec![OP_IF, OP_SHA256];
    push_data(&mut script, hash256.as_bytes())?;
    script.push(OP_EQUALVERIFY);
    push_data(&mut script, recipient_pubkey)?;
    script.push(OP_CHECKSIG);
    script.push(OP_ELSE);
    push_data(&mut script, &lock_time.to_le_bytes())?;
    script.push(OP_CHECKLOCKTIMEVERIFY);
    script.push(OP_DROP);
    push_data(&mut script, refund_pubkey)?;
    script.push(OP_CHECKSIG);
    script.push(OP_ENDIF);
    Ok(script)
}

/// Append a data push. Empty data becomes OP_0. Values 1..=16 become OP_1..OP_16.
pub fn push_data(buf: &mut Vec<u8>, data: &[u8]) -> Result<(), TltScriptError> {
    if data.is_empty() {
        buf.push(OP_0);
        return Ok(());
    }
    if data.len() == 1 && (1..=16).contains(&data[0]) {
        buf.push(OP_1 + data[0] - 1);
        return Ok(());
    }
    if data.len() > TLT_MAX_PUSH {
        return Err(TltScriptError::InvalidPush);
    }
    buf.push(OP_PUSH);
    buf.push(u8::try_from(data.len()).unwrap_or(255));
    buf.extend_from_slice(data);
    Ok(())
}

/// Evaluate `script_sig` then `script_pubkey` under `ctx`.
pub fn eval_tlt_script<C: SigChecker>(
    script_sig: &[u8],
    script_pubkey: &[u8],
    ctx: &TltSpendContext,
    checker: &C,
) -> Result<(), TltScriptError> {
    if script_pubkey.is_empty() {
        return Err(TltScriptError::EmptyScript);
    }
    check_len(script_sig)?;
    check_len(script_pubkey)?;
    let mut stack = Vec::new();
    run_script(&mut stack, script_sig, ctx, checker)?;
    if is_p2sh_script(script_pubkey) {
        if !is_push_only(script_sig)? {
            return Err(TltScriptError::P2shNotPushOnly);
        }
        let redeem = stack
            .last()
            .cloned()
            .ok_or(TltScriptError::StackUnderflow)?;
        check_len(&redeem)?;
        run_script(&mut stack, script_pubkey, ctx, checker)?;
        if !top_true(&stack) {
            return Err(TltScriptError::VerifyFailed);
        }
        stack.pop();
        run_script(&mut stack, &redeem, ctx, checker)?;
    } else {
        run_script(&mut stack, script_pubkey, ctx, checker)?;
    }
    if stack.len() == 1 && top_true(&stack) {
        Ok(())
    } else {
        Err(TltScriptError::VerifyFailed)
    }
}

/// Evaluate one covenant input against the script committed by the previous output.
pub fn eval_covenant_input<C: SigChecker>(
    tx: &TltCovenantTx,
    input_index: usize,
    script_pubkey: &[u8],
    spend_blue_score: u64,
    median_time_past_secs: u64,
    origin: TltOutputOrigin,
    checker: &C,
) -> Result<(), TltScriptError> {
    if tx.version != TLT_COVENANT_TX_VERSION {
        return Err(TltScriptError::BadVersion);
    }
    let input = tx.inputs.get(input_index).ok_or(TltScriptError::BadInput)?;
    let ctx = TltSpendContext {
        sighash_preimage: tx.sighash_preimage(),
        lock_time: tx.lock_time,
        sequence: input.sequence,
        spend_blue_score,
        median_time_past_secs,
        origin,
    };
    eval_tlt_script(&input.script_sig, script_pubkey, &ctx, checker)
}

fn check_len(script: &[u8]) -> Result<(), TltScriptError> {
    if script.len() > TLT_MAX_SCRIPT_LEN {
        Err(TltScriptError::ScriptTooLarge)
    } else {
        Ok(())
    }
}

fn is_push_only(script: &[u8]) -> Result<bool, TltScriptError> {
    let mut i = 0;
    while i < script.len() {
        let op = script[i];
        i += 1;
        if op == OP_0 || (OP_1..=OP_16).contains(&op) {
            continue;
        }
        if op == OP_PUSH {
            let len = *script.get(i).ok_or(TltScriptError::InvalidPush)? as usize;
            i += 1;
            if len == 0 || len > TLT_MAX_PUSH || i + len > script.len() {
                return Err(TltScriptError::InvalidPush);
            }
            i += len;
            continue;
        }
        return Ok(false);
    }
    Ok(true)
}

fn run_script<C: SigChecker>(
    stack: &mut Vec<Vec<u8>>,
    script: &[u8],
    ctx: &TltSpendContext,
    checker: &C,
) -> Result<(), TltScriptError> {
    let mut i = 0;
    let mut ops = 0usize;
    let mut exec: Vec<bool> = Vec::new();
    while i < script.len() {
        let op = script[i];
        i += 1;
        ops += 1;
        if ops > TLT_MAX_OPS {
            return Err(TltScriptError::TooManyOps);
        }
        if op == OP_PUSH {
            let len = *script.get(i).ok_or(TltScriptError::InvalidPush)? as usize;
            i += 1;
            if len == 0 || len > TLT_MAX_PUSH || i + len > script.len() {
                return Err(TltScriptError::InvalidPush);
            }
            let data = script[i..i + len].to_vec();
            i += len;
            if executing(&exec) {
                push(stack, data)?;
            }
            continue;
        }
        if !executing(&exec) && !matches!(op, OP_IF | OP_ELSE | OP_ENDIF) {
            if (OP_1..=OP_16).contains(&op) || op == OP_0 || op == OP_NOP {
                continue;
            }
            if !known_opcode(op) {
                return Err(TltScriptError::UnknownOpcode(op));
            }
            continue;
        }
        match op {
            OP_0 => push(stack, Vec::new())?,
            op if (OP_1..=OP_16).contains(&op) => push(stack, vec![op - OP_1 + 1])?,
            OP_NOP => {}
            OP_IF => {
                if exec.len() >= TLT_MAX_IF_DEPTH {
                    return Err(TltScriptError::UnbalancedIf);
                }
                if executing(&exec) {
                    let cond = pop(stack)?;
                    exec.push(is_true(&cond));
                } else {
                    exec.push(false);
                }
            }
            OP_ELSE => {
                let top = exec.last_mut().ok_or(TltScriptError::UnbalancedIf)?;
                *top = !*top;
            }
            OP_ENDIF => {
                exec.pop().ok_or(TltScriptError::UnbalancedIf)?;
            }
            OP_VERIFY => {
                if !is_true(&pop(stack)?) {
                    return Err(TltScriptError::VerifyFailed);
                }
            }
            OP_RETURN => return Err(TltScriptError::OpReturn),
            OP_DROP => {
                pop(stack)?;
            }
            OP_DUP => {
                let top = stack
                    .last()
                    .cloned()
                    .ok_or(TltScriptError::StackUnderflow)?;
                push(stack, top)?;
            }
            OP_EQUAL => {
                let a = pop(stack)?;
                let b = pop(stack)?;
                push(stack, bool_item(a == b))?;
            }
            OP_EQUALVERIFY => {
                let a = pop(stack)?;
                let b = pop(stack)?;
                if a != b {
                    return Err(TltScriptError::VerifyFailed);
                }
            }
            OP_SHA256 => {
                let item = pop(stack)?;
                push(stack, sha256(&item).0.to_vec())?;
            }
            OP_PUBKEYHASH => {
                let item = pop(stack)?;
                push(stack, sha256(&item).0[..20].to_vec())?;
            }
            OP_CHECKSIG => {
                // scriptSig pushes signature then pubkey, so pubkey is on top.
                let pubkey = pop(stack)?;
                let signature = pop(stack)?;
                let ok = checker.check_sig(&pubkey, &signature, &ctx.sighash_preimage);
                push(stack, bool_item(ok))?;
            }
            OP_CHECKMULTISIG => check_multisig(stack, ctx, checker)?,
            OP_CHECKLOCKTIMEVERIFY => check_locktime(stack, ctx)?,
            OP_CHECKSEQUENCEVERIFY => check_sequence(stack, ctx)?,
            other => return Err(TltScriptError::UnknownOpcode(other)),
        }
    }
    if exec.is_empty() {
        Ok(())
    } else {
        Err(TltScriptError::UnbalancedIf)
    }
}

fn known_opcode(op: u8) -> bool {
    matches!(
        op,
        OP_0 | OP_NOP
            | OP_IF
            | OP_ELSE
            | OP_ENDIF
            | OP_VERIFY
            | OP_RETURN
            | OP_DROP
            | OP_DUP
            | OP_EQUAL
            | OP_EQUALVERIFY
            | OP_SHA256
            | OP_PUBKEYHASH
            | OP_CHECKSIG
            | OP_CHECKMULTISIG
            | OP_CHECKLOCKTIMEVERIFY
            | OP_CHECKSEQUENCEVERIFY
    ) || (OP_1..=OP_16).contains(&op)
}

fn check_multisig<C: SigChecker>(
    stack: &mut Vec<Vec<u8>>,
    ctx: &TltSpendContext,
    checker: &C,
) -> Result<(), TltScriptError> {
    let n = read_count(&pop(stack)?)?;
    let mut pubs = Vec::with_capacity(n);
    for _ in 0..n {
        pubs.push(pop(stack)?);
    }
    pubs.reverse();
    let m = read_count(&pop(stack)?)?;
    if m == 0 || m > n {
        return Err(TltScriptError::InvalidMultisig);
    }
    let mut sigs = Vec::with_capacity(m);
    for _ in 0..m {
        sigs.push(pop(stack)?);
    }
    sigs.reverse();
    let mut key_idx = 0;
    let mut matched = 0;
    for sig in &sigs {
        let mut found = false;
        while key_idx < pubs.len() {
            let pubkey = &pubs[key_idx];
            key_idx += 1;
            if checker.check_sig(pubkey, sig, &ctx.sighash_preimage) {
                found = true;
                break;
            }
        }
        if !found {
            break;
        }
        matched += 1;
    }
    push(stack, bool_item(matched == m))
}

fn check_locktime(stack: &[Vec<u8>], ctx: &TltSpendContext) -> Result<(), TltScriptError> {
    if ctx.sequence == TLT_SEQUENCE_FINAL {
        return Err(TltScriptError::LocktimeNotMet);
    }
    let required = script_u64(stack.last().ok_or(TltScriptError::StackUnderflow)?)?;
    if locktime_is_time(required) != locktime_is_time(ctx.lock_time) {
        return Err(TltScriptError::LocktimeTypeMismatch);
    }
    if ctx.lock_time < required {
        return Err(TltScriptError::LocktimeNotMet);
    }
    if locktime_is_time(ctx.lock_time) {
        if ctx.median_time_past_secs < ctx.lock_time {
            return Err(TltScriptError::LocktimeNotMet);
        }
    } else if ctx.spend_blue_score < ctx.lock_time {
        return Err(TltScriptError::LocktimeNotMet);
    }
    Ok(())
}

fn check_sequence(stack: &[Vec<u8>], ctx: &TltSpendContext) -> Result<(), TltScriptError> {
    let operand = script_u32(stack.last().ok_or(TltScriptError::StackUnderflow)?)?;
    if operand & TLT_SEQUENCE_DISABLE_FLAG != 0 {
        return Ok(());
    }
    if ctx.sequence & TLT_SEQUENCE_DISABLE_FLAG != 0 {
        return Err(TltScriptError::SequenceDisabled);
    }
    let required_time = operand & TLT_SEQUENCE_TIME_FLAG != 0;
    let input_time = ctx.sequence & TLT_SEQUENCE_TIME_FLAG != 0;
    if required_time != input_time {
        return Err(TltScriptError::SequenceTypeMismatch);
    }
    let required = operand & TLT_SEQUENCE_LOCK_MASK;
    let offered = ctx.sequence & TLT_SEQUENCE_LOCK_MASK;
    if offered < required {
        return Err(TltScriptError::SequenceNotMet);
    }
    if required_time {
        let elapsed = ctx
            .median_time_past_secs
            .saturating_sub(ctx.origin.median_time_secs);
        if elapsed / TLT_CSV_TIME_STEP_SECS < u64::from(required) {
            return Err(TltScriptError::SequenceNotMet);
        }
    } else {
        let distance = ctx.spend_blue_score.saturating_sub(ctx.origin.blue_score);
        if distance < u64::from(required) {
            return Err(TltScriptError::SequenceNotMet);
        }
    }
    Ok(())
}

fn locktime_is_time(lock_time: u64) -> bool {
    lock_time >= TLT_LOCKTIME_TIME_THRESHOLD
}

fn read_count(bytes: &[u8]) -> Result<usize, TltScriptError> {
    let n = script_u64(bytes)?;
    if n == 0 || n > u64::from(TLT_MAX_MULTISIG) {
        return Err(TltScriptError::InvalidMultisig);
    }
    Ok(usize::try_from(n).unwrap_or(0))
}

fn script_u64(bytes: &[u8]) -> Result<u64, TltScriptError> {
    if bytes.is_empty() || bytes.len() > 8 {
        return Err(TltScriptError::InvalidNumber);
    }
    let mut buf = [0u8; 8];
    buf[..bytes.len()].copy_from_slice(bytes);
    Ok(u64::from_le_bytes(buf))
}

fn script_u32(bytes: &[u8]) -> Result<u32, TltScriptError> {
    let value = script_u64(bytes)?;
    u32::try_from(value).map_err(|_| TltScriptError::InvalidNumber)
}

fn executing(exec: &[bool]) -> bool {
    exec.iter().all(|flag| *flag)
}

fn is_true(bytes: &[u8]) -> bool {
    bytes.iter().any(|byte| *byte != 0)
}

fn bool_item(value: bool) -> Vec<u8> {
    if value {
        vec![1]
    } else {
        vec![0]
    }
}

fn top_true(stack: &[Vec<u8>]) -> bool {
    stack.last().is_some_and(|item| is_true(item))
}

fn push(stack: &mut Vec<Vec<u8>>, item: Vec<u8>) -> Result<(), TltScriptError> {
    if stack.len() >= TLT_MAX_STACK {
        return Err(TltScriptError::StackOverflow);
    }
    stack.push(item);
    Ok(())
}

fn pop(stack: &mut Vec<Vec<u8>>) -> Result<Vec<u8>, TltScriptError> {
    stack.pop().ok_or(TltScriptError::StackUnderflow)
}

fn sha256(bytes: &[u8]) -> Hash {
    let digest = Sha256::digest(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Hash(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EqChecker;
    impl SigChecker for EqChecker {
        fn check_sig(&self, pubkey: &[u8], signature: &[u8], _message: &[u8]) -> bool {
            !pubkey.is_empty() && pubkey == signature
        }
    }

    fn origin() -> TltOutputOrigin {
        TltOutputOrigin {
            blue_score: 0,
            median_time_secs: 0,
        }
    }

    fn ctx(sequence: u32, lock_time: u64, blue: u64, mtp: u64) -> TltSpendContext {
        TltSpendContext {
            sighash_preimage: b"agora-test-sighash".to_vec(),
            lock_time,
            sequence,
            spend_blue_score: blue,
            median_time_past_secs: mtp,
            origin: origin(),
        }
    }

    fn p2pkh_sig(address_byte: u8) -> (Vec<u8>, Vec<u8>, Address) {
        let pubkey = [address_byte; 33];
        let mut script_sig = Vec::new();
        push_data(&mut script_sig, &pubkey).unwrap();
        push_data(&mut script_sig, &pubkey).unwrap();
        let address = Address(sha256(&pubkey).0[..20].try_into().unwrap());
        (script_sig, script_p2pkh(&address), address)
    }

    #[test]
    fn p2pkh_style_accepts_matching_pubkey_and_rejects_a_different_one() {
        let (script_sig, script_pubkey, _) = p2pkh_sig(0x02);
        eval_tlt_script(
            &script_sig,
            &script_pubkey,
            &ctx(TLT_SEQUENCE_FINAL, 0, 1, 0),
            &EqChecker,
        )
        .unwrap();
        let (bad_sig, _, _) = p2pkh_sig(0x03);
        assert!(eval_tlt_script(
            &bad_sig,
            &script_pubkey,
            &ctx(TLT_SEQUENCE_FINAL, 0, 1, 0),
            &EqChecker,
        )
        .is_err());
    }

    #[test]
    fn p2sh_wraps_p2pkh_style_program() {
        let pubkey = [0x04u8; 33];
        let address = Address(sha256(&pubkey).0[..20].try_into().unwrap());
        let redeem = script_p2pkh(&address);
        let (script_pubkey, _) = script_p2sh(&redeem);
        let mut script_sig = Vec::new();
        push_data(&mut script_sig, &pubkey).unwrap();
        push_data(&mut script_sig, &pubkey).unwrap();
        push_data(&mut script_sig, &redeem).unwrap();
        eval_tlt_script(
            &script_sig,
            &script_pubkey,
            &ctx(TLT_SEQUENCE_FINAL, 0, 1, 0),
            &EqChecker,
        )
        .unwrap();
    }

    #[test]
    fn multisig_requires_m_of_n_in_pubkey_order() {
        let keys = [[0x11u8; 33], [0x22; 33], [0x33; 33]];
        let script = script_multisig(2, &keys).unwrap();
        let mut ok = Vec::new();
        push_data(&mut ok, &keys[0]).unwrap();
        push_data(&mut ok, &keys[2]).unwrap();
        eval_tlt_script(&ok, &script, &ctx(TLT_SEQUENCE_FINAL, 0, 1, 0), &EqChecker).unwrap();
        let mut wrong_order = Vec::new();
        push_data(&mut wrong_order, &keys[2]).unwrap();
        push_data(&mut wrong_order, &keys[0]).unwrap();
        assert!(eval_tlt_script(
            &wrong_order,
            &script,
            &ctx(TLT_SEQUENCE_FINAL, 0, 1, 0),
            &EqChecker,
        )
        .is_err());
    }

    #[test]
    fn absolute_blue_score_lock_and_relative_sequence_lock() {
        let pubkey = [0x05u8; 33];
        let mut locked = Vec::new();
        push_data(&mut locked, &10u64.to_le_bytes()).unwrap();
        locked.push(OP_CHECKLOCKTIMEVERIFY);
        locked.push(OP_DROP);
        push_data(&mut locked, &pubkey).unwrap();
        locked.push(OP_CHECKSIG);
        let mut script_sig = Vec::new();
        push_data(&mut script_sig, &pubkey).unwrap();
        let sequence = TLT_SEQUENCE_FINAL - 1;
        assert!(
            eval_tlt_script(&script_sig, &locked, &ctx(sequence, 10, 9, 0), &EqChecker).is_err()
        );
        eval_tlt_script(&script_sig, &locked, &ctx(sequence, 10, 10, 0), &EqChecker).unwrap();
        assert!(eval_tlt_script(
            &script_sig,
            &locked,
            &ctx(TLT_SEQUENCE_FINAL, 10, 10, 0),
            &EqChecker,
        )
        .is_err());

        let mut relative = vec![OP_1 + 1, OP_CHECKSEQUENCEVERIFY, OP_DROP];
        push_data(&mut relative, &pubkey).unwrap();
        relative.push(OP_CHECKSIG);
        let mut spend = ctx(2, 0, 2, 0);
        spend.origin.blue_score = 0;
        eval_tlt_script(&script_sig, &relative, &spend, &EqChecker).unwrap();
        spend.spend_blue_score = 1;
        assert!(eval_tlt_script(&script_sig, &relative, &spend, &EqChecker).is_err());
    }

    #[test]
    fn htlc_claim_and_refund_paths() {
        let recipient = [0x06u8; 33];
        let refund = [0x07u8; 33];
        let preimage = b"agora-htlc-preimage";
        let hash = sha256(preimage);
        let script = script_htlc(&hash, &recipient, &refund, 20).unwrap();
        let mut claim = Vec::new();
        // Signature is deeper than the preimage so SHA256 sees the preimage.
        push_data(&mut claim, &recipient).unwrap();
        push_data(&mut claim, preimage).unwrap();
        claim.push(OP_1);
        let sequence = TLT_SEQUENCE_FINAL - 1;
        eval_tlt_script(&claim, &script, &ctx(sequence, 0, 1, 0), &EqChecker).unwrap();

        let mut bad_preimage = Vec::new();
        push_data(&mut bad_preimage, &recipient).unwrap();
        push_data(&mut bad_preimage, b"wrong").unwrap();
        bad_preimage.push(OP_1);
        assert!(
            eval_tlt_script(&bad_preimage, &script, &ctx(sequence, 0, 1, 0), &EqChecker).is_err()
        );

        let mut refund_sig = Vec::new();
        push_data(&mut refund_sig, &refund).unwrap();
        refund_sig.push(OP_0);
        assert!(
            eval_tlt_script(&refund_sig, &script, &ctx(sequence, 20, 19, 0), &EqChecker).is_err()
        );
        eval_tlt_script(&refund_sig, &script, &ctx(sequence, 20, 20, 0), &EqChecker).unwrap();
    }

    #[test]
    fn op_return_unknown_opcode_and_unbalanced_if_fail() {
        assert_eq!(
            eval_tlt_script(
                &[],
                &[OP_RETURN],
                &ctx(TLT_SEQUENCE_FINAL, 0, 1, 0),
                &EqChecker
            ),
            Err(TltScriptError::OpReturn)
        );
        assert!(matches!(
            eval_tlt_script(&[], &[0xFF], &ctx(TLT_SEQUENCE_FINAL, 0, 1, 0), &EqChecker),
            Err(TltScriptError::UnknownOpcode(0xFF))
        ));
        assert_eq!(
            eval_tlt_script(&[OP_IF], &[], &ctx(TLT_SEQUENCE_FINAL, 0, 1, 0), &EqChecker),
            Err(TltScriptError::EmptyScript)
        );
        assert_eq!(
            eval_tlt_script(
                &[],
                &[OP_IF, OP_1, OP_ENDIF],
                &ctx(TLT_SEQUENCE_FINAL, 0, 1, 0),
                &EqChecker
            ),
            Err(TltScriptError::StackUnderflow)
        );
    }

    #[test]
    fn covenant_id_includes_script_sig_and_sighash_does_not() {
        let output = TltCovenantOutput {
            value: Amount::from_base_units(1),
            script_pubkey: script_p2pkh(&Address([9u8; 20])),
        };
        let mut tx = TltCovenantTx {
            version: TLT_COVENANT_TX_VERSION,
            inputs: vec![TltCovenantInput {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
                sequence: TLT_SEQUENCE_FINAL,
                script_sig: Vec::new(),
            }],
            outputs: vec![output],
            lock_time: 0,
            nonce: 7,
        };
        let bare_id = tx.tx_id();
        let bare_sighash = tx.sighash_preimage();
        tx.inputs[0].script_sig = vec![OP_1];
        assert_ne!(tx.tx_id(), bare_id);
        assert_eq!(tx.sighash_preimage(), bare_sighash);
        assert!(!sequence_signals_rbf(TLT_SEQUENCE_FINAL));
        assert!(sequence_signals_rbf(TLT_SEQUENCE_FINAL - 1));
    }

    #[test]
    fn v1_transaction_wire_has_implicit_final_sequence_and_no_locktime() {
        use crate::{Transaction, TxIn, TxOut};
        let tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 7,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1000),
                address: Address([0xAB; 20]),
            }],
            42,
        );
        let bytes = borsh::to_vec(&tx).unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(&1u32.to_le_bytes());
        expected.extend_from_slice(&1u32.to_le_bytes());
        expected.extend_from_slice(&[0u8; 32]);
        expected.extend_from_slice(&7u32.to_le_bytes());
        expected.extend_from_slice(&1u32.to_le_bytes());
        expected.extend_from_slice(&1000u64.to_le_bytes());
        expected.extend_from_slice(&[0xABu8; 20]);
        expected.extend_from_slice(&42u64.to_le_bytes());
        expected.extend_from_slice(&0u32.to_le_bytes());
        expected.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(bytes, expected);
        assert_eq!(tx.effective_lock_time(), 0);
        assert_eq!(tx.effective_sequence(0), Some(TLT_SEQUENCE_FINAL));
        assert!(!tx.signals_replace_by_fee());
        assert_eq!(tx.tx_id(), Hash::hash_borsh(&tx));
    }
}

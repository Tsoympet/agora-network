use thiserror::Error;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("unknown zone")]
    UnknownZone,
    #[error("missing utxo: {0}")]
    MissingUtxo(String),
    #[error("double spend: {0}")]
    DoubleSpend(String),
    #[error("invalid transaction: {0}")]
    InvalidTx(String),
    #[error("coinbase error: {0}")]
    Coinbase(String),
}

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GovernanceError {
    #[error("total supply must be greater than zero")]
    ZeroSupply,
    #[error("voter set is empty")]
    EmptyElectorate,
    #[error("vote tally overflow")]
    Overflow,
}

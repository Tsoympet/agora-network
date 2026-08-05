use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GovernanceError {
    #[error("total supply must be greater than zero")]
    ZeroSupply,
    #[error("voter set is empty")]
    EmptyElectorate,
    #[error("vote tally overflow")]
    Overflow,
    #[error("unknown proposal")]
    UnknownProposal,
    #[error("proposal is not open for deposits")]
    NotAcceptingDeposit,
    #[error("proposal is not open for voting")]
    NotAcceptingVotes,
    #[error("proposal cannot be tallied in its current status")]
    NotReadyToTally,
    #[error("proposal cannot be executed in its current status")]
    NotReadyToExecute,
    #[error("timelock has not elapsed")]
    TimelockActive,
    #[error("voter already cast a ballot on this proposal")]
    DuplicateVote,
    #[error("voter is not eligible in this chamber")]
    IneligibleVoter,
    #[error("proposal kind requires a different chamber")]
    WrongChamber,
    #[error("missing required sponsorship (Tamias)")]
    MissingSponsorship,
    #[error("missing required Archon assent")]
    MissingArchonAssent,
    #[error("seat is already occupied")]
    SeatOccupied,
    #[error("seat is vacant")]
    SeatVacant,
    #[error("invalid seat index for this rank")]
    InvalidSeat,
    #[error("invalid constitution payload")]
    InvalidConstitution,
    #[error("deposit below minimum")]
    InsufficientDeposit,
    #[error("quorum not reached")]
    QuorumNotMet,
    #[error("pass threshold not reached")]
    ThresholdNotMet,
}

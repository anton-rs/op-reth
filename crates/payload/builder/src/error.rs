//! Error types emitted by types or implementations of this crate.

use reth_primitives::H256;
use revm_primitives::{EVMError, B160};
use tokio::sync::oneshot;

/// Possible error variants during payload building.
#[derive(Debug, thiserror::Error)]
pub enum PayloadBuilderError {
    /// Thrown whe the parent block is missing.
    #[error("missing parent block {0:?}")]
    MissingParentBlock(H256),
    /// An oneshot channels has been closed.
    #[error("sender has been dropped")]
    ChannelClosed,
    /// Other internal error
    #[error(transparent)]
    Internal(#[from] reth_interfaces::Error),
    /// Unrecoverable error during evm execution.
    #[error("evm execution error: {0:?}")]
    EvmExecutionError(EVMError<reth_interfaces::Error>),
    /// Thrown if the payload requests withdrawals before Shanghai activation.
    #[error("withdrawals set before Shanghai activation")]
    WithdrawalsBeforeShanghai,
    /// Thrown when a transaction fails to convert to a
    /// [reth_primitives::TransactionSignedEcRecovered].
    #[cfg(feature = "optimism")]
    #[error("failed to convert deposit transaction to TransactionSignedEcRecovered")]
    TransactionEcRecoverFailed,
    /// Thrown when the L1 block info could not be parsed from the calldata of the
    /// first transaction supplied in the payload attributes.
    #[cfg(feature = "optimism")]
    #[error("failed to parse L1 block info from L1 info tx calldata")]
    L1BlockInfoParseFailed,
    /// Thrown when a database account could not be loaded.
    #[error("failed to load account {0:?}")]
    #[cfg(feature = "optimism")]
    AccountLoadFailed(B160),
    /// Thrown when a system transaction is sent post-Regolith.
    #[cfg(feature = "optimism")]
    #[error("system transaction sent post-Regolith")]
    SystemTransactionPostRegolith,
}

impl From<oneshot::error::RecvError> for PayloadBuilderError {
    fn from(_: oneshot::error::RecvError) -> Self {
        PayloadBuilderError::ChannelClosed
    }
}

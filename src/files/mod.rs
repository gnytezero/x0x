//! File transfer protocol for agent-to-agent file sharing.
//!
//! Transfers use direct messaging (QUIC streams) with chunked transfer
//! and SHA-256 integrity verification. Only accepted from trusted contacts
//! by default.

use serde::{Deserialize, Serialize};

/// Default chunk size: 64 KB.
pub const DEFAULT_CHUNK_SIZE: usize = 65536;

/// A file transfer offer sent to initiate transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOffer {
    /// Unique transfer ID.
    pub transfer_id: String,
    /// Original filename.
    pub filename: String,
    /// File size in bytes.
    pub size: u64,
    /// SHA-256 hash of the complete file.
    pub sha256: String,
    /// Chunk size in bytes.
    pub chunk_size: usize,
    /// Total number of chunks.
    pub total_chunks: u64,
}

/// A single file chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunk {
    /// Transfer ID this chunk belongs to.
    pub transfer_id: String,
    /// Chunk sequence number (0-indexed).
    pub sequence: u64,
    /// Base64-encoded chunk data.
    pub data: String,
}

/// Completion message sent after all chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileComplete {
    /// Transfer ID.
    pub transfer_id: String,
    /// SHA-256 hash (for verification).
    pub sha256: String,
}

/// Transfer direction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferDirection {
    /// Sending a file.
    Sending,
    /// Receiving a file.
    Receiving,
}

/// Transfer status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferStatus {
    /// Offer sent/received, waiting for acceptance.
    Pending,
    /// Transfer in progress.
    InProgress,
    /// Transfer complete and verified.
    Complete,
    /// Transfer failed.
    Failed,
    /// Transfer rejected by receiver.
    Rejected,
}

/// Tracks state of a file transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferState {
    /// Unique transfer ID.
    pub transfer_id: String,
    /// Direction (sending or receiving).
    pub direction: TransferDirection,
    /// Remote agent ID.
    pub remote_agent_id: String,
    /// Filename.
    pub filename: String,
    /// Total size in bytes.
    pub total_size: u64,
    /// Bytes transferred so far.
    pub bytes_transferred: u64,
    /// Current status.
    pub status: TransferStatus,
    /// SHA-256 hash of the file.
    pub sha256: String,
    /// Error message if failed.
    pub error: Option<String>,
    /// Timestamp when transfer started (unix seconds).
    pub started_at: u64,
}

/// File transfer message types (sent over direct messaging).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FileMessage {
    /// Offer to send a file.
    #[serde(rename = "file-offer")]
    Offer(FileOffer),
    /// A chunk of file data.
    #[serde(rename = "file-chunk")]
    Chunk(FileChunk),
    /// Transfer complete.
    #[serde(rename = "file-complete")]
    Complete(FileComplete),
    /// Accept a transfer offer.
    #[serde(rename = "file-accept")]
    Accept {
        /// Transfer ID to accept.
        transfer_id: String,
    },
    /// Reject a transfer offer.
    #[serde(rename = "file-reject")]
    Reject {
        /// Transfer ID to reject.
        transfer_id: String,
        /// Reason for rejection.
        reason: String,
    },
}

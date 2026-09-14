//! Durable receipts for explicit legacy Wiki/Web store imports.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const RECEIPT_VERSION: u8 = 1;

static JOURNAL_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
#[cfg(test)]
static FAIL_APPEND_KEY: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct LegacyImportReceipt {
    pub version: u8,
    pub idempotency_key: String,
    pub group_id: String,
    pub app: String,
    pub source_store_id: String,
    pub source_digest: String,
    pub endorser: String,
    pub authority_binding: String,
    pub destination_digest_before: String,
    pub destination_digest_after: String,
    pub imported_at_ms: u64,
}

pub(super) struct LegacyImportReceiptInput {
    pub idempotency_key: String,
    pub group_id: String,
    pub app: String,
    pub source_store_id: String,
    pub source_digest: String,
    pub endorser: String,
    pub authority_binding: String,
    pub destination_digest_before: String,
    pub destination_digest_after: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LegacyImportJournal {
    #[serde(default)]
    receipts: Vec<LegacyImportReceipt>,
}

pub(super) fn journal_path(kv_state_dir: &Path) -> PathBuf {
    kv_state_dir.join("legacy-page-import-receipts-v1.json")
}

pub(super) async fn read_receipts(path: &Path) -> std::io::Result<Vec<LegacyImportReceipt>> {
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let journal: LegacyImportJournal = serde_json::from_slice(&bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    if journal
        .receipts
        .iter()
        .any(|receipt| receipt.version != RECEIPT_VERSION)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unsupported legacy import receipt version",
        ));
    }
    Ok(journal.receipts)
}

pub(super) async fn append_receipt(
    path: &Path,
    receipt: LegacyImportReceipt,
) -> std::io::Result<()> {
    // Receipts for every group share one journal. Per-import and per-group
    // locks cannot prevent two unrelated imports from losing each other's
    // read-modify-write update.
    let _journal_guard = JOURNAL_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let mut receipts = read_receipts(path).await?;
    if let Some(existing) = receipts
        .iter()
        .find(|existing| existing.idempotency_key == receipt.idempotency_key)
    {
        return if existing == &receipt {
            sync_parent_directory(path).await
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "idempotency key already binds different import arguments",
            ))
        };
    }
    #[cfg(test)]
    if FAIL_APPEND_KEY
        .lock()
        .map(|mut key| {
            if key.as_deref() == Some(receipt.idempotency_key.as_str()) {
                key.take();
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "injected receipt append failure",
        ));
    }
    receipts.push(receipt);
    let bytes = serde_json::to_vec_pretty(&LegacyImportJournal { receipts })
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    write_atomic(path, &bytes).await
}

#[cfg(test)]
pub(super) fn fail_next_append_for_test(idempotency_key: &str) {
    if let Ok(mut key) = FAIL_APPEND_KEY.lock() {
        *key = Some(idempotency_key.to_string());
    }
}

pub(super) fn new_receipt(input: LegacyImportReceiptInput) -> LegacyImportReceipt {
    let imported_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64);
    LegacyImportReceipt {
        version: RECEIPT_VERSION,
        idempotency_key: input.idempotency_key,
        group_id: input.group_id,
        app: input.app,
        source_store_id: input.source_store_id,
        source_digest: input.source_digest,
        endorser: input.endorser,
        authority_binding: input.authority_binding,
        destination_digest_before: input.destination_digest_before,
        destination_digest_after: input.destination_digest_after,
        imported_at_ms,
    }
}

async fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let temporary = PathBuf::from(temporary);
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temporary, path).await?;
        sync_parent_directory(path).await?;
        Ok::<(), std::io::Error>(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

async fn sync_parent_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        let parent = parent.to_path_buf();
        tokio::task::spawn_blocking(move || std::fs::File::open(parent)?.sync_all())
            .await
            .map_err(std::io::Error::other)??;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(key: &str, group: &str) -> LegacyImportReceipt {
        new_receipt(LegacyImportReceiptInput {
            idempotency_key: key.to_string(),
            group_id: group.to_string(),
            app: "wiki".to_string(),
            source_store_id: format!("source-{group}"),
            source_digest: format!("digest-{group}"),
            endorser: "endorser".to_string(),
            authority_binding: "binding".to_string(),
            destination_digest_before: "before".to_string(),
            destination_digest_after: "after".to_string(),
        })
    }

    #[tokio::test]
    async fn journal_preserves_concurrent_cross_group_receipts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = journal_path(dir.path());
        let (a, b) = tokio::join!(
            append_receipt(&path, receipt("key-a", "group-a")),
            append_receipt(&path, receipt("key-b", "group-b")),
        );
        a.expect("append a");
        b.expect("append b");
        let saved = read_receipts(&path).await.expect("read journal");
        assert_eq!(saved.len(), 2);
        assert!(saved.iter().any(|item| item.idempotency_key == "key-a"));
        assert!(saved.iter().any(|item| item.idempotency_key == "key-b"));
    }

    #[tokio::test]
    async fn journal_rejects_same_key_with_different_binding() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = journal_path(dir.path());
        append_receipt(&path, receipt("same", "group-a"))
            .await
            .expect("first receipt");
        let error = append_receipt(&path, receipt("same", "group-b"))
            .await
            .expect_err("changed binding must conflict");
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    }
}

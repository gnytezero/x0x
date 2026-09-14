//! Bounded paging for retained group-store history images.
//!
//! Each payload is still authenticated by the surrounding group mutation.
//! The image digest binds every page to one immutable serialized CRDT image;
//! receivers assemble into a temporary buffer and merge only after the full
//! length and digest validate.

use super::{KvError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const PAGED_RETAINED_MAGIC: &[u8] = b"x0x.kv.retained.pages.v1\0";
pub(crate) const MAX_RETAINED_IMAGE_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_RETAINED_PAGES: u32 = 64;
const MAX_INFLIGHT_IMAGES: usize = 4;
const MAX_INFLIGHT_BYTES: usize = 32 * 1024 * 1024;
const INFLIGHT_TTL: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RetainedPageBinding {
    pub store_id: [u8; 32],
    pub endorser: [u8; 32],
    pub authorization: [u8; 32],
    pub image_id: [u8; 32],
}

#[derive(Debug)]
struct PendingImage {
    created: Instant,
    manifest: Option<RetainedPageV1>,
    pages: BTreeMap<u32, Vec<u8>>,
    received_len: usize,
}

#[derive(Debug, Default)]
pub(crate) struct RetainedPagePool {
    images: BTreeMap<RetainedPageBinding, PendingImage>,
    received_len: usize,
}

impl RetainedPagePool {
    pub(crate) fn push(
        &mut self,
        binding: RetainedPageBinding,
        frame: RetainedPageV1,
    ) -> Result<Option<Vec<u8>>> {
        self.prune();
        match &frame {
            RetainedPageV1::Manifest { image_id, .. } => {
                if image_id != &binding.image_id {
                    return Err(KvError::Gossip(
                        "retained manifest binding mismatch".to_string(),
                    ));
                }
                RetainedPageAssembler::from_manifest(&frame)?;
            }
            RetainedPageV1::Page {
                image_id,
                index,
                bytes,
            } => {
                if image_id != &binding.image_id
                    || *index >= MAX_RETAINED_PAGES
                    || bytes.len() > MAX_RETAINED_IMAGE_BYTES
                {
                    return Err(KvError::Gossip(
                        "retained page binding or size mismatch".to_string(),
                    ));
                }
            }
        }
        let validation = (|| -> Result<()> {
            match (&frame, self.images.get(&binding)) {
                (RetainedPageV1::Manifest { .. }, Some(pending)) => {
                    let mut assembler = RetainedPageAssembler::from_manifest(&frame)?;
                    pending.pages.iter().try_for_each(|(&index, bytes)| {
                        assembler
                            .push(RetainedPageV1::Page {
                                image_id: binding.image_id,
                                index,
                                bytes: bytes.clone(),
                            })
                            .map(|_| ())
                    })
                }
                (
                    RetainedPageV1::Page { .. },
                    Some(PendingImage {
                        manifest: Some(manifest),
                        pages,
                        ..
                    }),
                ) => {
                    let mut assembler = RetainedPageAssembler::from_manifest(manifest)?;
                    for (&index, bytes) in pages {
                        assembler.push(RetainedPageV1::Page {
                            image_id: binding.image_id,
                            index,
                            bytes: bytes.clone(),
                        })?;
                    }
                    assembler.push(frame.clone()).map(|_| ())
                }
                _ => Ok(()),
            }
        })();
        if let Err(error) = validation {
            if let Some(pending) = self.images.remove(&binding) {
                self.received_len = self.received_len.saturating_sub(pending.received_len);
            }
            return Err(error);
        }
        if !self.images.contains_key(&binding) && self.images.len() >= MAX_INFLIGHT_IMAGES {
            return Err(KvError::Gossip(
                "too many retained images are awaiting pages".to_string(),
            ));
        }
        if let RetainedPageV1::Page { index, bytes, .. } = &frame {
            let pending = self.images.get(&binding);
            let is_new_page = pending.is_none_or(|pending| !pending.pages.contains_key(index));
            if is_new_page {
                let image_len = pending
                    .map_or(0, |pending| pending.received_len)
                    .checked_add(bytes.len())
                    .ok_or_else(|| {
                        KvError::Gossip("retained page byte count overflow".to_string())
                    })?;
                let global_len = self.received_len.checked_add(bytes.len()).ok_or_else(|| {
                    KvError::Gossip("retained inflight byte count overflow".to_string())
                })?;
                if image_len > MAX_RETAINED_IMAGE_BYTES || global_len > MAX_INFLIGHT_BYTES {
                    return Err(KvError::Gossip(
                        "retained page pool exceeds resource limits".to_string(),
                    ));
                }
            }
        }
        let pending = self
            .images
            .entry(binding.clone())
            .or_insert_with(|| PendingImage {
                created: Instant::now(),
                manifest: None,
                pages: BTreeMap::new(),
                received_len: 0,
            });
        match frame {
            manifest @ RetainedPageV1::Manifest { image_id, .. } => {
                if image_id != binding.image_id {
                    return Err(KvError::Gossip(
                        "retained manifest binding mismatch".to_string(),
                    ));
                }
                if let Some(existing) = pending.manifest.as_ref() {
                    if existing != &manifest {
                        return Err(KvError::Gossip("conflicting retained manifest".to_string()));
                    }
                } else {
                    RetainedPageAssembler::from_manifest(&manifest)?;
                    pending.manifest = Some(manifest);
                }
            }
            RetainedPageV1::Page {
                image_id,
                index,
                bytes,
            } => {
                if image_id != binding.image_id || index >= MAX_RETAINED_PAGES {
                    return Err(KvError::Gossip(
                        "retained page binding mismatch".to_string(),
                    ));
                }
                if let Some(existing) = pending.pages.get(&index) {
                    if existing != &bytes {
                        return Err(KvError::Gossip(
                            "conflicting duplicate retained page".to_string(),
                        ));
                    }
                    return Ok(None);
                }
                let image_len = pending
                    .received_len
                    .checked_add(bytes.len())
                    .ok_or_else(|| {
                        KvError::Gossip("retained page byte count overflow".to_string())
                    })?;
                let global_len = self.received_len.checked_add(bytes.len()).ok_or_else(|| {
                    KvError::Gossip("retained inflight byte count overflow".to_string())
                })?;
                if image_len > MAX_RETAINED_IMAGE_BYTES || global_len > MAX_INFLIGHT_BYTES {
                    return Err(KvError::Gossip(
                        "retained page pool exceeds resource limits".to_string(),
                    ));
                }
                pending.pages.insert(index, bytes);
                pending.received_len = image_len;
                self.received_len = global_len;
            }
        }
        let Some(manifest) = pending.manifest.as_ref() else {
            return Ok(None);
        };
        let mut assembler = RetainedPageAssembler::from_manifest(manifest)?;
        let mut completed = None;
        for (&index, bytes) in &pending.pages {
            completed = assembler.push(RetainedPageV1::Page {
                image_id: binding.image_id,
                index,
                bytes: bytes.clone(),
            })?;
        }
        if completed.is_some() {
            if let Some(done) = self.images.remove(&binding) {
                self.received_len = self.received_len.saturating_sub(done.received_len);
            }
        }
        Ok(completed)
    }

    fn prune(&mut self) {
        let now = Instant::now();
        let expired: Vec<_> = self
            .images
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.created) > INFLIGHT_TTL)
            .map(|(binding, _)| binding.clone())
            .collect();
        for binding in expired {
            if let Some(pending) = self.images.remove(&binding) {
                self.received_len = self.received_len.saturating_sub(pending.received_len);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum RetainedPageV1 {
    Manifest {
        image_id: [u8; 32],
        total_len: u64,
        page_count: u32,
    },
    Page {
        image_id: [u8; 32],
        index: u32,
        bytes: Vec<u8>,
    },
}

pub(crate) fn encode_page(page: &RetainedPageV1) -> Result<Vec<u8>> {
    let body = bincode::serialize(page)?;
    let mut encoded = Vec::with_capacity(PAGED_RETAINED_MAGIC.len() + body.len());
    encoded.extend_from_slice(PAGED_RETAINED_MAGIC);
    encoded.extend_from_slice(&body);
    Ok(encoded)
}

pub(crate) fn decode_page(payload: &[u8]) -> Result<Option<RetainedPageV1>> {
    let Some(body) = payload.strip_prefix(PAGED_RETAINED_MAGIC) else {
        return Ok(None);
    };
    bincode::deserialize(body).map(Some).map_err(KvError::from)
}

pub(crate) fn split_image(image: &[u8], max_payload: usize) -> Result<Vec<Vec<u8>>> {
    if image.len() > MAX_RETAINED_IMAGE_BYTES {
        return Err(KvError::Gossip(
            "retained image exceeds paging resource limit".into(),
        ));
    }
    let image_id = *blake3::hash(image).as_bytes();
    let empty_page_overhead = encode_page(&RetainedPageV1::Page {
        image_id,
        index: 0,
        bytes: Vec::new(),
    })?
    .len();
    let chunk_len = max_payload
        .checked_sub(empty_page_overhead)
        .ok_or_else(|| {
            KvError::Gossip("retained page payload budget cannot hold framing".into())
        })?;
    if chunk_len == 0 {
        return Err(KvError::Gossip(
            "retained page payload budget is empty".into(),
        ));
    }
    let page_count_usize = image.len().div_ceil(chunk_len).max(1);
    let page_count = u32::try_from(page_count_usize)
        .map_err(|_| KvError::Gossip("retained page count overflow".into()))?;
    if page_count > MAX_RETAINED_PAGES {
        return Err(KvError::Gossip(
            "retained image requires too many pages".into(),
        ));
    }
    let total_len = u64::try_from(image.len())
        .map_err(|_| KvError::Gossip("retained image length overflow".into()))?;
    let mut payloads = vec![encode_page(&RetainedPageV1::Manifest {
        image_id,
        total_len,
        page_count,
    })?];
    if image.is_empty() {
        payloads.push(encode_page(&RetainedPageV1::Page {
            image_id,
            index: 0,
            bytes: Vec::new(),
        })?);
        return Ok(payloads);
    }
    for (index, bytes) in image.chunks(chunk_len).enumerate() {
        let index = u32::try_from(index)
            .map_err(|_| KvError::Gossip("retained page index overflow".into()))?;
        let payload = encode_page(&RetainedPageV1::Page {
            image_id,
            index,
            bytes: bytes.to_vec(),
        })?;
        if payload.len() > max_payload {
            return Err(KvError::Gossip(
                "encoded retained page exceeds payload budget".into(),
            ));
        }
        payloads.push(payload);
    }
    Ok(payloads)
}

#[derive(Debug)]
pub(crate) struct RetainedPageAssembler {
    image_id: [u8; 32],
    total_len: usize,
    page_count: u32,
    pages: BTreeMap<u32, Vec<u8>>,
    received_len: usize,
}

impl RetainedPageAssembler {
    pub(crate) fn from_manifest(manifest: &RetainedPageV1) -> Result<Self> {
        let RetainedPageV1::Manifest {
            image_id,
            total_len,
            page_count,
        } = manifest
        else {
            return Err(KvError::Gossip("retained paging manifest required".into()));
        };
        let total_len = usize::try_from(*total_len)
            .map_err(|_| KvError::Gossip("retained image length overflow".into()))?;
        if total_len > MAX_RETAINED_IMAGE_BYTES
            || *page_count == 0
            || *page_count > MAX_RETAINED_PAGES
        {
            return Err(KvError::Gossip(
                "retained paging manifest exceeds limits".into(),
            ));
        }
        Ok(Self {
            image_id: *image_id,
            total_len,
            page_count: *page_count,
            pages: BTreeMap::new(),
            received_len: 0,
        })
    }

    pub(crate) fn push(&mut self, page: RetainedPageV1) -> Result<Option<Vec<u8>>> {
        let RetainedPageV1::Page {
            image_id,
            index,
            bytes,
        } = page
        else {
            return Err(KvError::Gossip("retained paging page required".into()));
        };
        if image_id != self.image_id || index >= self.page_count {
            return Err(KvError::Gossip("retained page binding mismatch".into()));
        }
        if let Some(existing) = self.pages.get(&index) {
            if existing == &bytes {
                return Ok(None);
            }
            return Err(KvError::Gossip(
                "conflicting duplicate retained page".into(),
            ));
        }
        let next_len = self
            .received_len
            .checked_add(bytes.len())
            .ok_or_else(|| KvError::Gossip("retained page byte count overflow".into()))?;
        if next_len > self.total_len || next_len > MAX_RETAINED_IMAGE_BYTES {
            return Err(KvError::Gossip(
                "retained pages exceed declared image length".into(),
            ));
        }
        self.pages.insert(index, bytes);
        self.received_len = next_len;
        if self.pages.len() != self.page_count as usize {
            return Ok(None);
        }
        let mut image = Vec::with_capacity(self.total_len);
        for index in 0..self.page_count {
            let bytes = self
                .pages
                .get(&index)
                .ok_or_else(|| KvError::Gossip("retained page missing".into()))?;
            image.extend_from_slice(bytes);
            if image.len() > self.total_len {
                return Err(KvError::Gossip(
                    "retained pages exceed declared length".into(),
                ));
            }
        }
        if image.len() != self.total_len || blake3::hash(&image).as_bytes() != &self.image_id {
            return Err(KvError::Gossip("retained image digest mismatch".into()));
        }
        Ok(Some(image))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_reassemble_out_of_order_and_reject_tamper() {
        let image = vec![7; 4096];
        let payloads = split_image(&image, 512).expect("split");
        let manifest = decode_page(&payloads[0])
            .expect("decode")
            .expect("manifest");
        let mut assembler = RetainedPageAssembler::from_manifest(&manifest).expect("assembler");
        let mut pages: Vec<_> = payloads[1..]
            .iter()
            .map(|payload| decode_page(payload).expect("decode").expect("page"))
            .collect();
        pages.reverse();
        let mut complete = None;
        for page in pages {
            complete = assembler.push(page).expect("page").or(complete);
        }
        assert_eq!(complete.as_deref(), Some(image.as_slice()));

        let mut assembler = RetainedPageAssembler::from_manifest(&manifest).expect("assembler");
        let mut pages: Vec<_> = payloads[1..]
            .iter()
            .map(|payload| decode_page(payload).expect("decode").expect("page"))
            .collect();
        if let RetainedPageV1::Page { bytes, .. } = &mut pages[0] {
            bytes[0] ^= 1;
        }
        let mut result = Ok(None);
        for page in pages {
            result = assembler.push(page);
        }
        assert!(result.is_err());
    }

    #[test]
    fn assembler_rejects_oversize_and_conflicting_duplicate_before_completion() {
        let image_id = [3; 32];
        let manifest = RetainedPageV1::Manifest {
            image_id,
            total_len: 2,
            page_count: 2,
        };
        let mut assembler = RetainedPageAssembler::from_manifest(&manifest).expect("assembler");
        assembler
            .push(RetainedPageV1::Page {
                image_id,
                index: 0,
                bytes: vec![1],
            })
            .expect("first page");
        assert_eq!(assembler.received_len, 1);
        assert!(assembler
            .push(RetainedPageV1::Page {
                image_id,
                index: 0,
                bytes: vec![2],
            })
            .is_err());
        assert_eq!(assembler.received_len, 1);
        assert_eq!(assembler.pages.get(&0).map(Vec::as_slice), Some(&[1][..]));
        assert!(assembler
            .push(RetainedPageV1::Page {
                image_id,
                index: 1,
                bytes: vec![2, 3],
            })
            .is_err());
        assert_eq!(assembler.received_len, 1);
        assert!(!assembler.pages.contains_key(&1));
    }

    #[test]
    fn pool_accepts_pages_before_manifest_and_separates_authority_bindings() {
        let image = vec![9; 2048];
        let frames = split_image(&image, 512).expect("split");
        let decoded: Vec<_> = frames
            .iter()
            .map(|frame| decode_page(frame).expect("decode").expect("paged frame"))
            .collect();
        let image_id = match decoded[0] {
            RetainedPageV1::Manifest { image_id, .. } => image_id,
            RetainedPageV1::Page { .. } => panic!("manifest first"),
        };
        let binding = RetainedPageBinding {
            store_id: [1; 32],
            endorser: [2; 32],
            authorization: [3; 32],
            image_id,
        };
        let mut pool = RetainedPagePool::default();
        for frame in decoded.iter().skip(1).cloned() {
            assert!(pool.push(binding.clone(), frame).expect("page").is_none());
        }
        let wrong_authority = RetainedPageBinding {
            authorization: [4; 32],
            ..binding.clone()
        };
        assert!(pool
            .push(wrong_authority, decoded[0].clone())
            .expect("separate manifest")
            .is_none());
        let completed = pool
            .push(binding, decoded[0].clone())
            .expect("manifest completes");
        assert_eq!(completed.as_deref(), Some(image.as_slice()));
    }

    #[test]
    fn pool_discards_pre_manifest_poison_and_recovers_accounting() {
        let image = vec![5; 32];
        let frames = split_image(&image, 256).expect("split");
        let manifest = decode_page(&frames[0]).expect("decode").expect("manifest");
        let valid_page = decode_page(&frames[1]).expect("decode").expect("page");
        let image_id = match manifest {
            RetainedPageV1::Manifest { image_id, .. } => image_id,
            RetainedPageV1::Page { .. } => panic!("manifest first"),
        };
        let binding = RetainedPageBinding {
            store_id: [1; 32],
            endorser: [2; 32],
            authorization: [3; 32],
            image_id,
        };
        let poison = RetainedPageV1::Page {
            image_id,
            index: 1,
            bytes: vec![8; 7],
        };
        let mut pool = RetainedPagePool::default();
        assert!(pool
            .push(binding.clone(), poison)
            .expect("pending page")
            .is_none());
        assert_eq!(pool.received_len, 7);

        assert!(pool.push(binding.clone(), manifest.clone()).is_err());
        assert_eq!(pool.received_len, 0);
        assert!(!pool.images.contains_key(&binding));

        assert!(pool
            .push(binding.clone(), manifest)
            .expect("fresh manifest")
            .is_none());
        let completed = pool
            .push(binding, valid_page)
            .expect("valid page after poison");
        assert_eq!(completed.as_deref(), Some(image.as_slice()));
        assert_eq!(pool.received_len, 0);
    }

    #[test]
    fn pool_enforces_image_count_byte_limit_and_ttl() {
        let mut pool = RetainedPagePool::default();
        for tag in 0..MAX_INFLIGHT_IMAGES {
            let binding = RetainedPageBinding {
                store_id: [tag as u8; 32],
                endorser: [1; 32],
                authorization: [2; 32],
                image_id: [tag as u8; 32],
            };
            pool.push(
                binding,
                RetainedPageV1::Page {
                    image_id: [tag as u8; 32],
                    index: 0,
                    bytes: vec![tag as u8],
                },
            )
            .expect("bounded pending image");
        }
        let extra = RetainedPageBinding {
            store_id: [99; 32],
            endorser: [1; 32],
            authorization: [2; 32],
            image_id: [99; 32],
        };
        assert!(pool
            .push(
                extra.clone(),
                RetainedPageV1::Page {
                    image_id: extra.image_id,
                    index: 0,
                    bytes: vec![1],
                },
            )
            .is_err());

        for pending in pool.images.values_mut() {
            pending.created = Instant::now() - INFLIGHT_TTL - std::time::Duration::from_secs(1);
        }
        assert!(pool
            .push(
                extra.clone(),
                RetainedPageV1::Page {
                    image_id: extra.image_id,
                    index: 0,
                    bytes: vec![1],
                },
            )
            .expect("expired images pruned")
            .is_none());
        assert_eq!(pool.images.len(), 1);
        assert_eq!(pool.received_len, 1);

        let large_a = RetainedPageBinding {
            store_id: [10; 32],
            endorser: [1; 32],
            authorization: [2; 32],
            image_id: [10; 32],
        };
        let large_b = RetainedPageBinding {
            store_id: [11; 32],
            image_id: [11; 32],
            ..large_a.clone()
        };
        pool.push(
            large_a.clone(),
            RetainedPageV1::Page {
                image_id: large_a.image_id,
                index: 0,
                bytes: vec![0; MAX_RETAINED_IMAGE_BYTES],
            },
        )
        .expect("first bounded image");
        assert!(pool
            .push(
                large_b.clone(),
                RetainedPageV1::Page {
                    image_id: large_b.image_id,
                    index: 0,
                    bytes: vec![0; MAX_RETAINED_IMAGE_BYTES],
                },
            )
            .is_err());
        assert!(!pool.images.contains_key(&large_b));
    }
}

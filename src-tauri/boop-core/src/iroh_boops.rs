use anyhow::{Context, Result};
use bytes::Bytes;
use iroh_docs::{
    AuthorId, DocTicket,
    api::{Doc, protocol::ShareMode},
    engine::LiveEvent,
    store::Query,
    sync::Entry,
};
use n0_future::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::iroh_manager::IrohManager;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Boop {
    pub id: String,
    pub created: u64,
    pub blob_hash: String,
    pub is_listened: bool,
    pub mime_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingBoopDto {
    pub id: String,
    pub created: u64,
    pub blob_hash: String,
    pub is_ready: bool,
    pub mime_type: String,
}

impl Boop {
    fn from_bytes(bytes: Bytes) -> Result<Self> {
        let boop = serde_json::from_slice(&bytes).context("invalid json")?;
        Ok(boop)
    }

    fn as_bytes(&self) -> Result<Bytes> {
        let buf = serde_json::to_vec(self)?;
        Ok(buf.into())
    }
}

pub struct BoopQueue {
    iroh: IrohManager,
    doc: Doc,
    ticket: DocTicket,
    author: AuthorId,
    last_entry_count: AtomicUsize,
}

impl BoopQueue {
    pub async fn new(ticket: Option<String>, iroh: IrohManager) -> Result<Self> {
        let doc = match ticket {
            None => iroh.docs().create().await?,
            Some(t) => {
                let ticket = DocTicket::from_str(&t)?;
                iroh.docs().import(ticket).await?
            }
        };

        // Share the doc so others can write to it
        let ticket = doc.share(ShareMode::Write, Default::default()).await?;
        let author = iroh.author();

        Ok(Self {
            iroh,
            doc,
            ticket,
            author,
            last_entry_count: AtomicUsize::new(0),
        })
    }

    pub fn ticket(&self) -> String {
        self.ticket.to_string()
    }

    pub async fn doc_subscribe(&self) -> Result<impl Stream<Item = Result<LiveEvent>> + use<>> {
        self.doc.subscribe().await
    }

    pub async fn send_boop(&mut self, audio_bytes: Vec<u8>, mime_type: String) -> Result<()> {
        let created = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .expect("time drift")
            .as_secs();
        let id = uuid::Uuid::new_v4().to_string();
        
        let hash = self.iroh.blobs().add_bytes(audio_bytes).await?.hash;

        let boop = Boop {
            id: id.clone(),
            created,
            blob_hash: hash.to_string(),
            is_listened: false,
            mime_type,
        };

        // Insert metadata using chronological key
        let key = format!("boops/{:020}-{id}", created);
        self.doc
            .set_bytes(self.author, key.as_bytes().to_vec(), boop.as_bytes()?)
            .await?;
            
        Ok(())
    }

    pub async fn get_pending_boops(&self) -> Result<Vec<PendingBoopDto>> {
        // 1. Gather all listened tombstones
        let mut tombstones = std::collections::HashSet::new();
        let t_entries = self.doc.get_many(Query::key_prefix("listened/")).await?;
        let mut t_entries = t_entries.collect::<Vec<Result<Entry>>>().await.into_iter();
        while let Some(Ok(entry)) = t_entries.next() {
            if let Ok(key_str) = String::from_utf8(entry.key().to_vec()) {
                let id = key_str.replace("listened/", "");
                tombstones.insert(id);
            }
        }

        // 2. Fetch pending boops, scrubbing any that have tombstones
        let entries = self.doc.get_many(Query::key_prefix("boops/")).await?;
        let mut entries = entries.collect::<Vec<Result<Entry>>>().await.into_iter();
        
        let current_count = entries.len();
        let last_count = self.last_entry_count.swap(current_count, Ordering::SeqCst);
        if current_count > last_count {
            log::info!("New boops detected! Found {} potential boop metadata entries", current_count);
        }
        
        let mut boops = Vec::new();
        
        while let Some(Ok(entry)) = entries.next() {
            if let Ok(b) = self.iroh.blobs().get_bytes(entry.content_hash()).await {
                if let Ok(boop) = Boop::from_bytes(b) {
                    if tombstones.contains(&boop.id) && entry.author() == self.author {
                        // The recipient listened to it! Delete the doc entry.
                        log::info!("Garbage collecting boop {} due to tombstone", boop.id);
                        self.doc.del(self.author, entry.key().to_vec()).await.ok();
                    } else if !boop.is_listened && entry.author() != self.author {
                        use iroh_blobs::Hash;
                        use std::str::FromStr;
                        let hash_res = Hash::from_str(&boop.blob_hash);
                        let is_ready = if let Ok(h) = hash_res {
                            let has_blob = self.iroh.blobs().has(h).await.unwrap_or(false);
                            log::debug!("Boop {}: audio blob {} presence: {}", boop.id, boop.blob_hash, has_blob);
                            has_blob
                        } else { false };

                        boops.push(PendingBoopDto {
                            id: boop.id,
                            created: boop.created,
                            blob_hash: boop.blob_hash,
                            is_ready,
                            mime_type: boop.mime_type,
                        });
                    } else if entry.author() == self.author {
                        log::trace!("Skipping boop {} as it was authored by us", boop.id);
                    }
                }
            } else {
                log::warn!("Failed to get metadata blob {} for boop entry", entry.content_hash());
            }
        }
        
        boops.sort_by_key(|b| b.created);
        Ok(boops)
    }


    pub async fn mark_listened(&self, boop_id: &str) -> Result<()> {
        log::info!("Marking boop {} as listened", boop_id);
        
        // Write the tombstone receipt so the original author knows to delete
        let key = format!("listened/{}", boop_id);
        self.doc
            .set_bytes(self.author, key.as_bytes().to_vec(), vec![1])
            .await?;
            
        Ok(())
    }
    
    pub async fn get_audio_bytes(&self, hash_str: &str) -> Result<Vec<u8>> {
        use iroh_blobs::Hash;
        use std::str::FromStr;
        let hash = Hash::from_str(hash_str)?;
        let bytes = self.iroh.blobs().get_bytes(hash).await?;
        Ok(bytes.to_vec())
    }
}

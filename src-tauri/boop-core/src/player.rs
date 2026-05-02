use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait BoopPlayer: Send + Sync {
    /// Plays the given audio bytes. Returns when playback finishes or is stopped.
    async fn play(&self, audio_bytes: Vec<u8>) -> Result<()>;
    /// Stops any ongoing playback.
    async fn stop(&self) -> Result<()>;
}

//! Which model files there are to fetch: each one's Hugging Face repository and file, its size and
//! sha256, and the shape of stream it is opened with.
//!
//! learn-harness's catalog (`src/core/dictation-local.ts`, `electron/dictation-models.cjs`): the two
//! streaming models it offers in Q8_0, the library's recommended quantization, with the sizes and
//! hashes the Hugging Face tree API gives for `handy-computer/*-gguf`, and Silero, which is fetched
//! with the first of them. A window never names a URL: it names an entry, and the URL is made here.

/// How a model's stream is opened (transcribe.cpp's stream-slot family extension).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Parakeet's chunked-attention buffered stream, in milliseconds of audio.
    Buffered { left_ms: i32, chunk_ms: i32, right_ms: i32 },
    /// A cache-aware stream and its right context, in encoder frames.
    CacheAware { att_context_right: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The name the window's preferences keep it by.
    pub id: &'static str,
    pub repo: &'static str,
    pub file: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Model {
    pub entry: Entry,
    pub shape: Shape,
}

pub const PARAKEET: Model = Model {
    entry: Entry {
        id: "parakeet-unified-en-0.6b",
        repo: "handy-computer/parakeet-unified-en-0.6b-gguf",
        file: "parakeet-unified-en-0.6b-Q8_0.gguf",
        size: 731_357_568,
        sha256: "4b50b6dd862bf6e346929aaf4f5eaacec003bfa3f56462d6c874b41ef2f38795",
    },
    // The (70,2,4) menu entry learn-harness's owner chose: 160 ms chunks and 320 ms of lookahead.
    shape: Shape::Buffered { left_ms: 5600, chunk_ms: 160, right_ms: 320 },
};

pub const NEMOTRON: Model = Model {
    entry: Entry {
        id: "nemotron-3.5-asr-streaming-0.6b",
        repo: "handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf",
        file: "nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf",
        size: 751_094_240,
        sha256: "b94545b313b3223fda7b2857a52681da813935c2127643d1e9ff0c23d988089c",
    },
    // Six frames of right context: the same ~480 ms of lookahead as Parakeet's.
    shape: Shape::CacheAware { att_context_right: 6 },
};

pub const MODELS: [Model; 2] = [PARAKEET, NEMOTRON];

/// Silero VAD v6.2: fetched with the first model, 1.2 MB, and run on the CPU.
pub const VAD: Entry = Entry {
    id: "silero-vad-v6.2",
    repo: "handy-computer/silero-vad-v6.2-gguf",
    file: "silero-vad-v6.2-F32.gguf",
    size: 1_241_056,
    sha256: "0d871a800c420fdd5d1b17a3aea4296d1c39608f0ff4a9295eb76457eb1c88e6",
};

pub fn model(id: &str) -> Option<Model> {
    MODELS.into_iter().find(|model| model.entry.id == id)
}

impl Entry {
    pub fn url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/main/{}", self.repo, self.file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_is_named_once_and_fetched_from_its_own_repository() {
        let mut ids: Vec<&str> = MODELS.iter().map(|model| model.entry.id).chain([VAD.id]).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), MODELS.len() + 1);
        for entry in MODELS.iter().map(|model| model.entry).chain([VAD]) {
            assert_eq!(entry.sha256.len(), 64, "{}", entry.id);
            assert!(entry.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{}", entry.id);
            assert!(entry.url().starts_with("https://huggingface.co/handy-computer/"), "{}", entry.id);
            assert!(entry.url().ends_with(entry.file));
        }
    }

    #[test]
    fn parakeet_streams_buffered_and_nemotron_cache_aware() {
        assert_eq!(model("parakeet-unified-en-0.6b").map(|m| m.shape), Some(Shape::Buffered { left_ms: 5600, chunk_ms: 160, right_ms: 320 }));
        assert_eq!(model("nemotron-3.5-asr-streaming-0.6b").map(|m| m.shape), Some(Shape::CacheAware { att_context_right: 6 }));
        assert_eq!(model("whisper"), None);
    }
}

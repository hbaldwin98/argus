//! Turning this machine's microphone into text, on this machine: the microphone, the voice gate, the
//! speech engine, the model files and the cleanup of what they write.
//!
//! Its own crate so that ggml's native build (CMake and a C++ compiler; Vulkan or CUDA when asked)
//! stays out of `argus-desktop-core`, which every default `cargo test` builds. The engine is
//! transcribe.cpp, which Handy (cjpais/Handy) runs, and its constants and rules are Handy's by way of
//! learn-harness's Electron port.

pub mod capture;
pub mod engine;
pub mod models;
pub mod text;
pub mod vad;

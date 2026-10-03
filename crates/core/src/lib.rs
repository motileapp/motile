//! The part of every Motile app that isn't UI: this device's account, the connections to its
//! servers, the local copy of their threads, and turning transcripts into rows ready to draw.

pub mod api;
pub mod browse;
pub mod cache;
pub mod connection;
pub mod core;
pub mod ffi;
pub mod git;
pub mod link;
pub mod media;
pub mod render;

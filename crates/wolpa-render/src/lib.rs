//! ## wolpa-render — macOS Metal rendering
//!
//! This crate renders the grid state (from `wolpa-core`) onto the screen using
//! Apple's Metal GPU API for fast 2D text rendering.

#![allow(unexpected_cfgs)]

pub mod atlas;
pub mod font;
pub mod layout;
pub mod metal;

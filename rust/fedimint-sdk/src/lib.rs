//! High-level Fedimint client SDK. API skeleton per fedimint-sdk#344.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(missing_debug_implementations)]
// Skeleton-phase allowances — remove both when implementation starts. Parameters
// are deliberately named (they are rustdoc-visible API contract) but unused, and
// the private placeholder `inner` fields are never constructed or read while
// every body is unimplemented!(). CI runs with RUSTFLAGS="-D warnings"
// (section 3), so these must be in-source allows, not tolerated warnings:
#![allow(unused_variables)]
#![allow(dead_code)]

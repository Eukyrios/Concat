// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer Abdullah and Concat contributors

//! Everything the `.slint` tree exports: the `App` window, its globals and its
//! data structs.
//!
//! This crate exists only to hold the Slint compiler's output, so that it is
//! compiled apart from the app's Rust. See this crate's manifest.

// The workspace lints every public item for documentation, and the generated
// accessors are thousands of public items nobody documents. The allow covers
// them and nothing else: this file is all there is.
#![allow(missing_docs)]

slint::include_modules!();

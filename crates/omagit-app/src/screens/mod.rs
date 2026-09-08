//! The product screens.
//!
//! They live in `omagit-app` rather than in `omagit-ui` because a screen is
//! state and routing, which is this crate's job (SPEC §7); what a screen draws
//! *with* — the palette, the icons, the atoms of board 01 — comes from
//! `omagit-ui`, which knows nothing about repositories.
//!
//! Repositories is the first (M3). Working Copy (M4) and History (M6) join it,
//! and the topbar routes between them.

pub mod repositories;
pub mod working_copy;

pub use repositories::RepositoriesScreen;
pub use working_copy::WorkingCopyScreen;

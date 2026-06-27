//! Concrete fairy variants built on the generic [`WideVariant`] layer.
//!
//! Each module here is a zero-sized [`WideVariant`] rule layer plus a
//! [`GenericPosition`] type alias for it. The first is [`makruk`] (Makruk / Thai
//! chess), the opening Phase 1 fairy variant per
//! `docs/fairy-variants-architecture.md`; later variants land alongside it.
//!
//! [`WideVariant`]: super::WideVariant
//! [`GenericPosition`]: super::GenericPosition

pub mod capablanca;
pub mod makruk;

pub use capablanca::{Capablanca, CapablancaRules};
pub use makruk::{Makruk, MakrukRules};

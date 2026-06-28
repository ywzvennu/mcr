//! Concrete fairy variants built on the generic [`WideVariant`] layer.
//!
//! Each module here is a zero-sized [`WideVariant`] rule layer plus a
//! [`GenericPosition`] type alias for it. The first is [`makruk`] (Makruk / Thai
//! chess), the opening Phase 1 fairy variant per
//! `docs/fairy-variants-architecture.md`; later variants land alongside it.
//!
//! [`WideVariant`]: super::WideVariant
//! [`GenericPosition`]: super::GenericPosition

pub mod cambodian;
pub mod capablanca;
pub mod chak;
pub mod duck;
pub mod empire;
pub mod grand;
pub mod hoppelpoppel;
pub mod janggi;
pub mod knightmate;
pub mod makruk;
pub mod manchu;
pub mod minishogi;
pub mod minixiangqi;
pub mod orda;
pub mod ordamirror;
pub mod seirawan;
pub mod shako;
pub mod shatar;
pub mod shinobi;
pub mod shogi;
pub mod shogun;
pub mod sittuyin;
pub mod spartan;
pub mod synochess;
pub mod tori;
pub mod xiangqi;

pub use cambodian::{Cambodian, CambodianRules};
pub use capablanca::{Capablanca, CapablancaRules};
pub use chak::{Chak, ChakRules};
pub use duck::{Duck, DuckRules};
pub use empire::{Empire, EmpireRules};
pub use grand::{Grand, GrandRules};
pub use hoppelpoppel::{HoppelPoppel, HoppelPoppelRules};
pub use janggi::{Janggi, JanggiRules};
pub use knightmate::{Knightmate, KnightmateRules};
pub use makruk::{Makruk, MakrukRules};
pub use manchu::{Manchu, ManchuRules};
pub use minishogi::{Minishogi, MinishogiRules};
pub use minixiangqi::{Minixiangqi, MinixiangqiRules};
pub use orda::{Orda, OrdaRules};
pub use ordamirror::{Ordamirror, OrdamirrorRules};
pub use seirawan::{Seirawan, SeirawanRules};
pub use shako::{Shako, ShakoRules};
pub use shatar::{Shatar, ShatarRules};
pub use shinobi::{Shinobi, ShinobiRules};
pub use shogi::{Shogi, ShogiRules};
pub use shogun::{Shogun, ShogunRules};
pub use sittuyin::{Sittuyin, SittuyinRules};
pub use spartan::{Spartan, SpartanRules};
pub use synochess::{Synochess, SynochessRules};
pub use tori::{Tori, ToriRules};
pub use xiangqi::{Xiangqi, XiangqiRules};

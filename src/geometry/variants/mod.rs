//! Concrete fairy variants built on the generic [`WideVariant`] layer.
//!
//! Each module here is a zero-sized [`WideVariant`] rule layer plus a
//! [`GenericPosition`] type alias for it. The first is [`makruk`] (Makruk / Thai
//! chess), the opening Phase 1 fairy variant per
//! `docs/fairy-variants-architecture.md`; later variants land alongside it.
//!
//! [`WideVariant`]: super::WideVariant
//! [`GenericPosition`]: super::GenericPosition

pub mod alice;
pub mod asean;
pub mod bughouse;
pub mod cambodian;
pub mod cannonshogi;
pub mod capablanca;
pub mod capahouse;
pub mod chak;
pub mod chennis;
pub mod dobutsu;
pub mod dragon;
pub mod duck;
pub mod empire;
pub mod fogofwar;
pub mod gorogoro;
pub mod grand;
pub mod grandhouse;
pub mod hoppelpoppel;
pub mod janggi;
pub mod khans;
pub mod knightmate;
pub mod kyotoshogi;
pub mod makpong;
pub mod makruk;
pub mod manchu;
pub mod mansindam;
pub mod minishogi;
pub mod minixiangqi;
pub mod orda;
pub mod ordamirror;
pub mod placement;
pub mod seirawan;
pub mod shako;
pub mod shatar;
pub mod shatranj;
pub mod shinobi;
pub mod shogi;
pub mod shogun;
pub mod shoshogi;
pub mod shouse;
pub mod sittuyin;
pub mod spartan;
pub mod synochess;
pub mod tori;
pub mod xiangfu;
pub mod xiangqi;

pub use alice::{Alice, AliceRules};
pub use asean::{Asean, AseanRules};
pub use bughouse::{Bughouse, BughouseRules};
pub use cambodian::{Cambodian, CambodianRules};
pub use cannonshogi::{CannonShogi, CannonShogiRules};
pub use capablanca::{Capablanca, CapablancaRules};
pub use capahouse::{Capahouse, CapahouseRules};
pub use chak::{Chak, ChakRules};
pub use chennis::{Chennis, ChennisRules};
pub use dobutsu::{Dobutsu, DobutsuRules};
pub use dragon::{Dragon, DragonRules};
pub use duck::{Duck, DuckRules};
pub use empire::{Empire, EmpireRules};
pub use fogofwar::{FogOfWar, FogOfWarRules};
pub use gorogoro::{Gorogoro, GorogoroRules};
pub use grand::{Grand, GrandRules};
pub use grandhouse::{Grandhouse, GrandhouseRules};
pub use hoppelpoppel::{HoppelPoppel, HoppelPoppelRules};
pub use janggi::{Janggi, JanggiRules};
pub use khans::{Khans, KhansRules};
pub use knightmate::{Knightmate, KnightmateRules};
pub use kyotoshogi::{Kyotoshogi, KyotoshogiRules};
pub use makpong::{Makpong, MakpongRules};
pub use makruk::{Makruk, MakrukRules};
pub use manchu::{Manchu, ManchuRules};
pub use mansindam::{Mansindam, MansindamRules};
pub use minishogi::{Minishogi, MinishogiRules};
pub use minixiangqi::{Minixiangqi, MinixiangqiRules};
pub use orda::{Orda, OrdaRules};
pub use ordamirror::{Ordamirror, OrdamirrorRules};
pub use placement::{Placement, PlacementRules};
pub use seirawan::{Seirawan, SeirawanRules};
pub use shako::{Shako, ShakoRules};
pub use shatar::{Shatar, ShatarRules};
pub use shatranj::{Shatranj, ShatranjRules};
pub use shinobi::{Shinobi, ShinobiRules};
pub use shogi::{Shogi, ShogiRules};
pub use shogun::{Shogun, ShogunRules};
pub use shoshogi::{ShoShogi, ShoShogiRules};
pub use shouse::{Shouse, ShouseRules};
pub use sittuyin::{Sittuyin, SittuyinRules};
pub use spartan::{Spartan, SpartanRules};
pub use synochess::{Synochess, SynochessRules};
pub use tori::{Tori, ToriRules};
pub use xiangfu::{Xiangfu, XiangfuRules};
pub use xiangqi::{Xiangqi, XiangqiRules};

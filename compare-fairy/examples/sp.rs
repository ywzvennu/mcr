use mce::geometry::{AnyWideVariant, WideVariantId};
fn main() {
    let p = AnyWideVariant::startpos(WideVariantId::Seirawan);
    println!("startpos: {}", p.to_fen());
}

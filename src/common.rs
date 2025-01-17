pub const ATTACK: f32 = 1.0;
pub const RELEASE: f32 = 1.0;

pub fn to_db(val: f32) -> f32 { 20.0 * val.log10() }

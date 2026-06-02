pub(super) fn hash_cell(x: usize, y: usize, seed: u64) -> u64 {
    let mut value = seed;
    value ^= (x as u64).wrapping_mul(0x9e3779b97f4a7c15);
    value ^= (y as u64).wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

pub(super) fn hash_to_unit(hash: u64) -> f64 {
    const SCALE: f64 = 1.0 / ((1_u64 << 53) as f64);
    ((hash >> 11) as f64) * SCALE
}

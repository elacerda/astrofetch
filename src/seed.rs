//! Stable feature-seed derivation for procedural generation.
//!
//! The base scene RNG is intentionally kept separate from feature RNGs. New
//! procedural features derive independent sub-seeds from the user-visible base
//! seed plus a versioned namespace. This prevents adding or reordering one
//! feature from silently advancing another feature's RNG stream.

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;
const FEATURE_SEED_DOMAIN_V1: &[u8] = b"astrofetch.feature-seed.v1\0";

/// Versioned namespace reserved for the first barred-spiral implementation.
pub const SPIRAL_BAR_V1: &str = "spiral/bar/v1";

/// Seed context shared with procedural generators without exposing or advancing
/// the legacy scene RNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationContext {
    base_seed: u64,
}

impl GenerationContext {
    pub const fn new(base_seed: u64) -> Self {
        Self { base_seed }
    }

    pub const fn base_seed(self) -> u64 {
        self.base_seed
    }

    pub fn feature_seed(self, namespace: &str) -> u64 {
        derive_feature_seed(self.base_seed, namespace)
    }
}

/// Derives a stable feature seed from a scene seed and a versioned namespace.
///
/// This algorithm is part of AstroFetch's deterministic-generation contract.
/// Keep it stable unless the domain version is deliberately changed. Feature
/// namespaces should also be versioned so a future implementation can opt into
/// a new stream without perturbing unrelated features.
///
/// The derivation uses fixed FNV-1a byte hashing followed by a SplitMix64 final
/// avalanche. It does not depend on Rust's `Hash` implementations or randomized
/// hash state, so call order and platform hash configuration cannot affect it.
pub fn derive_feature_seed(base_seed: u64, namespace: &str) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    hash = fnv1a_update(hash, FEATURE_SEED_DOMAIN_V1);
    hash = fnv1a_update(hash, &base_seed.to_le_bytes());
    hash = fnv1a_update(hash, namespace.as_bytes());
    splitmix64(hash)
}

fn fnv1a_update(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{RngExt, SeedableRng};

    #[test]
    fn test_generation_context_preserves_base_seed() {
        let context = GenerationContext::new(42);
        assert_eq!(context.base_seed(), 42);
        assert_eq!(
            context.feature_seed(SPIRAL_BAR_V1),
            derive_feature_seed(42, SPIRAL_BAR_V1)
        );
    }

    #[test]
    fn test_feature_seed_fixed_anchors() {
        assert_eq!(derive_feature_seed(0, SPIRAL_BAR_V1), 0x51facbdfc907bb45);
        assert_eq!(derive_feature_seed(4, SPIRAL_BAR_V1), 0x78326439aca3a060);
        assert_eq!(derive_feature_seed(16, SPIRAL_BAR_V1), 0x9bcdc08f70035478);
        assert_eq!(derive_feature_seed(42, SPIRAL_BAR_V1), 0x13ca583ea0675dce);
    }

    #[test]
    fn test_feature_seed_namespaces_are_independent() {
        let base_seed = 42;
        let bar = derive_feature_seed(base_seed, SPIRAL_BAR_V1);
        let other = derive_feature_seed(base_seed, "test/other/v1");

        assert_ne!(bar, other);
    }

    #[test]
    fn test_feature_rng_streams_do_not_depend_on_construction_order() {
        let base_seed = 42;
        let bar_seed = derive_feature_seed(base_seed, SPIRAL_BAR_V1);
        let other_seed = derive_feature_seed(base_seed, "test/other/v1");

        let mut bar_first = StdRng::seed_from_u64(bar_seed);
        let mut other_second = StdRng::seed_from_u64(other_seed);
        let bar_first_values = [bar_first.random::<u64>(), bar_first.random::<u64>()];
        let other_second_values = [other_second.random::<u64>(), other_second.random::<u64>()];

        let mut other_first = StdRng::seed_from_u64(other_seed);
        let mut bar_second = StdRng::seed_from_u64(bar_seed);
        let other_first_values = [other_first.random::<u64>(), other_first.random::<u64>()];
        let bar_second_values = [bar_second.random::<u64>(), bar_second.random::<u64>()];

        assert_eq!(bar_first_values, bar_second_values);
        assert_eq!(other_second_values, other_first_values);
    }

    #[test]
    fn test_feature_seed_changes_with_base_seed() {
        assert_ne!(
            derive_feature_seed(41, SPIRAL_BAR_V1),
            derive_feature_seed(42, SPIRAL_BAR_V1)
        );
    }
}

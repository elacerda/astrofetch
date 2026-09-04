# Deterministic procedural generation

AstroFetch treats the user-visible scene seed and feature-specific randomness as separate concerns.

## Compatibility contract

For a fixed AstroFetch implementation and dependency set, the same explicit `--seed` must reproduce the same procedural choices.

New optional procedural features must not advance the legacy scene RNG merely because they are added, removed, or constructed in a different order. Instead, each feature derives an independent sub-seed from:

- the scene's base seed; and
- a stable, versioned namespace such as `spiral/bar/v1`.

The derivation is implemented by `derive_feature_seed` in `src/seed.rs`. The algorithm uses fixed byte hashing plus a fixed SplitMix64 avalanche and does not use Rust's `Hash` implementations or randomized hash state.

`GenerationContext` carries the base scene seed separately from the legacy `StdRng`. The engine creates this context from the already-resolved scene seed and passes it to the Spiral generator. The context does not itself consume randomness; future optional morphology derives feature seeds from it.

Feature namespaces are versioned deliberately. If a future implementation needs a different random stream for the same feature, it should opt into a new namespace such as `spiral/bar/v2` rather than silently changing unrelated feature streams.

## Legacy Spiral checkpoint

The Phase 0 baseline is `main` commit `f036c2b230dc5a1faf6f9dcb2614b12d0e7726e8` (`feat: prepare AstroFetch v0.4.0`).

The Spiral tests freeze the RNG checkpoint formed by:

1. all current `SpiralGalaxyConfig` draws; and
2. the immediately following OpenSimplex `noise_seed` draw.

These exact RNG checkpoints are anchored for seeds 4, 16, and 42. A separate test verifies that consuming an isolated feature RNG does not change the legacy configuration or OpenSimplex seed.

## Visual baseline

Phase 0 also freezes the final no-color Spiral terminal output at 40×20 for seeds 4, 16, and 42 using the existing HalfBlock, Shade, and ASCII renderers. The test hashes the final UTF-8 terminal lines after generation, normalization, stretch, threshold selection, background-star policy, and rendering.

The visual baseline deliberately covers renderer output rather than hashing the intermediate floating-point density map. This protects the visible behavior that later renderer-preserving refactors must maintain while avoiding an unnecessary bitwise contract on every intermediate floating-point operation.

Intentional morphology changes such as barred spirals are expected to update visual expectations only when their changed behavior is explicitly accepted. The legacy RNG checkpoint remains a separate guard against accidentally perturbing unrelated random streams.

## Scope of the guarantee

This contract protects feature-stream isolation and the explicitly anchored derivation algorithm. It does not promise that every AstroFetch release will render every seed byte-for-byte forever: intentional model changes can alter morphology.

`StdRng` is also an implementation supplied by `rand`; AstroFetch does not currently claim eternal cross-version compatibility across arbitrary future RNG dependency changes. If that guarantee becomes a product requirement, the base RNG algorithm should be pinned explicitly rather than inferred from `StdRng`.

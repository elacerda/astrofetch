# Elliptical morphology research for AstroFetch v2

This note defines the morphology targets for the next Elliptical model before production rendering changes are made. The goal is not photometric realism; it is to preserve recognizable elliptical-galaxy structure while producing visibly different terminal-scale morphologies from different deterministic seeds.

## Observational constraints worth preserving

Real ellipticals are not a single universal smooth profile. Their projected light distributions span a continuous range of concentrations that are commonly described with Sérsic-like profiles. Luminous giant ellipticals often show higher Sérsic indices, partially depleted cores, rounder projected shapes, and boxier isophotes, while lower-luminosity normal ellipticals tend to be flatter, coreless or centrally enhanced, and more often disky in their isophotal distortions.

Extended outer envelopes are also astrophysically meaningful. cD / brightest-cluster galaxies can retain an inner elliptical-like profile while declining much more slowly at large radius, producing a very broad low-surface-brightness halo. Rare merger remnants may also carry shells, ripples, tidal debris, or dust lanes, but those features should remain uncommon rather than defining the whole family.

At terminal resolution, the most useful signals are therefore low-order and large-scale: silhouette, axis ratio, orientation, radial concentration, core/cusp behavior, outer-envelope strength, and mild boxy/disky departures from a perfect ellipse.

## Reference categories

The v2 generator should be able to represent at least these broad visual families:

1. **Compact / disky elliptical** — moderately flattened, relatively compact, centrally concentrated, little outer envelope, slightly disky isophotes.
2. **Classical elliptical** — intermediate axis ratio, smooth radial decline, weak or neutral isophotal distortion, no strong special feature.
3. **Giant / boxy elliptical** — rounder on average, broader profile, possible soft/depleted core, slightly boxy isophotes.
4. **cD-like giant** — smooth bright body plus a faint, much broader outer envelope.

These are not intended as hard astrophysical classes. They are correlated procedural families that keep generated parameter combinations plausible while still allowing continuous variation inside each family.

## Procedural mapping

| Observed characteristic | Procedural analogue | Initial design target |
|---|---|---|
| round vs elongated projection | `axis_ratio` | bounded continuous `b/a`, roughly 0.52–0.98 overall |
| orientation on sky | `position_angle` | uniform over `[0, π)` |
| compact vs diffuse body | `effective_radius` / radial scale | family-dependent continuous range |
| central concentration | `profile_index` | Sérsic-inspired shape parameter, roughly 2–6 initially |
| depleted/soft core vs cusp | `central_structure` | bounded core softening or small central excess component |
| ordinary vs giant envelope | `outer_halo_strength`, `outer_halo_scale` | second broad, smooth, low-amplitude component |
| boxy ↔ classical ↔ disky isophotes | `isophote_shape` | small fourth-order angular term, centered near zero |
| very weak lopsidedness | `asymmetry` | defer until base morphology is validated; keep low-amplitude |
| shells / ripples | rare radial substructure | defer to rare-feature phase |
| dust lane | rare localized extinction | defer to rare-feature phase |

## Recommended v2 parameterization

The first production version should derive an explicit `EllipticalGalaxyConfig` (name not mandatory) from a versioned deterministic feature namespace such as `elliptical/morphology/v2`.

Recommended fields:

```text
family
axis_ratio
position_angle
effective_radius
profile_index
central_structure
outer_halo_strength
outer_halo_scale
isophote_shape
```

The important design choice is correlation, not merely adding more independent random numbers. For example:

- compact/disky scenes should preferentially be flatter, lower-envelope, and slightly positive in the disky shape term;
- giant/boxy scenes should preferentially be rounder, broader, more likely to have a soft core, and slightly negative in the boxy shape term;
- cD-like scenes should strongly favor an extended smooth envelope.

Exact ranges and weights must be tuned from visual seed sweeps rather than frozen from this note.

## Radial profile

Replace the current fixed double-Gaussian body with a numerically stable Sérsic-inspired radial law. Photometric exactness is not a requirement; terminal-visible differences are.

A normalized form equivalent in spirit to

```text
I_body(r) = exp(-k(n) * f(r / Re, n))
```

is sufficient if it gives monotonic, smooth concentration changes and behaves well near the origin. The implementation should avoid singular central behavior and should remain finite for every supported parameter value.

Core/cusp behavior should be implemented as a separate low-order central modifier rather than by allowing extreme profile-index values to do all the work.

## Boxy and disky isophotes

Use a subtle fourth-order angular distortion rather than high-frequency noise. A practical terminal-scale analogue is a small `cos(4θ)` perturbation to elliptical radius. The sign convention should be documented and visually verified so that one sign is consistently disky and the opposite sign boxy.

The perturbation should stay small enough that the object never develops corners, arms, or a visible thin disk. The value is in producing different contour character, not in making the effect obvious at first glance.

## Outer envelopes

A second broad component is justified for giant/cD-like outputs. It should:

- have a larger radial scale than the main body;
- remain substantially fainter than the main body;
- decline smoothly;
- be absent or very weak in most compact/classical scenes.

This gives AstroFetch a way to produce both tight ellipticals and large diffuse cluster-central-like systems without adding noise.

## Rare structures

Shells and dust lanes are real but should not be part of the v2 core gate. NASA/Hubble examples such as NGC 474 and NGC 3923 show shell systems associated with merger history, while NGC 1316 and NGC 2768 show that dust can occur in ellipticals. These belong in separate, versioned deterministic streams after the smooth morphology is already strong.

A useful product rule is: most ellipticals should remain clean and smooth; unusual structure should feel unusual.

## Determinism and compatibility

Elliptical v2 is an intentional morphology evolution, so exact v1.0.1 Elliptical pixels do not need to remain frozen. However:

- the same seed + same version/config must remain deterministic;
- Elliptical v2 randomness should use isolated versioned feature namespaces;
- unrelated Spiral, Cluster, Starfield, and model-resolution RNG behavior must not change;
- any texture/grain or future rare feature should receive its own deterministic namespace instead of consuming unrelated feature streams.

A likely implementation path is to pass the existing `GenerationContext` into Elliptical generation, mirroring the isolation already used for Spiral features.

## Terminal-resolution priorities

Prioritize, in this order:

1. axis-ratio and orientation diversity;
2. radial concentration and effective-radius diversity;
3. core/cusp behavior;
4. outer-envelope strength;
5. boxy/disky isophote shape;
6. only then weak asymmetry and rare structures.

If a mathematically realistic effect is not distinguishable at normal AstroFetch dimensions, it should not complicate the generator.

## Gate for implementation

Before merging Elliptical v2 core work, a fixed seed panel should demonstrate that unlabeled outputs are visibly distinguishable at normal terminal sizes while all still read as ellipticals. The initial panel should include seeds `0 1 2 3 4 5 8 13 16 21 42 64 99 128`, in color and `--no-color`, with the default renderer plus Shade and ASCII where relevant.

The core morphology gate is satisfied when:

- different seeds produce clearly different silhouettes/profile character;
- no family routinely resembles a spiral or lenticular disk;
- diffuse/cD-like cases remain smooth rather than noisy;
- no single special feature dominates the population;
- deterministic repeatability is preserved.

## References

- Alister W. Graham, *A review of elliptical and disc galaxy structure, and modern scaling laws* (NED): https://ned.ipac.caltech.edu/level5/Sept11/Graham/Graham_contents.html
- John Kormendy, *Elliptical Galaxies and Bulges of Disk Galaxies: Summary of Progress and Outstanding Issues* (NED): https://ned.ipac.caltech.edu/level5/March16/Kormendy/Kormendy4.html
- C. Struck, discussion of cD-galaxy envelopes in *Galaxy Collisions* (NED): https://ned.ipac.caltech.edu/level5/Struck/St9.html
- NASA/Hubble, NGC 474 shell galaxy: https://science.nasa.gov/missions/hubble/hubble-peers-through-giant-ellipticals-layers/
- NASA/Hubble, NGC 3923 shell system: https://science.nasa.gov/missions/hubble/hubble-spots-the-layers-of-ngc-3923/
- NASA/Hubble, NGC 1316 dust structure: https://science.nasa.gov/missions/hubble/elliptical-galaxy/
- NASA/Hubble, NGC 2768 dusty elliptical: https://science.nasa.gov/missions/hubble/hubble-catches-dusty-detail-in-elliptical-galaxy-ngc-2768/

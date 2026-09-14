# Startup performance baseline (Phase 0, post-v1.0)

Measured during the Phase 0 hardening cycle to establish a reproducible
startup baseline. No optimization work is implied by this record.

## Environment

- Host: Linux x86_64 (GNU/Linux), Tufao workstation
- Binary: `target/release/astrofetch` built from `main` (post-v1.0.0 hardening branch)
- Update check disabled on all runs (`--no-update-check`)
- Method: wall-clock milliseconds per invocation via `date +%s%N`, 11
  repetitions per case, median reported (no `hyperfine` available)
- Cache isolation: `XDG_CACHE_HOME` pointed at a dedicated directory
  (`~/.cache/astrofetch` layout: `packages`, `gpu`, `resolution`, `cosmetics`)

## Commands

```text
astrofetch
astrofetch --compact
astrofetch --logo-only
astrofetch --info-only
astrofetch --logo-only --model spiral --renderer half-block --seed 42
astrofetch --logo-only --model spiral --renderer quadrant --seed 42
```

(all with `--no-update-check`)

## Results (medians, ms)

| Case                              | Warm cache | Cold cache |
|-----------------------------------|-----------:|-----------:|
| Default (full display)            |         56 |         54 |
| `--compact`                       |         36 |         36 |
| `--logo-only`                     |          4 |          2 |
| `--info-only`                     |         53 |         54 |
| Spiral HalfBlock, logo-only, 42   |          4 |        n/a |
| Spiral Quadrant, logo-only, 42    |          6 |        n/a |

Full runs: min/median/max for the warm default case: 54/56/57 ms.

## Findings

- Full default startup is ~56 ms wall time; this is acceptable for shell
  startup use and no post-v1.0 regression was observed.
- Rendering is cheap: logo-only paths are 2–6 ms. The `--info-only` path
  (~53 ms) dominates the full-display cost, i.e. system information
  collection is the main startup expense.
- The system-information cache provides only a marginal benefit in this
  environment (~2 ms between cold and warm default runs); collection on
  this host is already fast. No optimization is required.
- Quadrant rendering adds ~2 ms over HalfBlock at 40×20 for the same seed.

## Conclusion

No optimization required. Re-measure after any change that touches system
collection, rendering, or startup I/O, and compare against this table.

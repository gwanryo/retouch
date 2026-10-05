#!/usr/bin/env bash
# Determinism lint (master plan §5). Engine code must not call std float transcendentals or FMA:
# they lower to platform libm / hardware paths that differ between targets. All such math goes
# through the pure-Rust `libm` crate with `force-soft-floats`, which (libm 0.2.16 configure.rs)
# disables both the `arch` cfg (hardware sqrt/fma/rint) and the `intrinsics` cfg even when some
# other crate turns the `arch` feature on. libm's own internals are allowed; our code may not
# call `libm::fma*` or `mul_add` explicitly. Exact IEEE ops (sqrt, abs, floor, ...) are allowed.
set -euo pipefail
cd "$(dirname "$0")/.."
fail=0
src=()
for d in engine/core/src engine/core/tests engine/cli/src engine/cli/tests engine/wasm/src engine/wasm/tests; do
  [ -d "$d" ] && src+=("$d")
done

fns='powf|powi|exp|exp2|exp_m1|ln|ln_1p|log|log2|log10|sin|cos|tan|asin|acos|atan|atan2|sinh|cosh|tanh|asinh|acosh|atanh|cbrt|hypot|mul_add|sin_cos|to_degrees|to_radians'
# method calls, tolerating whitespace: `x.powf(`, `x . powf (`
if grep -rnE --include='*.rs' "\.[[:space:]]*($fns)[[:space:]]*\(" "${src[@]}"; then
  echo "determinism lint: std float method found; use libm::* instead" >&2; fail=1
fi
# UFCS: `f32::powf(`, `f64 :: exp (`
if grep -rnE --include='*.rs' "\bf(32|64)[[:space:]]*::[[:space:]]*($fns)[[:space:]]*\(" "${src[@]}"; then
  echo "determinism lint: std float function (UFCS) found; use libm::* instead" >&2; fail=1
fi
if grep -rnE --include='*.rs' 'libm[[:space:]]*::[[:space:]]*fmaf?\b|\bfmaf?[[:space:]]*\(' "${src[@]}"; then
  echo "determinism lint: explicit FMA is forbidden" >&2; fail=1
fi

# No target-cpu / target-feature / fast-math in any cargo config, nor in the environment.
if find . -path ./engine/target -prune -o -path ./web/node_modules -prune -o \
     \( -path '*/.cargo/config' -o -path '*/.cargo/config.toml' \) -print | grep -q .; then
  echo "determinism lint: .cargo/config(.toml) is not allowed in this repo" >&2; fail=1
fi
for v in RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_RUSTFLAGS; do
  if [ -n "${!v:-}" ]; then
    echo "determinism lint: $v must be empty (is '${!v}')" >&2; fail=1
  fi
done

# Dependency features of the shipped (normal-edge) graph. Dev-dependencies such as
# wasm-bindgen-test -> num-traits turn libm's `arch` feature on in test builds; that is inert
# because `force-soft-floats` overrides it, which is why force-soft-floats is required below.
tree_jpeg=$(cargo tree --locked --manifest-path engine/Cargo.toml --workspace -e features,normal -i zune-jpeg)
if grep -qE 'zune-jpeg feature "(x86|neon|portable_simd|default)"' <<<"$tree_jpeg"; then
  echo "$tree_jpeg" >&2
  echo "determinism lint: zune-jpeg SIMD/default feature is enabled" >&2; fail=1
fi
tree_libm=$(cargo tree --locked --manifest-path engine/Cargo.toml --workspace -e features,normal -i libm)
if grep -qE 'libm feature "(arch|default)"' <<<"$tree_libm"; then
  echo "$tree_libm" >&2
  echo "determinism lint: libm arch/default feature is enabled" >&2; fail=1
fi
if ! grep -q 'libm feature "force-soft-floats"' <<<"$tree_libm"; then
  echo "determinism lint: libm force-soft-floats is not enabled" >&2; fail=1
fi
if cargo tree --locked --manifest-path engine/Cargo.toml --workspace -e all -i image >/dev/null 2>&1; then
  echo "determinism lint: the image crate must not be a dependency (zune-jpeg + png only)" >&2; fail=1
fi

[ "$fail" -eq 0 ] && echo "determinism lint: ok"
exit "$fail"

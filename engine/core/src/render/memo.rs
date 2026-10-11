//! Input-byte memo for [`super::render_rgba8`] (Plan 1b-2 Task 6a, bit-identical).
//!
//! On 8-bit input, stages 1-5 are a pure function of a pixel's three bytes `(r, g, b)`: stages
//! 1-3 are a per-channel LUT and stages 4-5 are per-pixel arithmetic with no position, seed or
//! neighbour input. Equal bytes therefore give equal output bytes, and a cache keyed on the bytes
//! returns exactly what recomputing would. Only misses run the stages, the finiteness check and
//! `buffer::quantize`, so the first non-finite pixel is always met on a miss.
//!
//! VALID ONLY while stages 1-5 are per-pixel and position-independent. Spatial or positional
//! stages (clarity, texture, dehaze, vignette, grain, local masks, Plan 1b split 3 onward) break
//! that premise: when they land, move the cache in front of them, i.e. cache the stage-5 output
//! as `[f32; 3]` and run the spatial stages on every pixel afterwards (the "EF" shape in the
//! Task 6 profile).

/// log2 of the slot count. 2^18 slots of 8 bytes = 2 MiB, allocated per render call.
const SLOT_BITS: u32 = 18;
const SLOTS: usize = 1 << SLOT_BITS;
/// Fibonacci hashing multiplier (2^32 / golden ratio, odd).
const HASH_MUL: u32 = 0x9E37_79B1;
/// Set in every stored key so that an all-zero slot never matches black `(0, 0, 0)`.
const VALID: u32 = 1 << 24;

/// Pixels rendered through the cache before deciding whether to keep it.
pub(super) const PROBE_PIXELS: usize = 65_536;

/// Cache key of one input pixel, valid bit included.
pub(super) fn key(r: u8, g: u8, b: u8) -> u32 {
    u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16 | VALID
}

/// Direct-mapped slot of `key`: the top [`SLOT_BITS`] bits of the Fibonacci hash.
pub(super) fn slot_of(key: u32) -> usize {
    (key.wrapping_mul(HASH_MUL) >> (32 - SLOT_BITS)) as usize
}

/// Whether to abandon the cache after the probe: only for images larger than the probe, and only
/// when fewer than 25% of the probed pixels hit (`probe_hits / PROBE_PIXELS < 1/4`, in integers).
pub(super) fn falls_back(total_pixels: usize, probe_hits: usize) -> bool {
    total_pixels > PROBE_PIXELS && probe_hits * 4 < PROBE_PIXELS
}

#[derive(Clone, Copy)]
struct Slot {
    key: u32,
    rgb: [u8; 3],
}

/// Direct-mapped cache from input bytes to output bytes. A colliding insert overwrites.
pub(super) struct ByteMemo {
    slots: Vec<Slot>,
}

impl ByteMemo {
    pub(super) fn new() -> Self {
        Self {
            slots: vec![
                Slot {
                    key: 0,
                    rgb: [0; 3]
                };
                SLOTS
            ],
        }
    }

    /// Output bytes stored for exactly `key`, if any (a different key in the slot is a miss).
    pub(super) fn get(&self, key: u32) -> Option<[u8; 3]> {
        let slot = self.slots[slot_of(key)];
        (slot.key == key).then_some(slot.rgb)
    }

    pub(super) fn insert(&mut self, key: u32, rgb: [u8; 3]) {
        self.slots[slot_of(key)] = Slot { key, rgb };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_below_a_quarter_hit_rate() {
        assert!(falls_back(PROBE_PIXELS + 1, PROBE_PIXELS / 4 - 1));
    }

    #[test]
    fn keeps_cache_at_exactly_a_quarter_hit_rate() {
        assert!(!falls_back(PROBE_PIXELS + 1, PROBE_PIXELS / 4));
    }

    #[test]
    fn never_falls_back_at_or_below_probe_size() {
        assert!(!falls_back(PROBE_PIXELS, 0));
        assert!(!falls_back(PROBE_PIXELS - 1, 0));
        assert!(!falls_back(1, 0));
    }

    #[test]
    fn slot_index_stays_in_range() {
        for k in [key(0, 0, 0), key(255, 255, 255), key(1, 2, 3), u32::MAX] {
            assert!(slot_of(k) < SLOTS);
        }
    }

    #[test]
    fn empty_cache_misses_black() {
        assert_eq!(ByteMemo::new().get(key(0, 0, 0)), None);
    }

    #[test]
    fn colliding_key_overwrites_and_misses_the_old_one() {
        let a = key(10, 20, 30);
        let b = (0..1u32 << 24)
            .map(|v| v | VALID)
            .find(|&k| k != a && slot_of(k) == slot_of(a))
            .unwrap();
        let mut memo = ByteMemo::new();
        memo.insert(a, [1, 2, 3]);
        assert_eq!(memo.get(b), None);
        memo.insert(b, [4, 5, 6]);
        assert_eq!((memo.get(a), memo.get(b)), (None, Some([4, 5, 6])));
    }
}

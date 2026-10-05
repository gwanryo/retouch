//! Color math inside wasm32 (`wasm-pack test --node engine/wasm`): Sharma 2005 vectors both ways.
#![cfg(target_arch = "wasm32")]
#![allow(clippy::unwrap_used)]

use engine_core::color::{delta_e00, sharma_2005_rows};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn sharma_vectors_match_within_1e_4_in_both_argument_orders() {
    let rows = sharma_2005_rows();
    assert_eq!(rows.len(), 34);
    for (i, (c1, c2, want)) in rows.into_iter().enumerate() {
        for got in [delta_e00(c1, c2), delta_e00(c2, c1)] {
            assert!(
                (got - want).abs() <= 1e-4,
                "pair {}: want {want} got {got}",
                i + 1
            );
        }
    }
}

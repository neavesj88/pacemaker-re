//! The label store lives in `pm_formats::ops_labels` (shared with `pm tags`); this keeps the
//! old path and the test that pins it to the TS mirror.

pub use pm_formats::ops_labels::*;

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The same vectors the TS mirror (`labels.ts`) is tested against.
    #[test]
    fn parity_vectors_match_the_ts_mirror() {
        #[derive(Deserialize)]
        struct Case {
            name: String,
            before: Labels,
            ops: Vec<Op>,
            after: Option<Labels>,
        }
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../../src/lib/state/labels.parity.json")).unwrap();
        assert!(cases.len() >= 10);
        for c in cases {
            match (apply_ops(&c.before, &c.ops), c.after) {
                (Ok(got), Some(want)) => assert_eq!(got, want, "{}", c.name),
                (Err(_), None) => {}
                (got, want) => panic!("{}: got {got:?}, wanted {want:?}", c.name),
            }
        }
    }
}

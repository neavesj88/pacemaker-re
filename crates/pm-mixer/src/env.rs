//! Envelope evaluation: piecewise-linear curves over sample positions, flat outside the nodes.

use pm_formats::mix::Node;

#[derive(Debug, Clone, Default)]
pub struct Env {
    pos: Vec<f64>,
    val: Vec<f32>,
}

impl Env {
    /// Builds a curve from nodes whose positions are offset by `shift` (the owning event's
    /// start when the nodes are event-relative). Nodes are sorted; non-finite values dropped.
    pub fn new(nodes: &[Node], shift: f64) -> Env {
        let mut v: Vec<(f64, f32)> =
            nodes.iter().filter(|n| n.value.is_finite()).map(|n| (f64::from(n.pos) + shift, n.value)).collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        Env { pos: v.iter().map(|n| n.0).collect(), val: v.iter().map(|n| n.1).collect() }
    }

    pub fn from_points(points: &[(f64, f32)]) -> Env {
        let mut v = points.to_vec();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        Env { pos: v.iter().map(|n| n.0).collect(), val: v.iter().map(|n| n.1).collect() }
    }

    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }

    /// Value at `at`; `None` for an empty curve.
    pub fn eval(&self, at: f64) -> Option<f32> {
        let n = self.pos.len();
        if n == 0 {
            return None;
        }
        if at <= self.pos[0] {
            return Some(self.val[0]);
        }
        let i = self.pos.partition_point(|&p| p <= at) - 1;
        if i == n - 1 {
            return Some(self.val[i]);
        }
        let span = self.pos[i + 1] - self.pos[i];
        if span <= 0.0 {
            return Some(self.val[i + 1]);
        }
        let t = ((at - self.pos[i]) / span) as f32;
        Some(self.val[i] + (self.val[i + 1] - self.val[i]) * t)
    }

    /// Value at `at`, or `default` for an empty curve.
    pub fn at(&self, at: f64, default: f32) -> f32 {
        self.eval(at).unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_and_clamps() {
        let e = Env::new(&[Node { pos: 100, value: 0.0 }, Node { pos: 200, value: 10.0 }], 0.0);
        assert_eq!(e.eval(0.0), Some(0.0));
        assert_eq!(e.eval(150.0), Some(5.0));
        assert_eq!(e.eval(900.0), Some(10.0));
        assert_eq!(Env::default().eval(1.0), None);
        assert_eq!(Env::default().at(1.0, 7.0), 7.0);
    }
}

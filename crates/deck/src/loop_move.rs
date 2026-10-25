#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopEdge {
    Both,
    In,
    Out,
}

pub(crate) fn moved(
    edge: LoopEdge,
    step: Option<f64>,
    steps: i32,
    start: f64,
    end: f64,
) -> (f64, f64) {
    let n = f64::from(steps);
    let delta = match (step, edge) {
        (Some(step), _) => step * n,
        (None, LoopEdge::Both) => (end - start) * n,
        (None, _) => {
            let len = end - start;
            let grow = if steps >= 0 { len * n } else { len / (2.0 * n) };
            if edge == LoopEdge::In { -grow } else { grow }
        }
    };
    match edge {
        LoopEdge::Both => (start + delta, end + delta),
        LoopEdge::In => (start + delta, end),
        LoopEdge::Out => (start, end + delta),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_moves_the_chosen_ends() {
        assert_eq!(moved(LoopEdge::Both, Some(1.0), 2, 4.0, 8.0), (6.0, 10.0));
        assert_eq!(moved(LoopEdge::In, Some(0.5), -1, 4.0, 8.0), (3.5, 8.0));
        assert_eq!(moved(LoopEdge::Out, Some(0.5), 1, 4.0, 8.0), (4.0, 8.5));
    }

    #[test]
    fn the_loop_size_moves_the_loop_or_doubles_and_halves_it() {
        assert_eq!(moved(LoopEdge::Both, None, -1, 4.0, 8.0), (0.0, 4.0));
        assert_eq!(moved(LoopEdge::In, None, 1, 4.0, 8.0), (0.0, 8.0));
        assert_eq!(moved(LoopEdge::In, None, -1, 4.0, 8.0), (6.0, 8.0));
        assert_eq!(moved(LoopEdge::Out, None, 1, 4.0, 8.0), (4.0, 12.0));
        assert_eq!(moved(LoopEdge::Out, None, -1, 4.0, 8.0), (4.0, 6.0));
        assert_eq!(moved(LoopEdge::Out, None, -2, 4.0, 8.0), (4.0, 7.0));
    }
}

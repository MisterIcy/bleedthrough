//! Strongly connected components with an explicit stack: call chains can be far deeper than the
//! thread stack allows.

/// Term: Call graph
///
/// The strongly connected components of the graph whose node `n` has the successors `edges[n]`,
/// callees first: a component appears after every component it reaches. Nodes inside a component
/// are in ascending order, and components are discovered from the lowest node outwards over sorted
/// successors, so the result depends only on the graph.
pub fn strongly_connected(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    const UNSEEN: usize = usize::MAX;
    let count = edges.len();
    let mut index = vec![UNSEEN; count];
    let mut low = vec![0usize; count];
    let mut on_stack = vec![false; count];
    let mut stack: Vec<usize> = Vec::new();
    let mut counter = 0usize;
    let mut components: Vec<Vec<usize>> = Vec::new();
    // (node, next successor position)
    let mut frames: Vec<(usize, usize)> = Vec::new();
    for root in 0..count {
        if index[root] != UNSEEN {
            continue;
        }
        frames.push((root, 0));
        while let Some(&(node, position)) = frames.last() {
            if position == 0 && index[node] == UNSEEN {
                index[node] = counter;
                low[node] = counter;
                counter += 1;
                stack.push(node);
                on_stack[node] = true;
            }
            if let Some(&next) = edges[node].get(position) {
                if let Some(frame) = frames.last_mut() {
                    frame.1 += 1;
                }
                if index[next] == UNSEEN {
                    frames.push((next, 0));
                } else if on_stack[next] {
                    low[node] = low[node].min(index[next]);
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == index[node] {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                component.sort_unstable();
                components.push(component);
            }
        }
    }
    components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_lists_callees_first() {
        // 0 -> 1 -> 2
        let components = strongly_connected(&[vec![1], vec![2], vec![]]);
        assert_eq!(components, vec![vec![2], vec![1], vec![0]]);
    }

    #[test]
    fn mutual_and_self_recursion_form_components() {
        // 0 <-> 1, 1 -> 2, 2 -> 2
        let components = strongly_connected(&[vec![1], vec![0, 2], vec![2]]);
        assert_eq!(components, vec![vec![2], vec![0, 1]]);
    }
}

#[cfg(test)]
mod depth_tests {
    use super::strongly_connected;

    /// spec: call-graph/Recursion is grouped
    #[test]
    fn a_hundred_thousand_node_chain_does_not_overflow_the_stack() {
        let n = 100_000;
        let edges: Vec<Vec<usize>> = (0..n)
            .map(|i| if i + 1 < n { vec![i + 1] } else { vec![] })
            .collect();
        let components = strongly_connected(&edges);
        assert_eq!(components.len(), n);
        assert_eq!(
            components.first(),
            Some(&vec![n - 1]),
            "the deepest callee first"
        );
        assert_eq!(components.last(), Some(&vec![0]));
    }

    /// spec: call-graph/Recursion is grouped
    #[test]
    fn a_long_cycle_is_one_component() {
        let n = 50_000;
        let edges: Vec<Vec<usize>> = (0..n).map(|i| vec![(i + 1) % n]).collect();
        let components = strongly_connected(&edges);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].len(), n);
    }
}

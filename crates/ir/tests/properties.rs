//! Properties of the graph helpers over arbitrary shapes.

use bleedthrough_ir::{Block, BlockId, Graph, Terminator, UnitKey, UnitKind};
use bleedthrough_syntax::Span;
use bleedthrough_testkit::check;
use bleedthrough_testkit::proptest::prelude::*;

fn graph_from(edges: &[(u32, u32)], count: u32) -> Graph {
    let blocks = (0..count)
        .map(|i| {
            let targets: Vec<BlockId> = edges
                .iter()
                .filter(|(from, _)| *from % count == i)
                .map(|(_, to)| BlockId::new(*to % count))
                .collect();
            let term = match targets.as_slice() {
                [] => Terminator::Exit,
                [one] => Terminator::Goto(*one),
                _ => Terminator::Choice(targets.clone()),
            };
            Block {
                instrs: vec![],
                term,
                handlers: vec![],
            }
        })
        .collect();
    Graph {
        key: UnitKey {
            span: Span::new(0, 0),
            name: "g".into(),
        },
        kind: UnitKind::Function,
        owner: None,
        is_static: true,
        params: vec![],
        captures: vec![],
        locals: vec![],
        blocks,
        entry: BlockId::new(0),
        exit: BlockId::new(count - 1),
        value_count: 0,
        recovered: false,
        unknowns: Default::default(),
        limits: Default::default(),
    }
}

/// spec: ir-lowering/Lowering is deterministic and independent of other files
#[test]
fn reverse_post_order_lists_each_reachable_block_once_and_starts_at_entry() {
    let shapes = (1u32..20, prop::collection::vec((0u32..40, 0u32..40), 0..60));
    check(shapes, |(count, edges)| {
        let g = graph_from(&edges, count);
        let order = g.reverse_post_order();
        prop_assert_eq!(order.first().copied(), Some(g.entry));
        let mut sorted = order.clone();
        sorted.sort();
        sorted.dedup();
        prop_assert_eq!(sorted.len(), order.len());
        for b in &order {
            for s in g.all_successors(*b) {
                prop_assert!(order.contains(&s));
            }
        }
        prop_assert_eq!(order, g.reverse_post_order());
        Ok(())
    });
}

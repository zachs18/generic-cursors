use generic_cursors::with_data::{MoveDecision, MutRefStackWithData};

pub struct Forest<T> {
    roots: Vec<ForestNode<T>>,
}

struct ForestNode<T> {
    data: T,
    children: Vec<ForestNode<T>>,
}

pub fn preorder_traverse<T, F: FnMut(&mut T, usize)>(tree: &mut Forest<T>, mut callback: F) {
    struct TraversalState {
        next_index: usize,
        depth: usize,
    }
    let mut cursor = MutRefStackWithData::new(
        &mut *tree.roots,
        TraversalState {
            next_index: 0,
            depth: 0,
        },
    );
    loop {
        let Ok(_) = cursor.move_with(|element, state| {
            if state.next_index >= element.len() {
                MoveDecision::Ascend
            } else {
                callback(&mut element[state.next_index].data, state.depth);
                let decision = MoveDecision::Descend(
                    &mut *element[state.next_index].children,
                    TraversalState {
                        next_index: 0,
                        depth: state.depth + 1,
                    },
                );
                state.next_index += 1;
                decision
            }
        }) else {
            break;
        };
    }
}

fn main() {
    let mut forest = Forest {
        roots: vec![
            ForestNode {
                data: 0u32,
                children: vec![
                    ForestNode {
                        data: 1u32,
                        children: vec![],
                    },
                    ForestNode {
                        data: 2u32,
                        children: vec![],
                    },
                    ForestNode {
                        data: 3u32,
                        children: vec![],
                    },
                ],
            },
            ForestNode {
                data: 4u32,
                children: vec![],
            },
            ForestNode {
                data: 5u32,
                children: vec![],
            },
            ForestNode {
                data: 6u32,
                children: vec![ForestNode {
                    data: 7u32,
                    children: vec![ForestNode {
                        data: 8u32,
                        children: vec![ForestNode {
                            data: 9u32,
                            children: vec![],
                        }],
                    }],
                }],
            },
        ],
    };
    preorder_traverse(&mut forest, |t, depth| {
        println!("{:depth$}{t}", "");
        *t *= *t;
    });
    println!();
    preorder_traverse(&mut forest, |t, depth| {
        println!("{:depth$}{t}", "");
    });
}

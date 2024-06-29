use crate::distr::{Distr, Sampler};
use crate::tree::{Node, Tree, TreeError};
use std::collections::VecDeque;

use clap::ValueEnum;
use rand::prelude::*;


/// Shape of random trees to generate
#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum TreeShape {
    /// Yule model tree shape
    Yule,
    /// Caterpillar tree shape
    Caterpillar,
    /// Ete3 Tree.populate replicate
    Ete3,
}

/// Genereates a random binary tree of a given size.
pub fn generate_tree(
    n_leaves: usize,
    brlens: bool,
    sampler_type: Distr,
) -> Result<Tree, TreeError> {
    let mut tree = Tree::new();
    // Add root
    tree.add(Node::default());

    let mut rng = thread_rng();

    let sampler = Sampler::new(sampler_type);

    let mut next_deq = VecDeque::new();
    next_deq.push_back(0);

    for _ in 0..(n_leaves - 1) {
        let parent_id = if rng.gen_bool(0.5) {
            next_deq.pop_front()
        } else {
            next_deq.pop_back()
        }
        .unwrap();
        let l1: Option<f64> = if brlens {
            Some(sampler.sample(&mut rng))
        } else {
            None
        };
        let l2: Option<f64> = if brlens {
            Some(sampler.sample(&mut rng))
        } else {
            None
        };
        next_deq.push_back(tree.add_child(Node::new(), parent_id, l1)?);
        next_deq.push_back(tree.add_child(Node::new(), parent_id, l2)?);
    }

    for (i, id) in next_deq.iter().enumerate() {
        tree.get_mut(id)?.set_name(format!("Tip_{i}"));
    }

    Ok(tree)
}

/// Generate a random binary tree under the Yule model.
pub fn generate_yule(
    n_leaves: usize,
    brlens: bool,
    sampler_type: Distr,
) -> Result<Tree, TreeError> {
    // Initialize tree
    let mut tree = Tree::new();
    let root = tree.add(Node::default());

    let mut rng = thread_rng();
    let sampler = Sampler::new(sampler_type);

    let mut parent_candidates = vec![root];

    while tree.n_leaves() != n_leaves {
        // Choose parent
        let parent = *parent_candidates
            .choose(&mut rng)
            .expect("No parent candidate");
        // .clone();

        // Generate child node
        let edge1: Option<f64> = brlens.then_some(sampler.sample(&mut rng));
        let edge2: Option<f64> = brlens.then_some(sampler.sample(&mut rng));
        let child1 = tree.add_child(Node::default(), parent, edge1)?;
        let child2 = tree.add_child(Node::default(), parent, edge2)?;
        parent_candidates.push(child1);
        parent_candidates.push(child2);

        let pos = parent_candidates.iter().position(|n| *n == parent).unwrap();
        parent_candidates.swap_remove(pos);
    }

    // Assign names to tips
    for (i, tip_idx) in tree.get_leaves().iter().cloned().enumerate() {
        tree.get_mut(&tip_idx)?.set_name(format!("Tip_{i}"));
    }

    Ok(tree)
}

/// Generates a caterpillar tree by adding children to the last node addesd to the tree
/// until we reach the desired numebr of leaves.
pub fn generate_caterpillar(
    n_leaves: usize,
    brlens: bool,
    sampler_type: Distr,
) -> Result<Tree, TreeError> {
    let mut tree = Tree::new();
    tree.add(Node::default());

    let mut rng = thread_rng();
    let sampler = Sampler::new(sampler_type);

    let mut parent = 0;
    for i in 1..n_leaves {
        let parent_bkp = parent;

        let l1: Option<f64> = brlens.then_some(sampler.sample(&mut rng));
        let l2: Option<f64> = brlens.then_some(sampler.sample(&mut rng));

        if i == n_leaves - 1 {
            // Adding tip
            tree.add_child(Node::new_named(&format!("Tip_{i}")), parent, l1)?;
            tree.add_child(Node::new_named(&format!("Tip_{}", i + 1)), parent, l2)?;
        } else {
            // Adding parent node
            parent = tree.add_child(Node::new(), parent, l1)?;
            tree.add_child(Node::new_named(&format!("Tip_{i}")), parent_bkp, l2)?;
        }
    }

    Ok(tree)
}

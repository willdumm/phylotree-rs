#![warn(missing_docs)]

//! The `phylotree` crate aims to be useful when dealing with phylogenetic trees.
//! It can be used to build such trees or read then from newick files. this crate
//! can also be used to compare trees.  
//!
//! Since phylogenetic trees and phylolgenetic distance matrices are so closely related
//! this crate can also be used to extract such matrices from phylolgenetic trees as
//! well as read and write phylip distance matrix files.  
//!
//! # A note on implementation
//! Recursive data structures can be a pain in rust, which is why this crate exists:  
//!   
//! **so you don't have to implement it...**  
//!   
//! To avoid this problem here the tree is stored as a vector
//! of nodes, each node has an identifier and accessing and mutating
//! nodes in a tree is done using these identifiers. As such we can have a
//! non-recursive data structure representing the tree but also easily
//! implement recursive algorithms *(like simple tree traversals)* on this tree.
//!
//! # Using `phylotree`
//! Most of the functionality is implemented in [`crate::tree`]. The
//! [`crate::distance`] module is used to dealt with phylolgenetic distance matrices.
//! [`crate::distr`] is a helper module to provide different branch
//! length distributions when generating random phylogenetic trees.
//!
//! ## Building trees
//! The simplest way to build a tree is to create an empty tree, add a root node and
//! then add children to the various added nodes:
//!
//! ```
//! use phylotree::tree::{Tree, Node};
//!
//! let mut tree = Tree::new();
//!
//! // Add the root node
//! let root = tree.add(Node::new());
//!
//! // Add a child to the root
//! let child1 = tree.add_child(Node::new_named("Child_1"), root, None).unwrap();
//! // Add a child to the root with a branch length
//! let child2 = tree.add_child(Node::new_named("Child_2"), root, Some(0.5)).unwrap();
//!
//! // Add more children
//! let child3 = tree.add_child(Node::new_named("Child_3"), child1, None).unwrap();
//!
//! // Get depth of child
//! assert_eq!(tree.get(&child3).unwrap().get_depth(), 2)
//! ```
//!
//! ## Reading and writing trees
//! This library can build trees strings (or files) encoded in the
//! [newick](https://en.wikipedia.org/wiki/Newick_format) format:
//! ```
//! use phylotree::tree::Tree;
//!
//! let newick_str = "((A:0.1,B:0.2)F:0.6,(C:0.3,D:0.4)E:0.5)G;";
//! let tree = Tree::from_newick(newick_str).unwrap();
//!
//! assert_eq!(tree.to_newick().unwrap(), newick_str)
//! ```
//!
//! ## Traversing trees
//! Several traversals are implemented to visit nodes in a particular order. pre-order,
//! post-order and level-order traversals are implemented on all trees. In-order traversls
//! are implemented only for binary trees. A traversals returns a [`Vec`] of [`tree::NodeId`]
//! in the order they are to be visited in.
//! ```
//! use phylotree::tree::{Tree, Node};
//!
//! //          |
//! //     -----G-----
//! //    |          |
//! // ---C---    ---F---
//! // |     |    |     |
//! // A     B    D     E
//!
//! let newick_str = "((A,B)C,(D,E)F)G;";
//! let mut tree = Tree::from_newick(newick_str).unwrap();
//! let root = tree.get_root().unwrap();
//!
//! let preorder: Vec<_> = tree.preorder(&root).unwrap()
//!     .iter()
//!     .map(|node_id| tree.get(node_id).unwrap().name.clone().unwrap())
//!     .collect();
//!
//! assert_eq!(preorder, vec!["G", "C", "A", "B", "F", "D", "E"]);
//!
//! // Add a child node to F so the tree is no longer binary
//! let f_idx = tree.get_by_name("F").unwrap().id;
//! tree.add_child(Node::new_named("third_child"), f_idx, None).unwrap();
//!
//! assert!(tree.inorder(&root).is_err())
//! ```
//!
//!
//! ## Comparing trees
//! A number of metrics taking into account topology and branch lenghts are implemented
//! in order to compare trees with each other:
//! ```
//! use phylotree::tree::Tree;
//!
//! // The second tree is just a random rotation of the first,
//! // they represent the same phylogeney
//! let newick_orig = "((A:0.1,B:0.2)F:0.6,(C:0.3,D:0.4)E:0.5)G;";
//! let newick_rota = "((D:0.3,C:0.4)E:0.5,(B:0.2,A:0.1)F:0.6)G;";
//!
//! let tree_orig = Tree::from_newick(newick_orig).unwrap();
//! let tree_rota = Tree::from_newick(newick_rota).unwrap();
//!
//! let rf = tree_orig.robinson_foulds(&tree_rota).unwrap();
//!
//! assert_eq!(rf, 0)
//! ```
//!
//! ## Computing distances between nodes in a tree
//! We can get the distance (either as number of edges or sum of edge lengths) betweem
//! nodes of the tree as well as compute the whole phyhlogenetic distance matrix
//! of a tree.
//! ```
//! use phylotree::tree::Tree;
//!
//! // The following tree is encoded by the newick string:
//! //          |
//! //     +----+----+
//! //     |         |
//! //    0.3        |
//! //     |         |
//! //     |        0.6
//! //   --+--       |
//! //   |   |       |
//! //  0.2 0.2      |
//! //   |   |    ---+---
//! //   T3  T1   |     |
//! //            |     |
//! //           0.4   0.5
//! //            |     |
//! //            |     |
//! //            T2    |
//! //                  T0
//!
//! let newick = "((T3:0.2,T1:0.2):0.3,(T2:0.4,T0:0.5):0.6);";
//! let tree = Tree::from_newick(newick).unwrap();
//!
//! let t0 = tree.get_by_name("T0").unwrap();
//! let t3 = tree.get_by_name("T3").unwrap();
//!
//! let (edge_sum, num_edges) = tree.get_distance(&t0.id, &t3.id).unwrap();
//!
//! assert_eq!(num_edges, 4);
//! assert_eq!(edge_sum, Some(0.5 + 0.6 + 0.3 + 0.2));
//!
//! // Compute the whole distance matrix
//! let matrix = tree.distance_matrix_recursive().unwrap();
//! let phylip="\
//! 4
//! T0    0  1.6  0.9  1.6
//! T1    1.6  0  1.5  0.4
//! T2    0.9  1.5  0  1.5
//! T3    1.6  0.4  1.5  0
//! ";
//!
//! assert_eq!(matrix.to_phylip(true).unwrap(), phylip)
//! ```
//!
#[cfg(feature = "python")]
pub mod python;

pub mod distance;
pub mod distr;
pub mod tree;
pub mod tree_generation;
pub use tree_generation::*;


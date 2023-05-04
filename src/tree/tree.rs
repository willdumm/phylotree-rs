use fixedbitset::FixedBitSet;
use itertools::Itertools;
use std::iter::zip;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use thiserror::Error;

use super::node::Node;
use super::{Edge, NodeId};

#[derive(Error, Debug)]
pub enum TreeError {
    #[error("This tree is not Binary.")]
    IsNotBinary,
    #[error("This tree is not rooted.")]
    IsNotRooted,
    #[error("This tree is empty.")]
    IsEmpty,
    #[error("All your leaf nodes must ne named.")]
    UnnamedLeaves,
    #[error("Your leaf names must be unique.")]
    DuplicateLeafNames,
    #[error("The leaf index of the tree is not initialized.")]
    LeafIndexNotInitialized,
    #[error("The tree must have all branch lengths.")]
    MissingBranchLengths,
    #[error("The trees have different tips indices.")]
    DifferentTipIndices,
    #[error("There is no node with index: {0}")]
    NodeNotFound(NodeId),
    #[error("No root node found")]
    RootNotFound,
    #[error("Error writing tree to file")]
    IoError(#[from] std::io::Error),
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Cannot have whitespace in number field.")]
    WhiteSpaceInNumber,
    #[error("Missing a closing bracket.")]
    UnclosedBracket,
    #[error("The tree is missin a semi colon at the end.")]
    NoClosingSemicolon,
    #[error("Problem with building the tree.")]
    TreeError(#[from] TreeError),
    #[error("Could not parse a branch length")]
    FloatError(#[from] std::num::ParseFloatError),
    #[error("Parent node of subtree not found")]
    NoSubtreeParent,
    #[error("Problem reading file")]
    IoError(#[from] std::io::Error),
}

/// A Vector backed Tree structure
#[derive(Debug, Clone)]
pub struct Tree {
    nodes: Vec<Node>,
    leaf_index: RefCell<Option<Vec<String>>>,
    partitions: RefCell<Option<HashMap<FixedBitSet, Option<Edge>>>>,
}

impl Tree {
    /// Create a new empty Tree object
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            leaf_index: RefCell::new(None),
            partitions: RefCell::new(None),
        }
    }

    // ############################
    // # adding and getting nodes #
    // ############################

    /// Add a new node to the tree.
    pub fn add(&mut self, node: Node) -> NodeId {
        let idx = self.nodes.len();
        let mut node = node;
        node.id = idx;
        self.nodes.push(node);

        idx
    }

    /// Add a child to one of the tree's nodes.  
    ///
    /// # Example
    /// ```
    /// use phylotree::tree::{Tree,Node};
    ///
    /// // Create the tree and add a root node
    /// let mut tree = Tree::new();
    /// let root_id = tree.add(Node::new());
    ///
    /// // Add children to the root
    /// let left = tree.add_child(Node::new(), root_id, None).unwrap();
    /// let right = tree.add_child(Node::new(), root_id, Some(0.1)).unwrap();
    ///
    /// assert_eq!(tree.get(&root_id).children.len(), 2);
    ///
    /// // The depths of child nodes are derived from the parent node
    /// assert_eq!(tree.get(&left).depth, 1);
    /// assert_eq!(tree.get(&right).depth, 1);
    ///
    /// // If an edge length is specified then it is set in both child and parent
    /// assert_eq!(tree.get(&right).parent_edge, Some(0.1));
    /// assert_eq!(tree.get(&root_id).get_child_edge(&right), Some(0.1));
    /// ```
    pub fn add_child(
        &mut self,
        node: Node,
        parent: NodeId,
        edge: Option<Edge>,
    ) -> Result<NodeId, TreeError> {
        if parent >= self.nodes.len() {
            return Err(TreeError::NodeNotFound(parent));
        }

        let mut node = node;

        node.set_parent(parent, edge);
        node.set_depth(self.get(&parent).depth + 1);

        let id = self.add(node);

        self.get_mut(&id).set_id(id);
        self.get_mut(&parent).add_child(id, edge);

        Ok(id)
    }

    /// Get a reference to a specific Node of the tree
    pub fn get(&self, id: &NodeId) -> &Node {
        &self.nodes[*id]
    }

    /// Get a mutable reference to a specific Node of the tree
    pub fn get_mut(&mut self, id: &NodeId) -> &mut Node {
        &mut self.nodes[*id]
    }

    /// Get a reference to a node in the tree by name
    /// ```
    /// use phylotree::tree::{Tree, Node};
    ///
    /// let mut tree = Tree::new();
    /// let root_idx = tree.add(Node::new_named("root"));
    /// let child_idx = tree.add_child(Node::new_named("child"), root_idx, None).unwrap();
    ///
    /// assert_eq!(tree.get_by_name("child"), Some(tree.get(&child_idx)));
    /// ```
    pub fn get_by_name(&self, name: &str) -> Option<&Node> {
        self.nodes
            .iter()
            .find(|node| node.name.is_some() && node.name == Some(String::from(name)))
    }

    /// Get a mutable reference to a node in the tree by name
    pub fn get_by_name_mut(&mut self, name: &str) -> Option<&mut Node> {
        self.nodes
            .iter_mut()
            .find(|node| node.name.is_some() && node.name == Some(String::from(name)))
    }

    /// Gets the root node. In the case of unrooted trees this node is a "virtual root"
    /// that has exactly 3 children.
    pub fn get_root(&self) -> Result<NodeId, TreeError> {
        self.nodes
            .iter()
            .filter(|&node| node.parent.is_none())
            .map(|node| node.id)
            .next()
            .ok_or(TreeError::RootNotFound)
    }

    /// Returns a [`Vec`] containing the Node IDs of leaf nodes of the tree
    /// ```
    /// use phylotree::tree::{Tree, Node};
    ///
    /// let mut tree = Tree::new();
    /// let root_idx = tree.add(Node::new());
    /// let left = tree.add_child(Node::new(), root_idx, None).unwrap();
    /// let right = tree.add_child(Node::new(), root_idx, None).unwrap();
    ///
    /// assert_eq!(tree.get_leaves(), vec![left, right]);
    /// ```
    pub fn get_leaves(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|&node| node.is_tip())
            .map(|node| node.id)
            .collect()
    }

    /// Gets the leaf indices in the subtree rooted at node of a specified index
    /// ```
    /// use phylotree::tree::Tree;
    ///
    /// let tree = Tree::from_newick("(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;").unwrap();
    /// let sub_root = tree.get_by_name("E").unwrap();
    /// let sub_leaves: Vec<_> = tree.get_subtree_leaves(&sub_root.id)
    ///     .iter()
    ///     .map(|id| tree.get(id).name.clone().unwrap())
    ///     .collect();
    ///
    /// assert_eq!(
    ///     sub_leaves,
    ///     vec![String::from("C"), String::from("D")]
    /// )
    /// ```
    pub fn get_subtree_leaves(&self, index: &NodeId) -> Vec<NodeId> {
        let mut indices = vec![];
        if self.get(index).is_tip() {
            return vec![*index];
        }

        for &child_idx in self.get(index).children.iter() {
            indices.extend(self.get_subtree_leaves(&child_idx))
        }

        indices
    }

    // #######################################
    // # getting characteristics of the tree #
    // #######################################

    /// Check if the tree is Binary
    pub fn is_binary(&self) -> bool {
        for node in self.nodes.iter() {
            // The "root" node of an unrooted binary tree has 3 children
            if node.parent.is_none() && node.children.len() > 3 {
                return false;
            }
            if node.children.len() > 2 {
                return false;
            }
        }
        true
    }

    /// Checks if the tree is rooted (i.e. the root node exists and has exactly 2 children)
    pub fn is_rooted(&self) -> Result<bool, TreeError> {
        let root_id = self.get_root()?;

        Ok(!self.nodes.is_empty() && self.get(&root_id).children.len() == 2)
    }

    /// Returns the number of nodes in the tree
    pub fn size(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of leaves in the tree
    pub fn n_leaves(&self) -> usize {
        self.nodes.iter().filter(|&node| node.is_tip()).count()
    }

    /// Returns the height of the tree
    /// (i.e. the number of edges from the root to the deepest tip)
    /// ```
    /// use phylotree::tree::Tree;
    ///
    /// let tree = Tree::from_newick("(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;").unwrap();
    /// assert_eq!(tree.height(), Some(0.9));
    ///
    /// let tree_no_brlen = Tree::from_newick("(A,B,(C,D)E)F;").unwrap();
    /// assert_eq!(tree_no_brlen.height(), Some(2.));
    /// ```
    pub fn height(&self) -> Option<Edge> {
        let root = self.get_root().unwrap();

        self.get_leaves()
            .iter()
            .map(|leaf| {
                let (edge_sum, num_edges) = self.get_distance(&root, leaf);
                match edge_sum {
                    Some(height) => height,
                    None => num_edges as f64,
                }
            })
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Returns the diameter of the tree
    /// (i.e. longest tip to tip distance)
    /// ```
    /// use phylotree::tree::Tree;
    ///
    /// let tree = Tree::from_newick("(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;").unwrap();
    /// assert_eq!(tree.diameter(), Some(1.1));
    ///
    /// let tree_no_brlen = Tree::from_newick("(A,B,(C,D)E)F;").unwrap();
    /// assert_eq!(tree_no_brlen.diameter(), Some(3.));
    /// ```
    pub fn diameter(&self) -> Option<f64> {
        self.get_leaves()
            .iter()
            .combinations(2)
            .map(|pair| {
                let (edge_sum, num_edges) = self.get_distance(pair[0], pair[1]);
                match edge_sum {
                    Some(height) => height,
                    None => num_edges as f64,
                }
            })
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Checks if the tree is rooted and binary
    fn check_rooted_binary(&self) -> Result<(), TreeError> {
        if !self.is_rooted()? {
            Err(TreeError::IsNotRooted)
        } else if !self.is_binary() {
            Err(TreeError::IsNotBinary)
        } else {
            Ok(())
        }
    }

    /// Computes the number of cherries in a tree
    pub fn cherries(&self) -> Result<usize, TreeError> {
        self.check_rooted_binary()?;
        if !self.nodes.is_empty() {
            let mut n = 0;
            for node in self.nodes.iter() {
                if node.children.len() == 2
                    && self.get(&node.children[0]).is_tip()
                    && self.get(&node.children[1]).is_tip()
                {
                    n += 1;
                }
            }
            Ok(n)
        } else {
            Err(TreeError::IsEmpty)
        }
    }

    /// Computes the Colless index for the tree.
    /// The colless index, $I_c$, measures the imbalance of a phylogenetic tree:  
    /// $$
    /// I_c = \sum_{i \in nodes} |L_i - R_i|
    /// $$
    ///
    /// Where $L_i$ is the number of leaves in the left subtree of node $i$ and
    /// $R_i$ the number of leaves in the right subtree of $i$.
    ///
    pub fn colless(&self) -> Result<usize, TreeError> {
        self.check_rooted_binary()?;

        let colless = self
            .nodes
            .iter()
            .filter(|node| !node.is_tip())
            .map(|node| {
                if node.children.is_empty() {
                    return 0;
                }
                let left = self.get_subtree_leaves(&node.children[0]).len();
                let right = if node.children.len() > 1 {
                    self.get_subtree_leaves(&node.children[1]).len()
                } else {
                    0
                };

                left.abs_diff(right)
            })
            .sum();

        Ok(colless)
    }

    /// Computes the normalized colless statistic with a Yule null model:  
    /// $$
    /// I_{yule} = \frac{I_c - n\cdot\log(n) - n(\gamma-1-\log(2))}{n}
    /// $$
    /// Where $I_c$ is the unnormalized colless index *(computed with [`Tree::colless()`])*,
    /// $n$ the number of leaves
    /// and $\gamma$ the [Euler constant](https://en.wikipedia.org/wiki/Euler%27s_constant).  
    /// *([see also apTreeshape](https://search.r-project.org/CRAN/refmans/apTreeshape/html/colless.html))*
    pub fn colless_yule(&self) -> Result<f64, TreeError> {
        self.colless().map(|i_c| {
            let n = self.n_leaves() as f64;
            let e_i_c = n * n.ln() + (0.57721566 - 1. - f64::ln(2.0)) * n;

            (i_c as f64 - e_i_c) / n
        })
    }

    /// Computes the normalized colless statistic with a PDA null model:  
    /// $$
    /// I_{PDA} = \frac{I_c}{n^{3/2}}
    /// $$
    /// Where $I_c$ is the unnormalized colless index *(computed with [`Tree::colless()`])*
    /// and $n$ the number of leaves.  
    /// *([see also apTreeshape](https://search.r-project.org/CRAN/refmans/apTreeshape/html/colless.html))*
    pub fn colless_pda(&self) -> Result<f64, TreeError> {
        self.colless()
            .map(|i_c| i_c as f64 / f64::powf(self.n_leaves() as f64, 3.0 / 2.0))
    }

    /// Computes the Sackin index. The Sackin index, $I_s$, is computed by taking the
    /// sum over all internal nodes of the number of leaves descending from that node.
    /// A smaller Sackin index means a more balanced tree.
    pub fn sackin(&self) -> Result<usize, TreeError> {
        self.check_rooted_binary()?;

        Ok(self
            .get_leaves()
            .iter()
            .map(|tip_idx| self.get(tip_idx).depth)
            .sum())
    }

    /// Computes the normalized Sackin index with a Yule null model:
    /// $$
    /// I_{yule} = \frac{I_s - 2n\cdot \sum_{j=2}^n \frac{1}{j}}{n}
    /// $$
    /// With $I_s$ the unnormalized Sackin index *(computed with [`Tree::sackin()`])*
    /// and $n$ the number of leaves in the tree.  
    /// *([see also apTreeshape](https://search.r-project.org/CRAN/refmans/apTreeshape/html/sackin.html))*
    pub fn sackin_yule(&self) -> Result<f64, TreeError> {
        self.sackin().map(|i_n| {
            let n = self.n_leaves();
            let sum: f64 = (2..=n).map(|i| 1.0 / (i as f64)).sum();

            (i_n as f64 - 2.0 * (n as f64) * sum) / n as f64
        })
    }

    /// Computes the normalized sackin statistic with a PDA null model:
    /// $$
    /// I_{PDA} = \frac{I_s}{n^{3/2}}
    /// $$
    /// With $I_s$ the unnormalized Sackin index *(computed with [`Tree::sackin()`])*
    /// and $n$ the number of leaves in the tree.  
    /// *([see also apTreeshape](https://search.r-project.org/CRAN/refmans/apTreeshape/html/sackin.html))*
    pub fn sackin_pda(&self) -> Result<f64, TreeError> {
        self.sackin()
            .map(|i_n| i_n as f64 / f64::powf(self.n_leaves() as f64, 3.0 / 2.0))
    }

    // ##########################
    // # Find paths in the tree #
    // ##########################

    /// Returns the path from the node to the root
    /// ```
    /// use phylotree::tree::Tree;
    ///
    /// let tree = Tree::from_newick("((A,(C,E)D)B,((H)I)G)F;").unwrap();
    /// let path: Vec<_> = tree.get_path_from_root(&5)
    ///     .iter()
    ///     .map(|id| tree.get(id).name.clone().unwrap())
    ///     .collect();
    ///
    /// assert_eq!(
    ///     path,
    ///     vec![String::from("F"), String::from("B"), String::from("D"), String::from("E")]
    /// )
    /// ```
    pub fn get_path_from_root(&self, node: &NodeId) -> Vec<NodeId> {
        let mut path = vec![];
        let mut current_node = *node;
        loop {
            path.push(current_node);
            match self.get(&current_node).parent {
                Some(parent) => current_node = parent,
                None => break,
            }
        }

        path.into_iter().rev().collect()
    }

    /// Gets the most recent common ancestor between two tree nodes
    /// ```
    /// use phylotree::tree::Tree;
    ///
    /// let tree = Tree::from_newick("((A,(C,E)D)B,((H)I)G)F;").unwrap();
    /// let ancestor = tree.get_common_ancestor(
    ///     &tree.get_by_name("A").unwrap().id,
    ///     &tree.get_by_name("D").unwrap().id,
    /// );
    ///
    /// assert_eq!(tree.get(&ancestor).name, Some("B".to_owned()))
    /// ```
    pub fn get_common_ancestor(&self, source: &NodeId, target: &NodeId) -> usize {
        if source == target {
            return *source;
        }
        let root_to_source = self.get_path_from_root(source);
        let root_to_target = self.get_path_from_root(target);

        let cursor = zip(root_to_source.iter(), root_to_target.iter())
            .enumerate()
            .filter(|(_, (s, t))| s != t)
            .map(|(idx, _)| idx)
            .next()
            .unwrap_or_else(|| {
                // One node is a child of the other
                root_to_source.len().min(root_to_target.len())
            });

        root_to_source[cursor - 1]
    }

    /// Gets the distance between 2 nodes, returns the sum of branch lengths (if all
    /// branches in the path have lengths) and the number of edges in the path.
    /// ```
    /// use phylotree::tree::Tree;
    ///
    /// let tree = Tree::from_newick("((A,(C,E)D)B,((H)I)G)F;").unwrap();
    /// let (sum_edge_lengths, num_edges) = tree.get_distance(
    ///     &tree.get_by_name("A").unwrap().id,
    ///     &tree.get_by_name("I").unwrap().id,
    /// );
    ///
    /// assert_eq!(num_edges, 4);
    /// assert!(sum_edge_lengths.is_none());
    /// ```
    pub fn get_distance(&self, source: &NodeId, target: &NodeId) -> (Option<f64>, usize) {
        let mut dist = 0.0;
        let mut branches = 0;
        let mut all_dists = true;

        if source == target {
            return (None, 0);
        }

        let root_to_source = self.get_path_from_root(source);
        let root_to_target = self.get_path_from_root(target);

        let cursor = zip(root_to_source.iter(), root_to_target.iter())
            .enumerate()
            .filter(|(_, (s, t))| s != t)
            .map(|(idx, _)| idx)
            .next()
            .unwrap_or_else(|| {
                // One node is a child of the other
                root_to_source.len().min(root_to_target.len())
            });

        for list in vec![root_to_source, root_to_target] {
            for node in list.iter().skip(cursor) {
                if let Some(d) = self.get(node).parent_edge {
                    dist += d;
                } else {
                    all_dists = false;
                }
                branches += 1;
            }
        }

        if all_dists {
            (Some(dist), branches)
        } else {
            (None, branches)
        }
    }

    // ##################
    // # alter the tree #
    // ##################

    /// Prune the subtree starting at a given root node.
    /// # Example
    /// ```
    /// use phylotree::tree::Tree;
    ///
    /// let mut tree = Tree::from_newick("((A,(C,E)D)B,((H)I)G)F;").unwrap();
    /// let root_idx = tree.get_by_name("G").unwrap().id;
    ///
    /// tree.prune(&root_idx);
    ///
    /// assert_eq!(tree.to_newick().unwrap(), String::from("((A,(C,E)D)B)F;"))
    /// ```
    pub fn prune(&mut self, root: &NodeId) {
        for child in self.get(root).children.clone() {
            self.prune(&child)
        }

        if let Some(parent) = self.get(root).parent {
            self.get_mut(&parent).children.retain(|val| val != root);
        }

        self.get_mut(root).delete();
    }

    /// Compress the tree (i.e. remove nodes with exactly 1 parent and 1 child and fuse branches together)
    pub fn compress(&mut self) {

    }

    /// Rescale the branch lenghts of the tree
    /// ```
    /// use phylotree::tree::Tree;
    ///
    /// let mut tree = Tree::from_newick("(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;").unwrap();
    /// // Double all branch lengths
    /// tree.rescale(2.0);
    ///
    /// assert_eq!(
    ///     tree.to_newick().unwrap(),
    ///     "(A:0.2,B:0.4,(C:0.6,D:0.8)E:1)F;"
    /// )
    /// ```
    pub fn rescale(&mut self, factor: f64) {
        for node in self.nodes.iter_mut() {
            node.rescale_edges(factor)
        }
    }

    // ########################
    // # read and write trees #
    // ########################

    /// Generate newick representation of tree
    fn to_newick_impl(&self, root: &NodeId) -> String {
        let root = self.get(root);
        if root.children.is_empty() {
            root.to_newick()
        } else {
            "(".to_string()
                + &(root
                    .children
                    .iter()
                    .map(|child_idx| self.to_newick_impl(child_idx)))
                .collect::<Vec<String>>()
                .join(",")
                + ")"
                + &(root.to_newick())
        }
    }

    /// Writes the tree as a newick formatted string
    /// # Example
    /// ```
    /// use phylotree::tree::Tree;
    ///
    /// let newick = "(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F:0.6;";
    /// let tree = Tree::from_newick(newick).unwrap();
    ///
    /// dbg!(&tree);
    ///
    /// assert_eq!(tree.to_newick().unwrap(), newick);
    /// ```
    pub fn to_newick(&self) -> Result<String, TreeError> {
        let root = self.get_root()?;
        Ok(self.to_newick_impl(&root) + ";")
    }

    /// Read a newick formatted string and build a [`Tree`] struct from it.
    /// # Example
    /// ```
    /// use phylotree::tree::Tree;
    ///
    /// let newick = "(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;";
    /// let tree = Tree::from_newick(newick).unwrap();
    ///
    /// assert_eq!(tree.size(), 6);
    /// assert_eq!(tree.n_leaves(), 4);
    /// assert_eq!(tree.is_rooted().unwrap(), false);
    /// ```
    pub fn from_newick(newick: &str) -> Result<Self, ParseError> {
        #[derive(Debug, PartialEq)]
        enum Field {
            Name,
            Length,
            Comment,
        }

        let mut tree = Tree::new();

        let mut parsing = Field::Name;
        let mut current_name: Option<String> = None;
        let mut current_length: Option<String> = None;
        let mut current_comment: Option<String> = None;
        let mut current_index: Option<NodeId> = None;
        let mut parent_stack: Vec<NodeId> = Vec::new();

        let mut open_delimiters = Vec::new();
        let mut within_quotes = false;

        for c in newick.chars() {
            // Add character in quotes to name
            if within_quotes && parsing == Field::Name && c != '"' {
                if let Some(name) = current_name.as_mut() {
                    name.push(c)
                } else {
                    current_name = Some(c.into())
                }
                continue;
            }

            // Add current character to comment
            if parsing == Field::Comment && c != ']' {
                if let Some(comment) = current_comment.as_mut() {
                    comment.push(c)
                } else {
                    current_comment = Some(c.into())
                }
                continue;
            }

            match c {
                '"' => {
                    // Enter or close quoted section (name)
                    // TODO: handle escaped quotes
                    within_quotes = !within_quotes;
                    if parsing == Field::Name {
                        if let Some(name) = current_name.as_mut() {
                            name.push(c)
                        } else {
                            current_name = Some(c.into())
                        }
                    }
                }
                '[' => {
                    parsing = Field::Comment;
                }
                ']' => {
                    parsing = Field::Name;
                }
                '(' => {
                    // Start subtree
                    match parent_stack.last() {
                        None => parent_stack.push(tree.add(Node::new())),
                        Some(parent) => {
                            parent_stack.push(tree.add_child(Node::new(), *parent, None)?)
                        }
                    };
                    open_delimiters.push(0);
                }
                ':' => {
                    // Start parsing length
                    parsing = Field::Length;
                }
                ',' => {
                    // Add sibling
                    let node = if let Some(index) = current_index {
                        tree.get_mut(&index)
                    } else {
                        if let Some(parent) = parent_stack.last() {
                            current_index = Some(tree.add_child(Node::new(), *parent, None)?);
                        } else {
                            unreachable!("Sould not be possible to have named child with no parent")
                        };
                        tree.get_mut(current_index.as_ref().unwrap())
                    };

                    if let Some(name) = current_name {
                        node.set_name(name);
                    }

                    let edge = if let Some(length) = current_length {
                        Some(length.parse()?)
                    } else {
                        None
                    };
                    if let Some(parent) = node.parent {
                        node.set_parent(parent, edge);
                    }

                    node.comment = current_comment;

                    current_name = None;
                    current_comment = None;
                    current_length = None;
                    current_index = None;

                    parsing = Field::Name;
                }
                ')' => {
                    // Close subtree
                    open_delimiters.pop();
                    let node = if let Some(index) = current_index {
                        tree.get_mut(&index)
                    } else {
                        if let Some(parent) = parent_stack.last() {
                            current_index = Some(tree.add_child(Node::new(), *parent, None)?);
                        } else {
                            unreachable!("Sould not be possible to have named child with no parent")
                        };
                        tree.get_mut(current_index.as_ref().unwrap())
                    };

                    if let Some(name) = current_name {
                        node.set_name(name);
                    }

                    let edge = if let Some(length) = current_length {
                        Some(length.parse()?)
                    } else {
                        None
                    };
                    if let Some(parent) = node.parent {
                        node.set_parent(parent, edge);
                    }

                    node.comment = current_comment;

                    current_name = None;
                    current_comment = None;
                    current_length = None;

                    parsing = Field::Name;

                    if let Some(parent) = parent_stack.pop() {
                        current_index = Some(parent)
                    } else {
                        return Err(ParseError::NoSubtreeParent);
                    }
                }
                ';' => {
                    // Finish parsing the Tree
                    if !open_delimiters.is_empty() {
                        return Err(ParseError::UnclosedBracket);
                    }
                    let node = tree.get_mut(current_index.as_ref().unwrap());
                    node.name = current_name;
                    node.comment = current_comment;
                    if let Some(length) = current_length {
                        node.parent_edge = Some(length.parse()?);
                    }

                    // Finishing pass to make sure that branch lenghts are set in both children and parents
                    let ids: Vec<_> = tree.nodes.iter().map(|node| node.id).collect();
                    for node_id in ids {
                        if let Some(edge) = tree.get(&node_id).parent_edge {
                            if let Some(parent) = tree.get(&node_id).parent {
                                tree.get_mut(&parent).set_child_edge(&node_id, Some(edge));
                            }
                        }
                    }

                    return Ok(tree);
                }
                _ => {
                    // Parse characters in fields
                    match parsing {
                        Field::Name => {
                            if let Some(name) = current_name.as_mut() {
                                name.push(c)
                            } else {
                                current_name = Some(c.into())
                            }
                        }
                        Field::Length => {
                            if c.is_whitespace() {
                                return Err(ParseError::WhiteSpaceInNumber);
                            }
                            if let Some(length) = current_length.as_mut() {
                                length.push(c)
                            } else {
                                current_length = Some(c.into())
                            }
                        }
                        Field::Comment => unimplemented!(),
                    };
                }
            }
        }

        Err(ParseError::NoClosingSemicolon)
    }

    /// Writes the tree to a newick file
    pub fn to_file(&self, path: &Path) -> Result<(), TreeError> {
        match fs::write(path, self.to_newick()?) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Creates a tree from a newick file
    pub fn from_file(path: &Path) -> Result<Self, ParseError> {
        let newick_string = fs::read_to_string(path)?;
        Self::from_newick(&newick_string)
    }
}

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::excessive_precision)]
mod tests {

    use super::*;

    /// Generates example tree from the tree traversal wikipedia page
    /// https://en.wikipedia.org/wiki/Tree_traversal#Depth-first_search
    /// The difference is that I is the left child of G since this tree structure
    /// cannot represent a right child only.
    fn build_simple_tree() -> Tree {
        let mut tree = Tree::new();
        tree.add(Node::new_named("F")); // 0
        tree.add_child(Node::new_named("B"), 0, None); // 1
        tree.add_child(Node::new_named("G"), 0, None); // 2
        tree.add_child(Node::new_named("A"), 1, None); // 3
        tree.add_child(Node::new_named("D"), 1, None); // 4
        tree.add_child(Node::new_named("I"), 2, None); // 5
        tree.add_child(Node::new_named("C"), 4, None); // 6
        tree.add_child(Node::new_named("E"), 4, None); // 7
        tree.add_child(Node::new_named("H"), 5, None); // 8

        tree
    }

    /// Generates example tree from the newick format wikipedia page
    /// https://en.wikipedia.org/wiki/Newick_format#Examples
    fn build_tree_with_lengths() -> Tree {
        let mut tree = Tree::new();
        tree.add_child(Node::new_named("F"), 0, None); // 1
        tree.add_child(Node::new_named("A"), 0, Some(0.1)); // 1
        tree.add_child(Node::new_named("B"), 0, Some(0.2)); // 2
        tree.add_child(Node::new_named("E"), 0, Some(0.5)); // 3
        tree.add_child(Node::new_named("C"), 3, Some(0.3)); // 4
        tree.add_child(Node::new_named("D"), 3, Some(0.4)); // 5

        tree
    }

    fn get_values(indices: &[usize], tree: &Tree) -> Vec<Option<String>> {
        indices
            .iter()
            .map(|idx| tree.get(idx).name.clone())
            .collect()
    }

    #[test]
    fn test_tips() {
        let mut tree = Tree::new();
        tree.add(Node::new_named("root"));
        assert_eq!(tree.get_leaves(), vec![0]);

        tree.add_child(Node::new_named("A"), 0, Some(0.1)); // 1
        tree.add_child(Node::new_named("B"), 0, Some(0.2)); // 2
        tree.add_child(Node::new_named("E"), 0, Some(0.5)); // 3

        assert_eq!(tree.get_leaves(), vec![1, 2, 3]);

        tree.add_child(Node::new_named("C"), 3, Some(0.3)); // 4
        tree.add_child(Node::new_named("D"), 3, Some(0.4)); // 5

        assert_eq!(tree.get_leaves(), vec![1, 2, 4, 5]);
    }

    #[test]
    fn test_binary() {
        let mut tree = Tree::new();
        tree.add(Node::new_named("root"));

        tree.add_child(Node::new_named("0L"), 0, None); //1
        tree.add_child(Node::new_named("0R"), 0, None); //2

        assert!(tree.is_binary());

        tree.add_child(Node::new_named("1L"), 1, None); //3
        tree.add_child(Node::new_named("1R"), 1, None); //4

        assert!(tree.is_binary());

        tree.add_child(Node::new_named("3L"), 3, None); //5
        tree.add_child(Node::new_named("3R"), 3, None); //6
        assert!(tree.is_binary());

        tree.add_child(Node::new_named("3?"), 3, None); //7
        assert!(!tree.is_binary());
    }

    #[test]
    fn binary_from_newick() {
        let test_cases = vec![("(A,B,(C,D)E)F;", false), ("((D,E)B,(F,G)C)A;", true)];

        for (newick, is_binary) in test_cases {
            assert_eq!(Tree::from_newick(newick).unwrap().is_binary(), is_binary)
        }
    }

    // #[test]
    // fn traverse_preorder() {
    //     let tree = build_simple_tree();
    //     let values: Vec<_> = get_values(&(tree.preorder(0).unwrap()), &tree)
    //         .into_iter()
    //         .flatten()
    //         .collect();
    //     assert_eq!(values, vec!["F", "B", "A", "D", "C", "E", "G", "I", "H"])
    // }

    // #[test]
    // fn iter_preorder() {
    //     let tree = build_simple_tree();
    //     let values: Vec<_> = tree
    //         .iter_preorder()
    //         .flat_map(|node| node.name.clone())
    //         .collect();
    //     assert_eq!(values, vec!["F", "B", "A", "D", "C", "E", "G", "I", "H"])
    // }

    // #[test]
    // fn traverse_postorder() {
    //     let tree = build_simple_tree();
    //     let values: Vec<_> = get_values(&(tree.postorder(0).unwrap()), &tree)
    //         .into_iter()
    //         .flatten()
    //         .collect();
    //     assert_eq!(values, vec!["A", "C", "E", "D", "B", "H", "I", "G", "F"])
    // }

    // #[test]
    // fn iter_postorder() {
    //     let tree = build_simple_tree();
    //     let values: Vec<_> = tree
    //         .iter_postorder()
    //         .unwrap()
    //         .flat_map(|node| node.name.clone())
    //         .collect();
    //     assert_eq!(values, vec!["A", "C", "E", "D", "B", "H", "I", "G", "F"])
    // }

    // #[test]
    // fn traverse_inorder() {
    //     let tree = build_simple_tree();
    //     let values: Vec<_> = get_values(&(tree.inorder(0).unwrap()), &tree)
    //         .into_iter()
    //         .flatten()
    //         .collect();
    //     assert_eq!(values, vec!["A", "B", "C", "D", "E", "F", "H", "I", "G"])
    // }

    // #[test]
    // fn traverse_levelorder() {
    //     let tree = build_simple_tree();
    //     let values: Vec<_> = get_values(&(tree.levelorder(0).unwrap()), &tree)
    //         .into_iter()
    //         .flatten()
    //         .collect();
    //     assert_eq!(values, vec!["F", "B", "G", "A", "D", "I", "C", "E", "H"])
    // }

    // #[test]
    // fn prune_tree() {
    //     let mut tree = build_simple_tree();
    //     tree.prune(4); // prune D subtree
    //     let values: Vec<_> = get_values(&(tree.preorder(0).unwrap()), &tree)
    //         .into_iter()
    //         .flatten()
    //         .collect();
    //     assert_eq!(values, vec!["F", "B", "A", "G", "I", "H"]);
    // }

    #[test]
    fn path_from_root() {
        let tree = build_simple_tree();
        let values: Vec<_> = get_values(&(tree.get_path_from_root(&7)), &tree)
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(values, vec!["F", "B", "D", "E"])
    }

    #[test]
    fn last_common_ancestor() {
        let test_cases = vec![
            ((3, 7), 1), // (A,E) -> B
            ((6, 8), 0), // (C,H) -> F
            ((3, 3), 3), // (A,A) -> A
            ((8, 5), 5), // (H,I) -> I
            ((4, 7), 4), // (D,E) -> D
        ];
        let tree = build_simple_tree();
        for ((source, target), ancestor) in test_cases {
            println!(
                "Testing: ({:?}, {:?}) -> {:?}",
                tree.get(&source).name,
                tree.get(&target).name,
                tree.get(&ancestor).name
            );
            assert_eq!(ancestor, tree.get_common_ancestor(&source, &target));
        }
    }

    #[test]
    fn get_distances_lengths() {
        let test_cases = vec![
            ((1, 3), (Some(0.6), 2)), // (A,E)
            ((1, 4), (Some(0.9), 3)), // (A,C)
            ((4, 5), (Some(0.7), 2)), // (C,D)
            ((5, 2), (Some(1.1), 3)), // (D,B)
            ((2, 5), (Some(1.1), 3)), // (B,D)
            ((0, 2), (Some(0.2), 1)), // (F,B)
            ((1, 1), (None, 0)),      // (A,A)
        ];
        let tree = build_tree_with_lengths();

        for ((idx_s, idx_t), (dist, branches)) in test_cases {
            let (d_pred, b_pred) = tree.get_distance(&idx_s, &idx_t);
            assert_eq!(branches, b_pred);
            match dist {
                None => assert!(d_pred.is_none()),
                Some(d) => {
                    assert!(d_pred.is_some());
                    assert!((d_pred.unwrap() - d).abs() < f64::EPSILON);
                }
            }
        }
    }

    #[test]
    fn get_correct_leaves() {
        let tree = build_simple_tree();
        let values: Vec<_> = get_values(&(tree.get_leaves()), &tree)
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(values, vec!["A", "C", "E", "H"])
    }

    // #[test]
    // fn generate_random_correct_size() {
    //     use rand::prelude::*;
    //     let mut rng = thread_rng();

    //     for size in (0..20).map(|_| rng.gen_range(10..=100)) {
    //         let tree = generate_tree(size, false, Distr::Uniform);
    //         assert_eq!(tree.get_leaves().len(), size);
    //     }
    // }

    // #[test]
    // fn genera_gamma() {
    //     use rand::prelude::*;
    //     let mut rng = thread_rng();
    //     for size in (0..20).map(|_| rng.gen_range(10..=100)) {
    //         let tree = generate_tree(size, true, Distr::Gamma);
    //         let tree2 = generate_tree(size, true, Distr::Uniform);
    //         assert_eq!(tree.get_leaves().len(), size);
    //         assert_eq!(tree2.get_leaves().len(), size);
    //         println!("G: {}", tree.to_newick());
    //         println!("U: {}", tree2.to_newick())
    //     }
    // }

    #[test]
    fn to_newick() {
        let tree = build_tree_with_lengths();
        assert_eq!(
            "(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;",
            tree.to_newick().unwrap()
        );
    }

    // test cases from https://github.com/ila/Newick-validator
    #[test]
    fn read_newick() {
        let newick_strings = vec![
            "((D,E)B,(F,G)C)A;",
            "(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;",
            "(A:0.1,B:0.2,(C:0.3,D:0.4):0.5);",
            "(dog:20,(elephant:30,horse:60):20):50;",
            "(A,B,(C,D));",
            "(A,B,(C,D)E)F;",
            "(((One:0.2,Two:0.3):0.3,(Three:0.5,Four:0.3):0.2):0.3,Five:0.7):0;",
            "(:0.1,:0.2,(:0.3,:0.4):0.5);",
            "(:0.1,:0.2,(:0.3,:0.4):0.5):0;",
            "(A:0.1,B:0.2,(C:0.3,D:0.4):0.5);",
            "(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;",
            "((B:0.2,(C:0.3,D:0.4)E:0.5)A:0.1)F;",
            "(,,(,));",
            "(\"hungarian dog\":20,(\"indian elephant\":30,\"swedish horse\":60):20):50;",
            "(\"hungarian dog\"[Comment_1]:20,(\"indian elephant\":30,\"swedish horse[Another interesting comment]\":60):20):50;",
        ];
        for newick in newick_strings {
            let tree = Tree::from_newick(newick).unwrap();
            assert_eq!(newick, tree.to_newick().unwrap());
        }
    }

    #[test]
    fn read_newick_fails() {
        let newick_strings = vec![
            ("((D,E)B,(F,G,C)A;", ParseError::UnclosedBracket),
            ("((D,E)B,(F,G)C)A", ParseError::NoClosingSemicolon),
        ];
        for (newick, _error) in newick_strings {
            let tree = Tree::from_newick(newick);
            assert!(tree.is_err());
        }
    }

    #[test]
    fn test_height() {
        // heights computed with ete3
        let test_cases = vec![
            ("(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;", 0.9),
            ("((B:0.2,(C:0.3,D:0.4)E:0.5)A:0.1)F;", 1.0),
            ("(A,B,(C,D)E)F;", 2.0),
            (
                "((((((((Tip9,Tip8),Tip7),Tip6),Tip5),Tip4),Tip3),Tip2),Tip0,Tip1);",
                8.0,
            ),
        ];

        for (newick, height) in test_cases {
            assert_eq!(Tree::from_newick(newick).unwrap().height().unwrap(), height)
        }
    }

    #[test]
    fn test_diam() {
        let test_cases = vec![
            ("((D,E)B,(F,G)C)A;", 4.0),
            ("(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;", 1.1),
            ("(A:0.1,B:0.2,(C:0.3,D:0.4):0.5);", 1.1),
            ("(A,B,(C,D));", 3.0),
            ("(A,B,(C,D)E)F;", 3.0),
        ];

        for (newick, diameter) in test_cases {
            assert_eq!(
                Tree::from_newick(newick).unwrap().diameter().unwrap(),
                diameter
            )
        }
    }

    #[test]
    fn test_cherries() {
        // Number of cherries computed with gotree
        let test_cases = vec![
            ("(((((((((Tip9,Tip8),Tip7),Tip6),Tip5),Tip4),Tip3),Tip2),Tip1),Tip0);", 1),
            ("(((i:0.1,j:0.1):0.1,(a:0.1,b:0.1):0.1):0.1,((c:0.1,d:0.1):0.1,((e:0.1,f:0.1):0.1,(g:0.1,h:0.1):0.1):0.1):0.1);", 5),
            ("((a:0.2,b:0.2):0.2,((c:0.2,d:0.2):0.2,((e:0.2,f:0.2):0.2,((g:0.2,h:0.2):0.2,(i:0.2,j:0.2):0.2):0.2):0.2):0.2);", 5),
            ("(((d:0.3,e:0.3):0.3,((f:0.3,g:0.3):0.3,(h:0.3,(i:0.3,j:0.3):0.3):0.3):0.3):0.3,(a:0.3,(b:0.3,c:0.3):0.3):0.3);", 4),
        ];

        for (newick, true_cherries) in test_cases {
            let tree = Tree::from_newick(newick).unwrap();
            let cherries = tree.cherries();
            assert!(cherries.is_ok());
            assert_eq!(cherries.unwrap(), true_cherries);
        }
    }

    #[test]
    fn test_colless_rooted() {
        // Colless index computed with gotree
        let test_cases = vec![
            ("(((((((((Tip9,Tip8),Tip7),Tip6),Tip5),Tip4),Tip3),Tip2),Tip1),Tip0);", 36),
            ("(((i:0.1,j:0.1):0.1,(a:0.1,b:0.1):0.1):0.1,((c:0.1,d:0.1):0.1,((e:0.1,f:0.1):0.1,(g:0.1,h:0.1):0.1):0.1):0.1);", 4),
            ("((a:0.2,b:0.2):0.2,((c:0.2,d:0.2):0.2,((e:0.2,f:0.2):0.2,((g:0.2,h:0.2):0.2,(i:0.2,j:0.2):0.2):0.2):0.2):0.2);", 12),
            ("(((d:0.3,e:0.3):0.3,((f:0.3,g:0.3):0.3,(h:0.3,(i:0.3,j:0.3):0.3):0.3):0.3):0.3,(a:0.3,(b:0.3,c:0.3):0.3):0.3);", 10),
        ];

        for (newick, true_colless) in test_cases {
            let tree = Tree::from_newick(newick).unwrap();
            let colless = tree.colless();
            assert!(colless.is_ok());
            if tree.colless().unwrap() != true_colless {
                panic!(
                    "Computed colless {} not equal to true {true_colless}",
                    tree.colless().unwrap()
                )
            };
        }
    }

    #[test]
    fn test_sackin_rooted() {
        // Sackin index computed with gotree
        let test_cases = vec![
            ("(((((((((Tip9,Tip8),Tip7),Tip6),Tip5),Tip4),Tip3),Tip2),Tip1),Tip0);", 54),
            ("(((i:0.1,j:0.1):0.1,(a:0.1,b:0.1):0.1):0.1,((c:0.1,d:0.1):0.1,((e:0.1,f:0.1):0.1,(g:0.1,h:0.1):0.1):0.1):0.1);", 34),
            ("((a:0.2,b:0.2):0.2,((c:0.2,d:0.2):0.2,((e:0.2,f:0.2):0.2,((g:0.2,h:0.2):0.2,(i:0.2,j:0.2):0.2):0.2):0.2):0.2);", 38),
            ("(((d:0.3,e:0.3):0.3,((f:0.3,g:0.3):0.3,(h:0.3,(i:0.3,j:0.3):0.3):0.3):0.3):0.3,(a:0.3,(b:0.3,c:0.3):0.3):0.3);", 36),
        ];

        for (newick, true_sackin) in test_cases {
            let tree = Tree::from_newick(newick).unwrap();
            let sackin = tree.sackin();
            assert!(sackin.is_ok());
            assert_eq!(tree.sackin().unwrap(), true_sackin);
        }
    }

    #[test]
    fn test_sackin_unrooted() {
        let test_cases = vec![
            "(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;",
            "((B:0.2,(C:0.3,D:0.4)E:0.5)A:0.1)F;",
            "(A,B,(C,D)E)F;",
            "((((((((Tip9,Tip8),Tip7),Tip6),Tip5),Tip4),Tip3),Tip2),Tip0,Tip1);",
        ];

        for newick in test_cases {
            let tree = Tree::from_newick(newick).unwrap();
            assert!(tree.sackin().is_err())
        }
    }

    #[test]
    fn test_rescale() {
        let test_cases = [
            ("((D:0.05307533041908017723553570021977,(C:0.08550401213833067060043902074540,(B:0.27463239708134284944307523801399,A:0.37113575171985613287972682883264)1:0.18134330279626256765546088445262)1:0.08033066840794983454188127325324)1:0.13864016688124142229199264875206,E:0.05060148260657528623829293223935);",
            "((D:0.04212872094323715649322181775460,(C:0.06786909546224775824363462106703,(B:0.21799038323938338401752901063446,A:0.29459024358034957558061250892933)1:0.14394185279875840177687962295749)1:0.06376273658252405718283029045779)1:0.11004609591585229333432494058798,E:0.04016509597234880352134567260691);",
            0.6525060248498331),
            ("(E:0.01699652764738122934229380689430,(D:0.00408169520164380558724381842239,(C:0.19713461567160570075962766622979,(B:0.12068059163592816107613003850929,A:0.45190753170439451613660253315174)1:0.03279750996120785189180679708443)1:0.21625179801434316062547225101298)1:0.03998705111996220251668887613050);",
            "(E:0.01986870266959113798255209815125,(D:0.00477144449924469995355513773916,(C:0.23044760352958004734347241537762,(B:0.14107392068250154681940955470054,A:0.52827357257097584675165080625447)1:0.03833982959587604877338407050047)1:0.25279532182407132845369801543711)1:0.04674430247278672095889717752470);",
            0.8860217291333011),
            ("((C:0.20738366520293352590620372666308,(B:0.19695170474498663315543467433599,A:0.02188551422116874478618342436675)1:0.05940680521299050026451382677806)1:0.13029006694844610936279138968530,(E:0.17189347707484656235799036494427,D:0.05867747522240193691622778260353)1:0.08673941227771603257323818070290);",
            "((C:0.18371634870356487456710681271943,(B:0.17447491841406459478491797199240,A:0.01938786624432843955223582099734)1:0.05262710219338979922287791168856)1:0.11542092936147484161235610145013,(E:0.15227641937588842768747099398752,D:0.05198100577716616849111019860175)1:0.07684042085359836515845444182560);",
            0.571639790198416),
        ];

        for (orig, rescaled, scale) in test_cases {
            let mut tree = Tree::from_newick(orig).unwrap();
            let rescaled = Tree::from_newick(rescaled).unwrap();

            tree.rescale(scale);

            println!("Dealing with tree: {} and scale {}", orig, scale);
            for (n1, n2) in zip(tree.nodes, rescaled.nodes) {
                assert_eq!(n1, n2)
            }
        }
    }

    #[test]
    fn test_mutability() {
        let tree = Tree::from_newick("(A:0.1,B:0.2,(C:0.3,D:0.4)E:0.5)F;").unwrap();
        // First computation
        assert_eq!(tree.diameter().unwrap(), 1.1);
        assert_eq!(tree.height().unwrap(), 0.9);
        // Cached value
        eprintln!("{:?}", tree);
        assert_eq!(tree.diameter().unwrap(), 1.1);
        assert_eq!(tree.height().unwrap(), 0.9);
    }

    // #[test]
    // fn test_unique_tip_names() {
    //     let test_cases = vec![
    //         ("(((((((((Tip9,Tip8),Tip7),Tip6),Tip5),Tip4),Tip3),Tip2),Tip1),Tip0);",true),
    //         ("(((i:0.1,j:0.1):0.1,(a:0.1,b:0.1):0.1):0.1,((c:0.1,d:0.1):0.1,((e:0.1,f:0.1):0.1,(g:0.1,h:0.1):0.1):0.1):0.1);", true),
    //         ("(((((((((,),),),),),),),),);",false),
    //         ("(((((((((Tip8,Tip8),Tip7),Tip6),Tip5),Tip4),Tip3),Tip2),Tip1),Tip0);",false),
    //     ];

    //     for (newick, is_unique) in test_cases {
    //         assert_eq!(
    //             Tree::from_newick(newick).unwrap().are_tip_names_unique(),
    //             is_unique,
    //             "Failed on: {newick}"
    //         )
    //     }
    // }

    // #[test]
    // fn test_descendants() {
    //     let tree = build_simple_tree();
    //     let descendants_b: Vec<_> = get_values(&tree.get_descendants(1), &tree)
    //         .into_iter()
    //         .flatten()
    //         .sorted()
    //         .collect();
    //     let descendants_g: Vec<_> = get_values(&tree.get_descendants(2), &tree)
    //         .into_iter()
    //         .flatten()
    //         .sorted()
    //         .collect();

    //     assert_eq!(descendants_b, vec!["A", "C", "D", "E"]);
    //     assert_eq!(descendants_g, vec!["H", "I"]);
    // }

    // #[test]
    // fn test_compress() {
    //     let mut tree = Tree::new(Some("root"));
    //     tree.add_child_with_len(Some("tip_A"), 0, Some(1.0));
    //     tree.add_child_with_len(Some("in_B"), 0, Some(1.0));
    //     tree.add_child_with_len(Some("in_C"), 2, Some(1.0));
    //     tree.add_child_with_len(Some("tip_D"), 3, Some(1.0));

    //     tree.compress().unwrap();

    //     assert_eq!(tree.to_newick(), "(tip_A:1,tip_D:3)root;");
    // }

    // #[test]
    // fn test_get_partitions() {
    //     let test_cases = vec![
    //         (
    //             "(((((((((Tip9,Tip8),Tip7),Tip6),Tip5),Tip4),Tip3),Tip2),Tip1),Tip0);", 
    //             "(Tip0,((Tip2,(Tip3,(Tip4,(Tip5,(Tip6,((Tip8,Tip9),Tip7)))))),Tip1));",
    //         ),
    //         (
    //             "(((i:0.1,j:0.1):0.1,(a:0.1,b:0.1):0.1):0.1,((c:0.1,d:0.1):0.1,((e:0.1,f:0.1):0.1,(g:0.1,h:0.1):0.1):0.1):0.1);", 
    //             "(((c:0.1,d:0.1):0.1,((g:0.1,h:0.1):0.1,(f:0.1,e:0.1):0.1):0.1):0.1,((i:0.1,j:0.1):0.1,(a:0.1,b:0.1):0.1):0.1);",
    //         ),
    //         (
    //             "((a:0.2,b:0.2):0.2,((c:0.2,d:0.2):0.2,((e:0.2,f:0.2):0.2,((g:0.2,h:0.2):0.2,(i:0.2,j:0.2):0.2):0.2):0.2):0.2);", 
    //             "((((e:0.2,f:0.2):0.2,((i:0.2,j:0.2):0.2,(g:0.2,h:0.2):0.2):0.2):0.2,(d:0.2,c:0.2):0.2):0.2,(b:0.2,a:0.2):0.2);",
    //         ),
    //         (
    //             "(((d:0.3,e:0.3):0.3,((f:0.3,g:0.3):0.3,(h:0.3,(i:0.3,j:0.3):0.3):0.3):0.3):0.3,(a:0.3,(b:0.3,c:0.3):0.3):0.3);", 
    //             "((((g:0.3,f:0.3):0.3,((i:0.3,j:0.3):0.3,h:0.3):0.3):0.3,(d:0.3,e:0.3):0.3):0.3,((b:0.3,c:0.3):0.3,a:0.3):0.3);",
    //         ),
    //     ];

    //     for (newick, rot_newick) in test_cases {
    //         let tree = Tree::from_newick(newick).unwrap();
    //         let rota = Tree::from_newick(rot_newick).unwrap();

    //         let ps_orig: HashSet<_> = HashSet::from_iter(tree.get_partitions().unwrap());
    //         let ps_rota: HashSet<_> = HashSet::from_iter(rota.get_partitions().unwrap());

    //         assert_eq!(ps_orig, ps_rota);
    //     }
    // }

    // #[test]
    // fn self_rf() {
    //     let test_cases = vec![
    //         (
    //             "(((((((((Tip9,Tip8),Tip7),Tip6),Tip5),Tip4),Tip3),Tip2),Tip1),Tip0);", 
    //             "(Tip0,((Tip2,(Tip3,(Tip4,(Tip5,(Tip6,((Tip8,Tip9),Tip7)))))),Tip1));",
    //         ),
    //         (
    //             "(((i:0.1,j:0.1):0.1,(a:0.1,b:0.1):0.1):0.1,((c:0.1,d:0.1):0.1,((e:0.1,f:0.1):0.1,(g:0.1,h:0.1):0.1):0.1):0.1);", 
    //             "(((c:0.1,d:0.1):0.1,((g:0.1,h:0.1):0.1,(f:0.1,e:0.1):0.1):0.1):0.1,((i:0.1,j:0.1):0.1,(a:0.1,b:0.1):0.1):0.1);",
    //         ),
    //         (
    //             "((a:0.2,b:0.2):0.2,((c:0.2,d:0.2):0.2,((e:0.2,f:0.2):0.2,((g:0.2,h:0.2):0.2,(i:0.2,j:0.2):0.2):0.2):0.2):0.2);", 
    //             "((((e:0.2,f:0.2):0.2,((i:0.2,j:0.2):0.2,(g:0.2,h:0.2):0.2):0.2):0.2,(d:0.2,c:0.2):0.2):0.2,(b:0.2,a:0.2):0.2);",
    //         ),
    //         (
    //             "(((d:0.3,e:0.3):0.3,((f:0.3,g:0.3):0.3,(h:0.3,(i:0.3,j:0.3):0.3):0.3):0.3):0.3,(a:0.3,(b:0.3,c:0.3):0.3):0.3);", 
    //             "((((g:0.3,f:0.3):0.3,((i:0.3,j:0.3):0.3,h:0.3):0.3):0.3,(d:0.3,e:0.3):0.3):0.3,((b:0.3,c:0.3):0.3,a:0.3):0.3);",
    //         ),
    //     ];

    //     for (newick, rot_newick) in test_cases {
    //         let tree = Tree::from_newick(newick).unwrap();
    //         let rota = Tree::from_newick(rot_newick).unwrap();

    //         tree.init_leaf_index().unwrap();
    //         rota.init_leaf_index().unwrap();

    //         assert_eq!(
    //             tree.robinson_foulds(&rota).unwrap(),
    //             0,
    //             "Ref{:#?}\nRot:{:#?}",
    //             tree.leaf_index,
    //             rota.leaf_index
    //         );
    //     }
    // }

    // #[test]
    // // Robinson foulds distances according to
    // // https://evolution.genetics.washington.edu/phylip/doc/treedist.html
    // fn robinson_foulds_treedist() {
    //     let trees = vec![
    //         "(A:0.1,(B:0.1,(H:0.1,(D:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,((J:0.1,H:0.1):0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,(H:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((F:0.1,I:0.1):0.1,(G:0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((F:0.1,I:0.1):0.1,(G:0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,(H:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //     ];
    //     let rfs = vec![
    //         vec![0, 4, 2, 10, 10, 10, 10, 10, 10, 10, 2, 10],
    //         vec![4, 0, 2, 10, 8, 10, 8, 10, 8, 10, 2, 10],
    //         vec![2, 2, 0, 10, 10, 10, 10, 10, 10, 10, 0, 10],
    //         vec![10, 10, 10, 0, 2, 2, 4, 2, 4, 0, 10, 2],
    //         vec![10, 8, 10, 2, 0, 4, 2, 4, 2, 2, 10, 4],
    //         vec![10, 10, 10, 2, 4, 0, 2, 2, 4, 2, 10, 2],
    //         vec![10, 8, 10, 4, 2, 2, 0, 4, 2, 4, 10, 4],
    //         vec![10, 10, 10, 2, 4, 2, 4, 0, 2, 2, 10, 0],
    //         vec![10, 8, 10, 4, 2, 4, 2, 2, 0, 4, 10, 2],
    //         vec![10, 10, 10, 0, 2, 2, 4, 2, 4, 0, 10, 2],
    //         vec![2, 2, 0, 10, 10, 10, 10, 10, 10, 10, 0, 10],
    //         vec![10, 10, 10, 2, 4, 2, 4, 0, 2, 2, 10, 0],
    //     ];

    //     for indices in (0..trees.len()).combinations(2) {
    //         let (i0, i1) = (indices[0], indices[1]);

    //         let t0 = Tree::from_newick(trees[i0]).unwrap();
    //         let t1 = Tree::from_newick(trees[i1]).unwrap();

    //         assert_eq!(t0.robinson_foulds(&t1).unwrap(), rfs[i0][i1])
    //     }
    // }

    // #[test]
    // // Robinson foulds distances according to
    // // https://evolution.genetics.washington.edu/phylip/doc/treedist.html
    // fn weighted_robinson_foulds_treedist() {
    //     let trees = vec![
    //         "(A:0.1,(B:0.1,(H:0.1,(D:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,((J:0.1,H:0.1):0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,(H:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((F:0.1,I:0.1):0.1,(G:0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((F:0.1,I:0.1):0.1,(G:0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,(H:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //     ];
    //     let rfs = vec![
    //         vec![
    //             0.,
    //             0.4,
    //             0.2,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.2,
    //             0.9999999999999999,
    //         ],
    //         vec![
    //             0.4,
    //             0.,
    //             0.2,
    //             0.9999999999999999,
    //             0.7999999999999999,
    //             0.9999999999999999,
    //             0.7999999999999999,
    //             0.9999999999999999,
    //             0.7999999999999999,
    //             0.9999999999999999,
    //             0.2,
    //             0.9999999999999999,
    //         ],
    //         vec![
    //             0.2,
    //             0.2,
    //             0.,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.,
    //             0.9999999999999999,
    //         ],
    //         vec![
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.,
    //             0.2,
    //             0.2,
    //             0.4,
    //             0.2,
    //             0.4,
    //             0.,
    //             0.9999999999999999,
    //             0.2,
    //         ],
    //         vec![
    //             0.9999999999999999,
    //             0.7999999999999999,
    //             0.9999999999999999,
    //             0.2,
    //             0.,
    //             0.4,
    //             0.2,
    //             0.4,
    //             0.2,
    //             0.2,
    //             0.9999999999999999,
    //             0.4,
    //         ],
    //         vec![
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.2,
    //             0.4,
    //             0.,
    //             0.2,
    //             0.2,
    //             0.4,
    //             0.2,
    //             0.9999999999999999,
    //             0.2,
    //         ],
    //         vec![
    //             0.9999999999999999,
    //             0.7999999999999999,
    //             0.9999999999999999,
    //             0.4,
    //             0.2,
    //             0.2,
    //             0.,
    //             0.4,
    //             0.2,
    //             0.4,
    //             0.9999999999999999,
    //             0.4,
    //         ],
    //         vec![
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.2,
    //             0.4,
    //             0.2,
    //             0.4,
    //             0.,
    //             0.2,
    //             0.2,
    //             0.9999999999999999,
    //             0.,
    //         ],
    //         vec![
    //             0.9999999999999999,
    //             0.7999999999999999,
    //             0.9999999999999999,
    //             0.4,
    //             0.2,
    //             0.4,
    //             0.2,
    //             0.2,
    //             0.,
    //             0.4,
    //             0.9999999999999999,
    //             0.2,
    //         ],
    //         vec![
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.,
    //             0.2,
    //             0.2,
    //             0.4,
    //             0.2,
    //             0.4,
    //             0.,
    //             0.9999999999999999,
    //             0.2,
    //         ],
    //         vec![
    //             0.2,
    //             0.2,
    //             0.,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.,
    //             0.9999999999999999,
    //         ],
    //         vec![
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.9999999999999999,
    //             0.2,
    //             0.4,
    //             0.2,
    //             0.4,
    //             0.,
    //             0.2,
    //             0.2,
    //             0.9999999999999999,
    //             0.,
    //         ],
    //     ];

    //     for indices in (0..trees.len()).combinations(2) {
    //         let (i0, i1) = (indices[0], indices[1]);
    //         let t0 = Tree::from_newick(trees[i0]).unwrap();
    //         let t1 = Tree::from_newick(trees[i1]).unwrap();

    //         assert!((t0.weighted_robinson_foulds(&t1).unwrap() - rfs[i0][i1]).abs() <= f64::EPSILON)
    //     }
    // }

    // #[test]
    // // Branch score distances according to
    // // https://evolution.genetics.washington.edu/phylip/doc/treedist.html
    // fn khuner_felsenstein_treedist() {
    //     let trees = vec![
    //         "(A:0.1,(B:0.1,(H:0.1,(D:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,((J:0.1,H:0.1):0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,(H:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((F:0.1,I:0.1):0.1,(G:0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((F:0.1,I:0.1):0.1,(G:0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,(H:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //     ];
    //     let rfs: Vec<Vec<f64>> = vec![
    //         vec![
    //             0.,
    //             0.2,
    //             0.14142135623730953,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.14142135623730953,
    //             0.316227766016838,
    //         ],
    //         vec![
    //             0.2,
    //             0.,
    //             0.14142135623730953,
    //             0.316227766016838,
    //             0.28284271247461906,
    //             0.316227766016838,
    //             0.28284271247461906,
    //             0.316227766016838,
    //             0.28284271247461906,
    //             0.316227766016838,
    //             0.14142135623730953,
    //             0.316227766016838,
    //         ],
    //         vec![
    //             0.14142135623730953,
    //             0.14142135623730953,
    //             0.,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.,
    //             0.316227766016838,
    //         ],
    //         vec![
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.,
    //             0.14142135623730953,
    //             0.14142135623730953,
    //             0.2,
    //             0.14142135623730953,
    //             0.2,
    //             0.,
    //             0.316227766016838,
    //             0.14142135623730953,
    //         ],
    //         vec![
    //             0.316227766016838,
    //             0.28284271247461906,
    //             0.316227766016838,
    //             0.14142135623730953,
    //             0.,
    //             0.2,
    //             0.14142135623730953,
    //             0.2,
    //             0.14142135623730953,
    //             0.14142135623730953,
    //             0.316227766016838,
    //             0.2,
    //         ],
    //         vec![
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.14142135623730953,
    //             0.2,
    //             0.,
    //             0.14142135623730953,
    //             0.14142135623730953,
    //             0.2,
    //             0.14142135623730953,
    //             0.316227766016838,
    //             0.14142135623730953,
    //         ],
    //         vec![
    //             0.316227766016838,
    //             0.28284271247461906,
    //             0.316227766016838,
    //             0.2,
    //             0.14142135623730953,
    //             0.14142135623730953,
    //             0.,
    //             0.2,
    //             0.14142135623730953,
    //             0.2,
    //             0.316227766016838,
    //             0.2,
    //         ],
    //         vec![
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.14142135623730953,
    //             0.2,
    //             0.14142135623730953,
    //             0.2,
    //             0.,
    //             0.14142135623730953,
    //             0.14142135623730953,
    //             0.316227766016838,
    //             0.,
    //         ],
    //         vec![
    //             0.316227766016838,
    //             0.28284271247461906,
    //             0.316227766016838,
    //             0.2,
    //             0.14142135623730953,
    //             0.2,
    //             0.14142135623730953,
    //             0.14142135623730953,
    //             0.,
    //             0.2,
    //             0.316227766016838,
    //             0.14142135623730953,
    //         ],
    //         vec![
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.,
    //             0.14142135623730953,
    //             0.14142135623730953,
    //             0.2,
    //             0.14142135623730953,
    //             0.2,
    //             0.,
    //             0.316227766016838,
    //             0.14142135623730953,
    //         ],
    //         vec![
    //             0.14142135623730953,
    //             0.14142135623730953,
    //             0.,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.,
    //             0.316227766016838,
    //         ],
    //         vec![
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.316227766016838,
    //             0.14142135623730953,
    //             0.2,
    //             0.14142135623730953,
    //             0.2,
    //             0.,
    //             0.14142135623730953,
    //             0.14142135623730953,
    //             0.316227766016838,
    //             0.,
    //         ],
    //     ];

    //     for indices in (0..trees.len()).combinations(2) {
    //         let (i0, i1) = (indices[0], indices[1]);
    //         let t0 = Tree::from_newick(trees[i0]).unwrap();
    //         let t1 = Tree::from_newick(trees[i1]).unwrap();

    //         println!(
    //             "[{i0}, {i1}] c:{:?} ==? t:{}",
    //             t0.khuner_felsenstein(&t1).unwrap(),
    //             rfs[i0][i1]
    //         );

    //         assert_eq!(t0.khuner_felsenstein(&t1).unwrap(), rfs[i0][i1])
    //     }
    // }

    // #[test]
    // fn test_rf_unrooted() {
    //     let ref_s = "(((aaaaaaaaad:0.18749,aaaaaaaaae:0.18749):0.18749,((aaaaaaaaaf:0.18749,(aaaaaaaaag:0.18749,(aaaaaaaaah:0.18749,(aaaaaaaaai:0.18749,aaaaaaaaaj:0.18749):0.18749):0.18749):0.18749):0.18749,(aaaaaaaaak:0.18749,(aaaaaaaaal:0.18749,aaaaaaaaam:0.18749):0.18749):0.18749):0.18749):0.18749,((aaaaaaaaan:0.18749,aaaaaaaaao:0.18749):0.18749,(aaaaaaaaaa:0.18749,(aaaaaaaaab:0.18749,aaaaaaaaac:0.18749):0.18749):0.18749):0.18749);";
    //     let prd_s = "(aaaaaaaaag:0.24068,(aaaaaaaaah:0.21046,(aaaaaaaaai:0.15487,aaaaaaaaaj:0.17073)1.000:0.22813)0.999:0.26655,(aaaaaaaaaf:0.27459,((((aaaaaaaaan:0.17964,aaaaaaaaao:0.13686)0.994:0.18171,(aaaaaaaaaa:0.19386,(aaaaaaaaab:0.15663,aaaaaaaaac:0.20015)1.000:0.26799)0.981:0.15442)0.999:0.38320,(aaaaaaaaad:0.18133,aaaaaaaaae:0.17164)0.990:0.18734)0.994:0.18560,(aaaaaaaaak:0.24485,(aaaaaaaaal:0.17930,aaaaaaaaam:0.22072)1.000:0.22274)0.307:0.05569)1.000:0.22736)0.945:0.12401);";

    //     let ref_tree = Tree::from_newick(ref_s).unwrap();
    //     let prd_tree = Tree::from_newick(prd_s).unwrap();

    //     let ref_parts: HashSet<_> = HashSet::from_iter(ref_tree.get_partitions().unwrap());
    //     let prd_parts: HashSet<_> = HashSet::from_iter(prd_tree.get_partitions().unwrap());

    //     // let common = ref_parts.intersection(&prd_parts).count();

    //     println!("Leaf indices:");
    //     println!("Ref: {:#?}", ref_tree.leaf_index);

    //     println!("\nPartitions: ");
    //     println!("Ref: ");
    //     for i in ref_parts.iter().sorted() {
    //         println!("\t{i:#018b}");
    //     }
    //     println!("\nPrd: ");
    //     for i in prd_parts.iter().sorted() {
    //         println!("\t{i:#018b}");
    //     }

    //     // println!("tree\treference\tcommon\tcompared\trf\trf_comp");
    //     // println!(
    //     //     "0\t{}\t{}\t{}\t{}\t{}",
    //     //     ref_parts.len() - common,
    //     //     common,
    //     //     prd_parts.len() - common,
    //     //     rf,
    //     //     ref_parts.len() + prd_parts.len() - 2*common,
    //     // );

    //     // panic!()
    // }

    // #[test]
    // fn rooted_vs_unrooted_partitions() {
    //     let rooted = Tree::from_newick("((Tip_3,Tip_4),(Tip_0,(Tip_1,Tip_2)));").unwrap();
    //     let unrooted = Tree::from_newick("(Tip_3,Tip_4,(Tip_0,(Tip_1,Tip_2)));").unwrap();

    //     let parts_rooted = rooted.get_partitions().unwrap();
    //     let parts_unrooted = unrooted.get_partitions().unwrap();

    //     assert_eq!(parts_rooted, parts_unrooted);
    // }

    // #[test]
    // fn new_vs_old() {
    //     let trees = vec![
    //         "(A:0.1,(B:0.1,(H:0.1,(D:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,((J:0.1,H:0.1):0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,(H:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((F:0.1,I:0.1):0.1,(G:0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((F:0.1,I:0.1):0.1,(G:0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,(H:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //     ];

    //     for newicks in trees.iter().combinations(2) {
    //         let t1 = Tree::from_newick(newicks[0]).unwrap();
    //         let t2 = Tree::from_newick(newicks[1]).unwrap();

    //         assert_eq!(
    //             t1.robinson_foulds(&t2).unwrap(),
    //             t1.robinson_foulds(&t2).unwrap()
    //         )
    //     }
    // }

    // #[test]
    // fn medium() {
    //     fn get_bitset(hash: usize, len: usize) -> FixedBitSet {
    //         // eprintln!("Converting : {hash}");
    //         let mut set = FixedBitSet::with_capacity(len);
    //         let mut hash = hash;
    //         for i in 0..len {
    //             // eprintln!("\t{hash:#0len$b}", len = len);
    //             if hash & 1 == 1 {
    //                 set.insert(i)
    //             }
    //             hash >>= 1
    //         }
    //         let mut toggled = set.clone();
    //         toggled.toggle_range(..);

    //         // println!("Done\n");

    //         set.min(toggled)
    //     }

    //     let n1 = "((((Tip_13,Tip_14),(Tip_15,(Tip_16,Tip_17))),(((Tip_18,Tip_19),Tip_0),(Tip_1,Tip_2))),((Tip_3,(Tip_4,Tip_5)),(Tip_6,(Tip_7,(Tip_8,(Tip_9,(Tip_10,(Tip_11,Tip_12))))))));";
    //     let n2 = "(((Tip_7,(Tip_8,Tip_9)),((Tip_10,(Tip_11,Tip_12)),((Tip_13,(Tip_14,Tip_15)),(Tip_16,Tip_17)))),((Tip_18,Tip_19),(Tip_0,(Tip_1,(Tip_2,(Tip_3,(Tip_4,(Tip_5,Tip_6))))))));";
    //     let rf_true = 26;

    //     let reftree = Tree::from_newick(n1).unwrap();
    //     let compare = Tree::from_newick(n2).unwrap();

    //     let index: Vec<_> = reftree.get_leaf_names().into_iter().sorted().collect();
    //     let index2: Vec<_> = compare.get_leaf_names().into_iter().sorted().collect();

    //     let p1 = reftree.get_partitions().unwrap();
    //     println!("REF: [");
    //     for p in p1 {
    //         print!("(");
    //         for b in p.ones() {
    //             print!("{:?}, ", index[b]);
    //         }
    //         println!("),");
    //     }
    //     println!("]\n");

    //     let p2 = compare.get_partitions().unwrap();
    //     println!("COMP: [");
    //     for p in p2 {
    //         print!("(");
    //         for b in p.ones() {
    //             print!("{:?}, ", index2[b]);
    //         }
    //         println!("),");
    //     }
    //     println!("]\n");

    //     for node in compare.nodes.iter() {
    //         if node.is_tip() {
    //             println!("COMP TIP: {node:?}")
    //         }
    //     }

    //     // dbg!(&reftree);
    //     // dbg!(&compare);

    //     assert_eq!(reftree.robinson_foulds(&compare).unwrap(), rf_true)
    // }

    // #[test]
    // // Robinson foulds distances according to
    // // https://evolution.genetics.washington.edu/phylip/doc/treedist.html
    // fn robinson_foulds_treedist_new() {
    //     let trees = vec![
    //         "(A:0.1,(B:0.1,(H:0.1,(D:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,((J:0.1,H:0.1):0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,(H:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((F:0.1,I:0.1):0.1,(G:0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((F:0.1,I:0.1):0.1,(G:0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,(((J:0.1,H:0.1):0.1,D:0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,(G:0.1,((F:0.1,I:0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(D:0.1,(H:0.1,(J:0.1,(((G:0.1,E:0.1):0.1,(F:0.1,I:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1):0.1);",
    //         "(A:0.1,(B:0.1,(E:0.1,((G:0.1,(F:0.1,I:0.1):0.1):0.1,((J:0.1,(H:0.1,D:0.1):0.1):0.1,C:0.1):0.1):0.1):0.1):0.1);",
    //     ];
    //     let rfs = vec![
    //         vec![0, 4, 2, 10, 10, 10, 10, 10, 10, 10, 2, 10],
    //         vec![4, 0, 2, 10, 8, 10, 8, 10, 8, 10, 2, 10],
    //         vec![2, 2, 0, 10, 10, 10, 10, 10, 10, 10, 0, 10],
    //         vec![10, 10, 10, 0, 2, 2, 4, 2, 4, 0, 10, 2],
    //         vec![10, 8, 10, 2, 0, 4, 2, 4, 2, 2, 10, 4],
    //         vec![10, 10, 10, 2, 4, 0, 2, 2, 4, 2, 10, 2],
    //         vec![10, 8, 10, 4, 2, 2, 0, 4, 2, 4, 10, 4],
    //         vec![10, 10, 10, 2, 4, 2, 4, 0, 2, 2, 10, 0],
    //         vec![10, 8, 10, 4, 2, 4, 2, 2, 0, 4, 10, 2],
    //         vec![10, 10, 10, 0, 2, 2, 4, 2, 4, 0, 10, 2],
    //         vec![2, 2, 0, 10, 10, 10, 10, 10, 10, 10, 0, 10],
    //         vec![10, 10, 10, 2, 4, 2, 4, 0, 2, 2, 10, 0],
    //     ];

    //     for indices in (0..trees.len()).combinations(2) {
    //         let (i0, i1) = (indices[0], indices[1]);

    //         let t0 = Tree::from_newick(trees[i0]).unwrap();
    //         let t1 = Tree::from_newick(trees[i1]).unwrap();

    //         assert_eq!(t0.robinson_foulds_new(&t1).unwrap(), rfs[i0][i1])
    //     }
    // }

    // the reference distance matrix was computed with ete3
    // #[test]
    // fn compute_distance_matrix() {
    //     let tree = Tree::from_newick("((A:0.1,B:0.2)F:0.6,(C:0.3,D:0.4)E:0.5)G;").unwrap();
    //     let true_dists: HashMap<(String, String), f64> = HashMap::from_iter(vec![
    //         (("A".into(), "B".into()), 0.30000000000000004),
    //         (("A".into(), "C".into()), 1.5),
    //         (("A".into(), "D".into()), 1.6),
    //         (("B".into(), "C".into()), 1.6),
    //         (("B".into(), "D".into()), 1.7000000000000002),
    //         (("C".into(), "D".into()), 0.7),
    //     ]);

    //     let matrix = tree.distance_matrix().unwrap();

    //     for ((n1, n2), dist) in true_dists {
    //         assert!(
    //             (dist - matrix.get(&n1, &n2).unwrap()) <= f64::EPSILON,
    //             "d({n1},{n2}) want:{dist} got:{}",
    //             matrix.get(&n1, &n2).unwrap()
    //         )
    //     }
    // }
}

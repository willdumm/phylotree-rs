use tree_iterators_rs::prelude::*;
use std::slice::Iter;


use super::node::Node;
use super::tree::Tree;
use super::NodeId;


#[derive(Copy, Clone)]
pub struct NodeInTree<'a> {
    pub tree: &'a Tree,
    pub node: NodeId,
}


pub struct NodeInTreeIterator<'a> {
    pub nodeids: Iter<'a, usize>,
    pub tree: &'a Tree,
}

impl<'a> Iterator for NodeInTreeIterator<'a> {
    type Item = NodeInTree<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let nextid = self.nodeids.next();
        match nextid {
            Some(unwrapped_next) => {
                Some(NodeInTree { tree: self.tree, node: *unwrapped_next })
            },
            None => None,
        }
    }
}

impl<'a> NodeInTree<'a> {
    pub fn get_ref(&self) -> &'a Node {
        self.tree.get(&self.node).unwrap()
    }

    pub fn iter_children(&self) -> NodeInTreeIterator<'a> {
        NodeInTreeIterator {nodeids: self.get_ref().children.iter(), tree: self.tree}
    }

    pub fn postorder(&self) -> impl TreeIteratorMut<Item = NodeId> + 'a {
        self.dfs_postorder()
    }
}

impl<'a> OwnedTreeNode for NodeInTree<'a> {
    type OwnedValue = NodeId;
    type OwnedChildren = NodeInTreeIterator<'a>;

    fn get_value_and_children(self) -> (Self::OwnedValue, Option<Self::OwnedChildren>) {
        let res = Some(self.iter_children());
        (
            self.node,
            res
        )
    }
}


#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn compare_traversals() {

        let tree = Tree::from_newick("((3,4)2,(6,7)5)1;").unwrap();
        let root = tree.get_root().unwrap();

        let postorder = "3426751";
        let preorder = "1234567";
        let levelorder = "1253467";

        fn get_str(iter: &[usize], tree: &Tree) -> String {
            iter.iter()
                .map(|id| tree.get(id).unwrap().name.clone().unwrap())
                .collect()
        }

        assert_eq!(get_str(&tree.postorder(&root).unwrap(), &tree), postorder);
        assert_eq!(get_str(&tree.preorder(&root).unwrap(), &tree), preorder);
        assert_eq!(get_str(&tree.levelorder(&root).unwrap(), &tree), levelorder);
        let wnode = NodeInTree{tree: &tree, node: root};
        assert_eq!(get_str(&wnode.dfs_preorder().collect::<Vec<NodeId>>(), &tree), preorder);
        let wnode = NodeInTree{tree: &tree, node: root};
        assert_eq!(get_str(&wnode.dfs_postorder().collect::<Vec<NodeId>>(), &tree), postorder);
        let wnode = NodeInTree{tree: &tree, node: root};
        assert_eq!(get_str(&wnode.bfs().collect::<Vec<NodeId>>(), &tree), levelorder);
    }
}

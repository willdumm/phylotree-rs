use tree_iterators_rs::prelude::*;
use std::slice::Iter;


use super::node::Node;
use super::tree::Tree;
use super::NodeId;


struct NodeInTree<'a> {
    tree: &'a Tree,
    node: NodeId,
}


struct IterNodeIterator<'a> {
    nodeids: Iter<'a, usize>,
    tree: &'a Tree,
}

impl<'a> Iterator for IterNodeIterator<'a> {
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
    fn get_ref(&self) -> &'a Node {
        self.tree.get(&self.node).unwrap()
    }

    fn iter_children(&self) -> IterNodeIterator<'a> {
        IterNodeIterator {nodeids: self.get_ref().children.iter(), tree: self.tree}
    }

}

impl<'a> OwnedTreeNode for NodeInTree<'a> {
    type OwnedValue = NodeInTree<'a>;
    type OwnedChildren = IterNodeIterator<'a>;

    fn get_value_and_children(self) -> (Self::OwnedValue, Option<Self::OwnedChildren>) {
        let res = Some(self.iter_children());
        (
            self,
            res
        )
    }
}


#[cfg(test)]
// #[allow(clippy::excessive_precision)]
mod tests {

    use super::*;

    #[test]
    fn compare_traversals() {
        // Ancestors
        // {
        //     todo!();
        // }

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
        assert_eq!(get_str(&wnode.dfs_preorder().map(|n| n.node).collect::<Vec<NodeId>>(), &tree), preorder);
        let wnode = NodeInTree{tree: &tree, node: root};
        assert_eq!(get_str(&wnode.dfs_postorder().map(|n| n.node).collect::<Vec<NodeId>>(), &tree), postorder);
        let wnode = NodeInTree{tree: &tree, node: root};
        assert_eq!(get_str(&wnode.bfs().map(|n| n.node).collect::<Vec<NodeId>>(), &tree), levelorder);
    }
}

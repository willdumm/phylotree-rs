use criterion::{criterion_group, criterion_main, PlotConfiguration};
use criterion::{BenchmarkId, Criterion};

use phylotree::{distr::Distr::Uniform, generate_tree, tree::NodeInTree};


/// Measure how distance matrix extraction scales with tree size
fn dm_vs_treesize(c: &mut Criterion) {
    let plot_config = PlotConfiguration::default().summary_scale(criterion::AxisScale::Logarithmic);

    let mut group = c.benchmark_group("dm_vs_treesize");
    group.plot_config(plot_config);

    for size in [10, 20, 40, 100, 500, 1000, 2000, 5000, 10000].iter() {
        group.bench_with_input(BenchmarkId::new("PhyloDM", size), size, |bencher, size| {
            bencher.iter(|| {
                let tree = generate_tree(*size, true, Uniform).unwrap();
                tree.distance_matrix()
            })
        });
        if *size <= 1000 {
            group.bench_with_input(
                BenchmarkId::new("Recursive", size),
                size,
                |bencher, size| {
                    bencher.iter(|| {
                        let tree = generate_tree(*size, true, Uniform).unwrap();
                        tree.distance_matrix_recursive()
                    })
                },
            );
        }
    }

    group.finish();
}

/// Benchmark newick parsing
fn newick_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("newick_parsing");
    for size in [10, 20, 40, 100, 500, 1000, 2000, 5000, 10000, 20000].iter() {
        let newick = generate_tree(*size, true, Uniform)
            .unwrap()
            .to_newick()
            .unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(*size), size, |bencher, _| {
            bencher.iter(|| {
                let _ = phylotree::tree::Tree::from_newick(&newick);
            })
        });
    }
}

/// Compare iterators
fn recursive_postorder(c: &mut Criterion) {
    let mut group = c.benchmark_group("recursive_postorder");
    for size in [10, 20, 40, 100, 500, 1000, 2000, 5000, 10000, 20000].iter() {
        let tree = generate_tree(*size, true, Uniform)
            .unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(*size), size, |bencher, _| {
            bencher.iter(|| {
                for nid in tree.postorder(&tree.get_root().unwrap()) {
                    let _ = nid;
                }
            })
        });
    }
}

fn flat_postorder(c: &mut Criterion) {
    let mut group = c.benchmark_group("flat_postorder");
    for size in [10, 20, 40, 100, 500, 1000, 2000, 5000, 10000, 20000].iter() {
        let tree = generate_tree(*size, true, Uniform)
            .unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(*size), size, |bencher, _| {
            bencher.iter(|| {
                let rootnode = NodeInTree {tree: &tree, node: tree.get_root().unwrap()};
                for node in rootnode.postorder() {
                    let _ = node;
                }
            })
        });
    }
}
criterion_group!(benches, dm_vs_treesize, newick_parsing, recursive_postorder, flat_postorder);
criterion_main!(benches);

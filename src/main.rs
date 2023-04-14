use std::path::Path;
use clap::Parser;
use phylotree::*;

mod cli;

fn print_header() {
    println!("height\tnodes\ttips\trooted\tbinary\tsackin")
}

fn print_stats(path: &Path) {
    let mut tree = Tree::from_file(path).unwrap();
    println!(
        "{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t",
        tree.height(),
        tree.size(),
        tree.get_leaves().len(),
        tree.is_rooted(),
        tree.is_binary(),
        tree.sackin()
    )
}
fn main() {
    match cli::Args::parse().command {
        cli::Commands::Generate {
            tips,
            branch_lengths,
            output,
            trees,
        } => {
            if let Some(ntrees) = trees {
                // Create output directory if it's missing
                std::fs::create_dir_all(&output).unwrap();

                for i in 1..=ntrees {
                    let output = output.join(format!("{i}_{tips}_tips.nwk"));
                    let random = generate_tree(tips, branch_lengths);
                    random.to_file(&output).unwrap()
                }
            } else {
                let random = generate_tree(tips, branch_lengths);
                random.to_file(&output).unwrap()
            }
        }
        cli::Commands::Stats { trees } => {
            print_header();
            for tree in trees {
                print_stats(&tree)
            }
        }
    }
}

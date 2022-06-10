mod hashtree;

fn main() -> std::io::Result<()> {
    let old_tree = hashtree::load_tree("./hashtree.json")?;
    let new_tree = hashtree::hash_tree("./src")?;
    // hashtree::save_tree("./hashtree.json", &new_tree)?;

    // println!("{:#?}", old_tree);
    // println!("{:#?}", new_tree);
    println!("{:#?}", hashtree::diff(&old_tree, &new_tree));

    Ok(())
}

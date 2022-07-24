#![allow(dead_code)]
use std::io::Write;

use anyhow::Result;
use bbup_rust::{fs, model::ExcludeList};
fn test_delta_stuff() -> Result<()> {
    fs::make_clean_dir("./prova")?;
    fs::create_file("./prova/text.txt")?;
    fs::create_file("./prova/ciao/test.txt")?;
    fs::create_file("./prova/pippo/test.txt")?;
    fs::create_dir("./prova/boh")?;
    fs::create_symlink("./prova/boh/link", "../text.txt")?;
    let mut old_tree =
        bbup_rust::fstree::generate_fstree("./prova", &ExcludeList::from(&Vec::new())?)?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    std::fs::write("./prova/ciao/test.txt", b"ciao")?;
    std::fs::write("./prova/pippo/test.txt", b"")?;
    fs::create_file("./prova/text2.txt")?;
    fs::remove_dir_all("./prova/boh")?;
    let mid_tree = bbup_rust::fstree::generate_fstree("./prova", &ExcludeList::from(&Vec::new())?)?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    std::fs::write("./prova/text2.txt", b"ehi")?;
    fs::remove_file("./prova/text.txt")?;
    fs::create_dir("./prova/ehi")?;
    fs::create_symlink("./prova/ehi/link", "../text2.txt")?;
    let new_tree = bbup_rust::fstree::generate_fstree("./prova", &ExcludeList::from(&Vec::new())?)?;
    fs::remove_dir_all("./prova")?;

    let delta1 = bbup_rust::fstree::get_delta(&old_tree, &mid_tree);
    let mut delta2 = bbup_rust::fstree::get_delta(&mid_tree, &new_tree);
    let delta_tot = bbup_rust::fstree::get_delta(&old_tree, &new_tree);
    println!("old tree:\n{}\n", old_tree);
    println!("mid tree:\n{}\n", mid_tree);
    println!("new tree:\n{}\n", new_tree);
    println!("delta1:\n{}\n", delta1);
    println!("delta2:\n{}\n", delta2);
    old_tree = old_tree.try_apply_delta(&delta1)?;
    assert_eq!(old_tree, mid_tree);

    delta2.merge_prec(&delta1)?;
    println!("updated delta2:\n{}\n", delta2);
    assert_eq!(delta_tot, delta2);

    Ok(())
}

fn test_conflict_stuff() -> Result<()> {
    use bbup_rust::fstree::{check_for_conflicts, generate_fstree, get_delta};

    let eel = &ExcludeList::from(&Vec::new())?;
    fs::make_clean_dir("./prova/ehi")?;
    let base_tree = generate_fstree("./prova", eel)?;

    let mut file = fs::create_file("./prova/text.txt")?;
    file.write_all(b"ciao")?;
    fs::create_file("./prova/sa/sa/ciaoo.txt")?;
    let mut file = fs::create_file("./prova/ehi/sa/ehi.txt")?;
    file.write_all(b"ehi")?;
    let tree1 = generate_fstree("./prova", eel)?;

    let mut file = fs::create_file("./prova/text.txt")?;
    file.write_all(b"ehi :)")?;
    fs::create_file("./prova/ehi/sa/ehi.txt")?;
    let tree2 = generate_fstree("./prova", eel)?;
    fs::remove_dir_all("./prova")?;

    let delta1 = get_delta(&base_tree, &tree1);
    let delta2 = get_delta(&base_tree, &tree2);

    println!("{}", check_for_conflicts(&delta1, &delta2).unwrap());

    Ok(())
}

fn main() -> Result<()> {
    // test_delta_stuff()
    // test_conflict_stuff()
    // dbg!(fs::make_clean_dir("./prova")?);
    // dbg!(fs::create_symlink("./prova/sa/link", "../test.txt")?);
    // println!("{:?}", fs::read_link("./prova/sa/link")?);
    // fs::create_file("./prova/test.txt")?;
    // dbg!(fs::rename("./prova/sa/link", "./prova/se/new-link")?);
    // println!("{:?}", fs::read_link("./prova/se/new-link")?);
    // fs::remove_dir_all("./prova")?;
    Ok(())
}

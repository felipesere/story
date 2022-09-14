use std::env;

fn main() {
    let src = env::var("CARGO_MANIFEST_DIR").unwrap();

    let dirty = built::util::get_repo_description(src.as_ref())
        .ok()
        .flatten()
        .map_or(false, |(_, dirty)| dirty);

    let sha = built::util::get_repo_head(src.as_ref())
        .ok()
        .flatten()
        .map(|(_, mut sha)| {
            sha.truncate(7);
            sha
        })
        .map(|sha| if dirty { format!("{}-dirty", sha) } else { sha })
        .or_else(|| std::env::var("GITHUB_SHA").ok())
        .unwrap_or_else(|| "none".into());

    let pkg_version = env!("CARGO_PKG_VERSION");
    println!(r#"cargo:rustc-env=FANCY_VERSION={} ({})"#, pkg_version, sha)
}

use std::env;

fn main() {
    let src = env::var("CARGO_MANIFEST_DIR").unwrap();
    let sha = built::util::get_repo_head(&src.as_ref())
        .ok()
        .flatten()
        .map(|(_, sha)| sha)
        .or_else(|| std::env::var("GITHUB_SHA").ok())
        .unwrap_or("none".into());

    let pkg_version = env!("CARGO_PKG_VERSION");
    println!(r#"cargo:rustc-env=FANCY_VERSION="{} ({})""#, pkg_version, sha)
}
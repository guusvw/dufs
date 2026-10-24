use std::process::Command;

/// Exposes the commit being built as `DUFS_COMMIT` (shown by `dufs_build_info`):
/// the `DUFS_COMMIT` environment variable if set, else the git checkout's HEAD,
/// else `unknown` (e.g. a crates.io or Docker build without `.git`).
fn main() {
    println!("cargo:rerun-if-env-changed=DUFS_COMMIT");
    let commit = std::env::var("DUFS_COMMIT")
        .ok()
        .map(|v| v.trim().chars().take(12).collect::<String>())
        .filter(|v| !v.is_empty())
        .or_else(git_commit)
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=DUFS_COMMIT={commit}");
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).ok())
        .flatten()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn git_commit() -> Option<String> {
    let commit = git(&["rev-parse", "--short=12", "HEAD"])?;
    // Rebuild when HEAD moves: when it is switched, and when the branch it
    // points to gets a new commit.
    if let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head}");
    }
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git(&["rev-parse", "--git-path", &branch]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    Some(commit)
}

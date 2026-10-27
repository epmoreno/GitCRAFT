//! Deterministic temporary repositories for integration tests.
//!
//! Every repo is isolated from the developer's git config, uses a fixed
//! identity and a fake clock that advances one minute per commit, so commit
//! ids are identical across runs and machines.
//!
//! Helpers panic on failure: they are meant for tests only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tempfile::TempDir;

use crate::{CancellationToken, Git, GitCommand};

pub const AUTHOR_NAME: &str = "GitCRAFT Test";
pub const AUTHOR_EMAIL: &str = "test@gitcraft.invalid";
/// 2023-11-14T22:13:20Z. The fake clock starts here.
pub const EPOCH: u64 = 1_700_000_000;

pub struct TestRepo {
    dir: TempDir,
    git: Git,
    clock: AtomicU64,
}

impl TestRepo {
    /// An empty repository whose initial branch is `main`.
    pub async fn new() -> Self {
        let repo = Self {
            dir: tempfile::tempdir().expect("create temp dir"),
            git: Git::discover().expect("git must be installed to run tests"),
            clock: AtomicU64::new(0),
        };
        repo.run(&["init", "--initial-branch=main"]).await;
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn git(&self) -> &Git {
        &self.git
    }

    /// An isolated, deterministic read command in this repo.
    pub fn read(&self) -> GitCommand {
        self.isolate(self.git.read(self.path()))
    }

    /// An isolated, deterministic write command in this repo.
    pub fn write(&self) -> GitCommand {
        self.isolate(self.git.write(self.path()))
    }

    fn isolate(&self, cmd: GitCommand) -> GitCommand {
        let date = format!("@{} +0000", EPOCH + self.clock.load(Ordering::SeqCst) * 60);
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", AUTHOR_NAME)
            .env("GIT_AUTHOR_EMAIL", AUTHOR_EMAIL)
            .env("GIT_COMMITTER_NAME", AUTHOR_NAME)
            .env("GIT_COMMITTER_EMAIL", AUTHOR_EMAIL)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
    }

    /// Runs a write command and returns its trimmed stdout. Panics on failure.
    pub async fn run(&self, args: &[&str]) -> String {
        let out = self
            .write()
            .args(args)
            .output(&CancellationToken::new())
            .await
            .unwrap_or_else(|e| panic!("git {args:?} failed: {e}"));
        out.stdout_lossy().trim_end().to_owned()
    }

    pub fn write_file(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.path().join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(&path, contents).expect("write file");
        path
    }

    /// Stages everything and commits. Returns the new commit id.
    pub async fn commit_all(&self, message: &str) -> String {
        self.tick();
        self.run(&["add", "--all"]).await;
        self.run(&["commit", "--allow-empty", "--quiet", "-m", message])
            .await;
        self.head().await
    }

    /// Writes `relative`, stages everything and commits. Returns the commit id.
    pub async fn commit_file(&self, relative: &str, contents: &str, message: &str) -> String {
        self.write_file(relative, contents);
        self.commit_all(message).await
    }

    pub async fn head(&self) -> String {
        self.run(&["rev-parse", "HEAD"]).await
    }

    pub async fn rev_parse(&self, rev: &str) -> String {
        self.run(&["rev-parse", "--verify", rev]).await
    }

    pub async fn create_branch(&self, name: &str) {
        self.run(&["branch", "--", name]).await;
    }

    pub async fn checkout(&self, name: &str) {
        self.run(&["switch", "--quiet", name]).await;
    }

    /// Creates a merge commit (never fast-forward). Returns its id.
    pub async fn merge(&self, branch: &str, message: &str) -> String {
        self.tick();
        self.run(&["merge", "--no-ff", "--quiet", "-m", message, branch])
            .await;
        self.head().await
    }

    pub async fn tag_lightweight(&self, name: &str) {
        self.run(&["tag", name]).await;
    }

    pub async fn tag_annotated(&self, name: &str, message: &str) {
        self.tick();
        self.run(&["tag", "-a", "-m", message, name]).await;
    }

    fn tick(&self) {
        self.clock.fetch_add(1, Ordering::SeqCst);
    }

    /// A small repository exercising branches, a merge and both tag kinds:
    ///
    /// ```text
    /// * E        (HEAD -> main, tag: v2.0)
    /// *   M      Merge branch 'feature'
    /// |\
    /// | * D      (feature)
    /// | * C
    /// * | B      (tag: v1.0, annotated)
    /// |/
    /// | * F      (topic, unmerged)
    /// |/
    /// * A        root
    /// ```
    ///
    /// Returns the repo and a map from the letters above to commit ids.
    pub async fn sample_history() -> (Self, BTreeMap<&'static str, String>) {
        let repo = Self::new().await;
        let mut ids = BTreeMap::new();

        ids.insert("A", repo.commit_file("README.md", "# Sample\n", "A").await);
        repo.create_branch("topic").await;
        repo.create_branch("feature").await;

        repo.checkout("topic").await;
        ids.insert("F", repo.commit_file("topic.txt", "topic\n", "F").await);

        repo.checkout("feature").await;
        ids.insert("C", repo.commit_file("feature.txt", "c\n", "C").await);
        ids.insert("D", repo.commit_file("feature.txt", "c\nd\n", "D").await);

        repo.checkout("main").await;
        ids.insert("B", repo.commit_file("main.txt", "b\n", "B").await);
        repo.tag_annotated("v1.0", "Release 1.0").await;
        ids.insert("M", repo.merge("feature", "Merge branch 'feature'").await);
        ids.insert("E", repo.commit_file("main.txt", "b\ne\n", "E").await);
        repo.tag_lightweight("v2.0").await;

        (repo, ids)
    }
}

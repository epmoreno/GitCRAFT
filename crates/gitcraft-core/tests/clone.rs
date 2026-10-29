//! Cloning from local repositories (`file://` URLs and paths), never from a
//! real server.

use gitcraft_core::ops::clone::{Destination, check_destination, clone_repository};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind, Git};

fn git() -> Git {
    Git::discover().expect("git must be installed to run the tests")
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

/// A bare remote with two commits on main and a feature branch.
async fn remote() -> (TestRepo, tempfile::TempDir) {
    let repo = TestRepo::new().await;
    repo.commit_file("README.md", "hola\n", "uno").await;
    repo.commit_file("lista.txt", "a\n", "dos").await;
    repo.create_branch("feature").await;
    let bare = repo.add_bare_remote("origin").await;
    (repo, bare)
}

fn file_url(path: &std::path::Path) -> String {
    format!("file://{}", path.display())
}

#[tokio::test]
async fn clones_a_file_url_into_a_new_folder() {
    let (source, bare) = remote().await;
    let parent = tempfile::tempdir().expect("tempdir");
    assert_eq!(check_destination(parent.path(), "copia"), Destination::Free);

    let mut progress = Vec::new();
    let path = clone_repository(
        &git(),
        &file_url(bare.path()),
        parent.path(),
        "copia",
        &c(),
        |line| progress.push(line.to_owned()),
    )
    .await
    .expect("clone");
    assert_eq!(path, parent.path().join("copia"));
    assert!(!progress.is_empty(), "git reported progress");

    let clone = |args: &[&str]| {
        let path = path.clone();
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        async move {
            git()
                .read(&path)
                .args(args)
                .output(&c())
                .await
                .expect("git")
                .stdout_lossy()
                .trim()
                .to_owned()
        }
    };
    assert_eq!(clone(&["rev-parse", "HEAD"]).await, source.head().await);
    assert_eq!(
        clone(&["rev-parse", "--abbrev-ref", "main@{upstream}"]).await,
        "origin/main"
    );
    assert!(clone(&["branch", "-r"]).await.contains("origin/feature"));
    assert_eq!(
        std::fs::read_to_string(path.join("lista.txt")).expect("read"),
        "a\n"
    );
}

#[tokio::test]
async fn clones_a_plain_path_into_an_empty_folder() {
    let (_source, bare) = remote().await;
    let parent = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(parent.path().join("vacia")).expect("mkdir");
    assert_eq!(
        check_destination(parent.path(), "vacia"),
        Destination::EmptyFolder
    );
    let path = clone_repository(
        &git(),
        bare.path().to_str().expect("utf-8"),
        parent.path(),
        "vacia",
        &c(),
        |_| {},
    )
    .await
    .expect("clone");
    assert!(path.join(".git").is_dir());
}

#[tokio::test]
async fn never_clones_into_a_folder_with_files() {
    let (_source, bare) = remote().await;
    let parent = tempfile::tempdir().expect("tempdir");
    let busy = parent.path().join("ocupada");
    std::fs::create_dir(&busy).expect("mkdir");
    std::fs::write(busy.join("mio.txt"), "no tocar\n").expect("write");
    std::fs::write(parent.path().join("archivo"), "x").expect("write");

    assert_eq!(
        check_destination(parent.path(), "ocupada"),
        Destination::NotEmpty
    );
    assert_eq!(
        check_destination(parent.path(), "archivo"),
        Destination::NotEmpty
    );
    let err = clone_repository(
        &git(),
        &file_url(bare.path()),
        parent.path(),
        "ocupada",
        &c(),
        |_| {},
    )
    .await
    .expect_err("not empty");
    assert_eq!(err.kind(), ErrorKind::DestinationNotEmpty);
    assert_eq!(
        std::fs::read_to_string(busy.join("mio.txt")).expect("read"),
        "no tocar\n"
    );
    assert!(!busy.join(".git").exists());
}

#[tokio::test]
async fn refuses_bad_names_urls_and_parents() {
    let (_source, bare) = remote().await;
    let parent = tempfile::tempdir().expect("tempdir");
    for name in ["", ".", "..", "a/b"] {
        assert_eq!(
            check_destination(parent.path(), name),
            Destination::InvalidName,
            "{name:?}"
        );
    }
    assert_eq!(
        check_destination(&parent.path().join("no-existe"), "x"),
        Destination::ParentMissing
    );
    for url in ["", "--upload-pack=touch /tmp/pwned", "-x"] {
        assert!(
            clone_repository(&git(), url, parent.path(), "x", &c(), |_| {})
                .await
                .is_err(),
            "{url:?}"
        );
    }
    assert!(!parent.path().join("x").exists());

    let missing = file_url(&bare.path().join("no-existe.git"));
    let err = clone_repository(&git(), &missing, parent.path(), "y", &c(), |_| {})
        .await
        .expect_err("missing remote");
    assert_eq!(err.kind(), ErrorKind::RemoteUnreachable);
    assert!(
        !parent.path().join("y").exists(),
        "git cleans up after itself"
    );
}

#[tokio::test]
async fn can_be_cancelled() {
    let (_source, bare) = remote().await;
    let parent = tempfile::tempdir().expect("tempdir");
    let cancel = c();
    cancel.cancel();
    let err = clone_repository(
        &git(),
        &file_url(bare.path()),
        parent.path(),
        "z",
        &cancel,
        |_| {},
    )
    .await
    .expect_err("cancelled");
    assert_eq!(err.kind(), ErrorKind::Cancelled);
    assert!(!parent.path().join("z").join(".git").exists());
}

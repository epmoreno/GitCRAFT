//! Tags: create (lightweight, annotated), push, delete locally and on a
//! remote, against real repositories and a local bare remote.

use gitcraft_core::ops::tag::{
    RemoteTag, TagNameCheck, TagPushOutcome, check_tag_name, create_tag, delete_remote_tag,
    delete_tag, push_tag, remote_tags, tag_info,
};
use gitcraft_core::testing::{AUTHOR_NAME, TestRepo};
use gitcraft_core::{CancellationToken, ErrorKind};

fn c() -> CancellationToken {
    CancellationToken::new()
}

async fn check(repo: &TestRepo, name: &str) -> TagNameCheck {
    check_tag_name(repo.git(), repo.path(), name, &c())
        .await
        .expect("check")
}

#[tokio::test]
async fn names() {
    let (repo, _) = TestRepo::sample_history().await;
    assert_eq!(check(&repo, "v3.0").await, TagNameCheck::Valid);
    assert_eq!(check(&repo, "release/3.0").await, TagNameCheck::Valid);
    for bad in ["", "-x", "a b", "a..b", "v3/", "x~1", "x:y"] {
        assert_eq!(check(&repo, bad).await, TagNameCheck::Invalid, "{bad:?}");
    }
    assert_eq!(check(&repo, "v1.0").await, TagNameCheck::Exists);
}

#[tokio::test]
async fn lightweight_and_annotated() {
    let (repo, ids) = TestRepo::sample_history().await;
    let light = create_tag(repo.git(), repo.path(), &ids["C"], "light", None, &c())
        .await
        .expect("lightweight");
    assert!(!light.annotated);
    assert_eq!(
        (light.oid.as_str(), light.commit.as_str()),
        (ids["C"].as_str(), ids["C"].as_str())
    );

    let note = create_tag(
        repo.git(),
        repo.path(),
        &ids["D"],
        "release/2.1",
        Some("Release 2.1\n\nWith notes.\n"),
        &c(),
    )
    .await
    .expect("annotated");
    assert!(note.annotated);
    assert_eq!(note.commit, ids["D"]);
    assert_ne!(note.oid, ids["D"], "an annotated tag is its own object");
    let object = repo.run(&["cat-file", "-p", "release/2.1"]).await;
    assert!(
        object.contains(&format!("tagger {AUTHOR_NAME}")),
        "{object}"
    );
    assert!(object.ends_with("Release 2.1\n\nWith notes."), "{object}");
}

#[tokio::test]
async fn existing_or_invalid_names_are_refused_and_nothing_moves() {
    let (repo, ids) = TestRepo::sample_history().await;
    let before = repo.rev_parse("v1.0").await;
    let err = create_tag(repo.git(), repo.path(), &ids["E"], "v1.0", None, &c())
        .await
        .expect_err("exists");
    assert_eq!(err.kind(), ErrorKind::TagExists);
    assert_eq!(repo.rev_parse("v1.0").await, before);
    let err = create_tag(repo.git(), repo.path(), &ids["E"], "-x", None, &c())
        .await
        .expect_err("invalid");
    assert_eq!(err.kind(), ErrorKind::InvalidTagName);
    let err = create_tag(
        repo.git(),
        repo.path(),
        &ids["E"],
        "empty",
        Some("  \n"),
        &c(),
    )
    .await
    .expect_err("empty message");
    assert_eq!(err.kind(), ErrorKind::Unknown);
    assert_eq!(check(&repo, "empty").await, TagNameCheck::Valid);
}

#[tokio::test]
async fn deleting_locally_needs_the_tag_as_it_was_shown() {
    let (repo, ids) = TestRepo::sample_history().await;
    let info = tag_info(repo.git(), repo.path(), "v2.0", &c())
        .await
        .expect("info")
        .expect("exists");
    // Moved elsewhere since it was shown: nothing is deleted.
    let err = delete_tag(repo.git(), repo.path(), "v2.0", &ids["A"], &c())
        .await
        .expect_err("stale");
    assert_eq!(err.kind(), ErrorKind::StaleDiff);
    delete_tag(repo.git(), repo.path(), "v2.0", &info.oid, &c())
        .await
        .expect("delete");
    assert_eq!(check(&repo, "v2.0").await, TagNameCheck::Valid);
}

#[tokio::test]
async fn push_then_delete_on_the_remote_only_when_asked() {
    let (repo, ids) = TestRepo::sample_history().await;
    let _bare = repo.add_bare_remote("origin").await;
    let tag = create_tag(repo.git(), repo.path(), &ids["E"], "v3.0", None, &c())
        .await
        .expect("create");
    let on = |repo: &TestRepo| {
        let repo_path = repo.path().to_path_buf();
        let git = repo.git().clone();
        async move {
            remote_tags(&git, &repo_path, "v3.0", &c())
                .await
                .expect("ls-remote")
        }
    };
    assert_eq!(on(&repo).await, vec![]);

    assert_eq!(
        push_tag(repo.git(), repo.path(), "origin", "v3.0", &c())
            .await
            .expect("push"),
        TagPushOutcome::Done
    );
    assert_eq!(
        on(&repo).await,
        vec![RemoteTag {
            remote: "origin".into(),
            oid: tag.oid.clone(),
            same: true
        }]
    );

    // Deleting locally never touches the remote.
    delete_tag(repo.git(), repo.path(), "v3.0", &tag.oid, &c())
        .await
        .expect("delete local");
    assert_eq!(on(&repo).await.len(), 1);

    // Stale lease (the remote's tag is not what we say): refused.
    let refused = delete_remote_tag(repo.git(), repo.path(), "origin", "v3.0", &ids["A"], &c())
        .await
        .expect("push");
    assert!(
        matches!(refused, TagPushOutcome::Rejected { .. }),
        "{refused:?}"
    );
    assert_eq!(on(&repo).await.len(), 1);

    assert_eq!(
        delete_remote_tag(repo.git(), repo.path(), "origin", "v3.0", &tag.oid, &c())
            .await
            .expect("delete remote"),
        TagPushOutcome::Done
    );
    assert_eq!(on(&repo).await, vec![]);
}

#[tokio::test]
async fn a_different_tag_on_the_remote_is_not_replaced() {
    let (repo, ids) = TestRepo::sample_history().await;
    let _bare = repo.add_bare_remote("origin").await;
    create_tag(repo.git(), repo.path(), &ids["A"], "v9", None, &c())
        .await
        .expect("create");
    push_tag(repo.git(), repo.path(), "origin", "v9", &c())
        .await
        .expect("push");
    let remote_oid = repo.rev_parse("v9").await;
    repo.run(&["tag", "-d", "v9"]).await;
    create_tag(repo.git(), repo.path(), &ids["E"], "v9", None, &c())
        .await
        .expect("create again elsewhere");
    let outcome = push_tag(repo.git(), repo.path(), "origin", "v9", &c())
        .await
        .expect("push");
    assert!(
        matches!(outcome, TagPushOutcome::Rejected { .. }),
        "{outcome:?}"
    );
    let there = remote_tags(repo.git(), repo.path(), "v9", &c())
        .await
        .expect("ls-remote");
    assert_eq!(there[0].oid, remote_oid);
    assert!(!there[0].same);
    // Unknown remotes are refused.
    assert!(
        push_tag(repo.git(), repo.path(), "--all", "v9", &c())
            .await
            .is_err()
    );
}

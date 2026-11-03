//! Git configuration, always against a temporary global file: these tests
//! must never touch the real `~/.gitconfig`.

use gitcraft_core::ops::config::{
    self, ConfigChange, ConfigTarget, PullStrategy, global_defaults, pull_strategy_changes,
};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, Git};

struct Isolated {
    _dir: tempfile::TempDir,
    global: std::path::PathBuf,
    git: Git,
}

fn isolated() -> Isolated {
    let dir = tempfile::tempdir().expect("tempdir");
    let global = dir.path().join("gitconfig");
    let git = Git::discover()
        .expect("git")
        .with_env("GIT_CONFIG_GLOBAL", &global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    Isolated {
        _dir: dir,
        global,
        git,
    }
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

fn set(key: &str, value: Option<&str>) -> ConfigChange {
    ConfigChange {
        key: key.into(),
        value: value.map(Into::into),
    }
}

#[tokio::test]
async fn writes_reads_and_removes_global_values_literally() {
    let env = isolated();
    let global = ConfigTarget::Global;
    assert_eq!(
        config::get(&env.git, &global, "user.name", &c())
            .await
            .expect("get"),
        None
    );
    // A value that looks like an option, and one with spaces and accents.
    config::apply(
        &env.git,
        &global,
        &[
            set("user.name", Some("-n Ana Pérez")),
            set("user.email", Some("--ana@example.com")),
        ],
        &c(),
    )
    .await
    .expect("apply");
    assert_eq!(
        config::get(&env.git, &global, "user.name", &c())
            .await
            .expect("get"),
        Some("-n Ana Pérez".into())
    );
    let file = std::fs::read_to_string(&env.global).expect("read");
    assert!(file.contains("email = --ana@example.com"), "{file}");

    // Removing something that is not set is fine.
    config::apply(
        &env.git,
        &global,
        &[set("user.email", None), set("init.defaultBranch", None)],
        &c(),
    )
    .await
    .expect("unset");
    assert_eq!(
        config::get(&env.git, &global, "user.email", &c())
            .await
            .expect("get"),
        None
    );
}

#[tokio::test]
async fn reports_where_the_effective_value_comes_from() {
    let env = isolated();
    let repo = TestRepo::new().await;
    let local = ConfigTarget::Repo(repo.path().to_owned());
    config::apply(
        &env.git,
        &ConfigTarget::Global,
        &[set("user.name", Some("Global"))],
        &c(),
    )
    .await
    .expect("global");
    let value = config::effective(&env.git, repo.path(), "user.name", &c())
        .await
        .expect("effective")
        .expect("set");
    assert_eq!(
        (value.value.as_str(), value.scope.as_str()),
        ("Global", "global")
    );

    config::apply(&env.git, &local, &[set("user.name", Some("Repo"))], &c())
        .await
        .expect("local");
    let value = config::effective(&env.git, repo.path(), "user.name", &c())
        .await
        .expect("effective")
        .expect("set");
    assert_eq!(
        (value.value.as_str(), value.scope.as_str()),
        ("Repo", "local")
    );
    assert_eq!(
        config::get(&env.git, &ConfigTarget::Global, "user.name", &c())
            .await
            .expect("get"),
        Some("Global".into()),
        "the repository value does not touch the global one"
    );
}

#[tokio::test]
async fn sets_the_default_pull_strategy_with_one_deciding_key() {
    let env = isolated();
    let global = ConfigTarget::Global;
    assert_eq!(
        global_defaults(&env.git, &c())
            .await
            .expect("read")
            .pull_strategy,
        None
    );
    for strategy in [
        PullStrategy::FastForwardOnly,
        PullStrategy::Rebase,
        PullStrategy::Merge,
    ] {
        config::apply(
            &env.git,
            &global,
            &pull_strategy_changes(Some(strategy)),
            &c(),
        )
        .await
        .expect("apply");
        let defaults = global_defaults(&env.git, &c()).await.expect("read");
        assert_eq!(defaults.pull_strategy, Some(strategy));
    }
    let defaults = global_defaults(&env.git, &c()).await.expect("read");
    assert_eq!(defaults.pull_rebase.as_deref(), Some("false"));
    assert_eq!(defaults.pull_ff, None, "ff-only was removed");

    config::apply(&env.git, &global, &pull_strategy_changes(None), &c())
        .await
        .expect("unset");
    let defaults = global_defaults(&env.git, &c()).await.expect("read");
    assert_eq!((defaults.pull_rebase, defaults.pull_ff), (None, None));
}

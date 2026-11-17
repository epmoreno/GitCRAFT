//! Interactive rebase, read-only part (task 13.1): the range, the refusals
//! and the prediction of a plan, compared with what `git rebase -i` does.

use gitcraft_core::CancellationToken;
use gitcraft_core::ops::interactive_rebase::{
    InteractivePreview, InteractiveRefusal, PlanPreview, PlanProblem, PlanStep, RebasePlan,
    StepAction, interactive_rebase_preview, simulate_plan, validate_plan,
};
use gitcraft_core::ops::merge_rebase::MoveBlock;
use gitcraft_core::testing::TestRepo;

fn token() -> CancellationToken {
    CancellationToken::new()
}

async fn preview(repo: &TestRepo, from: &str) -> InteractivePreview {
    interactive_rebase_preview(repo.git(), repo.path(), from, &token())
        .await
        .expect("preview")
}

fn step(oid: &str, action: StepAction) -> PlanStep {
    PlanStep {
        oid: oid.to_owned(),
        action,
        message: None,
    }
}

fn with_message(oid: &str, action: StepAction, message: &str) -> PlanStep {
    PlanStep {
        message: Some(message.to_owned()),
        ..step(oid, action)
    }
}

fn plan(p: &InteractivePreview, steps: Vec<PlanStep>) -> RebasePlan {
    RebasePlan {
        from: p.from.clone(),
        steps,
        expected: p.fingerprint.clone(),
    }
}

async fn simulate(repo: &TestRepo, plan: &RebasePlan) -> PlanPreview {
    simulate_plan(repo.git(), repo.path(), plan, &token())
        .await
        .expect("simulate")
}

/// Everything that must not change: refs, HEAD, index and files.
async fn state(repo: &TestRepo) -> String {
    let refs = repo
        .run(&["for-each-ref", "--format=%(refname) %(objectname)"])
        .await;
    let head = repo.run(&["rev-parse", "HEAD"]).await;
    let status = repo
        .run(&["status", "--porcelain=v2", "--untracked-files=all"])
        .await;
    format!("{refs}\n{head}\n{status}")
}

/// Runs the plan with `git rebase -i` itself (no new messages: those come in
/// 13.2). Returns whether it finished.
async fn real_rebase(repo: &TestRepo, p: &InteractivePreview, steps: &[PlanStep]) -> bool {
    let mut todo = String::new();
    for s in steps {
        let command = match s.action {
            StepAction::Pick | StepAction::Reword => "pick",
            StepAction::Squash => "fixup",
            StepAction::Drop => "drop",
        };
        todo.push_str(&format!("{command} {}\n", s.oid));
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("todo");
    std::fs::write(&file, todo).expect("write todo");
    let path = file.to_string_lossy().replace('\\', "/");
    let mut args = vec!["rebase", "-i", "--empty=drop"];
    let base;
    match &p.base {
        Some(b) => {
            base = b.clone();
            args.push(&base);
        }
        None => args.push("--root"),
    }
    let out = repo
        .write()
        .args(args)
        .env("GIT_SEQUENCE_EDITOR", format!("cp '{path}'"))
        .env("GIT_EDITOR", "false")
        .accept_exit_code(1)
        .output(&token())
        .await
        .expect("rebase -i");
    out.exit_code == Some(0)
}

async fn tree(repo: &TestRepo) -> String {
    repo.run(&["rev-parse", "HEAD^{tree}"])
        .await
        .trim()
        .to_owned()
}

/// base, then A, B and C, each adding its own file, by different authors.
async fn three(repo: &TestRepo) -> (String, String, String, String) {
    let base = repo.commit_file("base.txt", "base\n", "Base").await;
    repo.write_file("a.txt", "a\n");
    repo.run(&["add", "--all"]).await;
    let a = repo
        .commit_with(&[
            "--author=Ana Pérez <ana@example.invalid>",
            "-m",
            "Añade a",
            "-m",
            "Con un cuerpo.\n# Y una línea con almohadilla.",
        ])
        .await;
    let b = repo.commit_file("b.txt", "b\n", "Añade b").await;
    let c = repo.commit_file("c.txt", "c\n", "Añade c").await;
    (base, a, b, c)
}

#[tokio::test]
async fn lists_the_range_oldest_first_with_whole_messages() {
    let repo = TestRepo::new().await;
    let (base, a, b, c) = three(&repo).await;
    let p = preview(&repo, &a[..8]).await;
    assert_eq!(p.refused, None);
    assert_eq!(p.branch.as_deref(), Some("main"));
    assert_eq!(p.from, a);
    assert_eq!(p.base.as_deref(), Some(base.as_str()));
    let oids: Vec<_> = p.commits.iter().map(|c| c.commit.oid.as_str()).collect();
    assert_eq!(oids, [a.as_str(), b.as_str(), c.as_str()]);
    let first = &p.commits[0];
    assert_eq!(first.commit.subject, "Añade a");
    assert_eq!(
        first.message,
        "Añade a\n\nCon un cuerpo.\n# Y una línea con almohadilla."
    );
    assert_eq!(first.author, "Ana Pérez");
    assert!(first.author_time > 0);
    assert!(p.commits.iter().all(|c| !c.published));
    assert!(p.published_in.is_empty());
    assert_eq!(p.blocked, None);
    assert!(p.messages && p.exact, "the CI's git is ≥ 2.40");

    // From the root commit: the whole branch, onto nothing.
    let p = preview(&repo, &base).await;
    assert_eq!(p.refused, None);
    assert_eq!(p.base, None);
    assert_eq!(p.commits.len(), 4);
}

#[tokio::test]
async fn refuses_without_touching_anything() {
    let repo = TestRepo::new().await;
    let (_, a, _, c) = three(&repo).await;

    // Not in the current branch.
    repo.run(&["switch", "--quiet", "-c", "side", &a]).await;
    let side = repo.commit_file("side.txt", "s\n", "Lado").await;
    repo.checkout("main").await;
    assert_eq!(
        preview(&repo, &side).await.refused,
        Some(InteractiveRefusal::NotInBranch)
    );

    // Detached HEAD.
    repo.run(&["switch", "--quiet", "--detach", &c]).await;
    assert_eq!(
        preview(&repo, &a).await.refused,
        Some(InteractiveRefusal::Detached)
    );
    repo.checkout("main").await;

    // Merges in the range, and a merge as the chosen commit.
    let merge = repo.merge("side", "Merge side").await;
    assert_eq!(
        preview(&repo, &a).await.refused,
        Some(InteractiveRefusal::MergesInRange)
    );
    assert_eq!(
        preview(&repo, &merge).await.refused,
        Some(InteractiveRefusal::MergesInRange)
    );

    // Something in progress (a merge stopped by a conflict).
    let repo = TestRepo::new().await;
    let (_, a, _, _) = three(&repo).await;
    repo.run(&["switch", "--quiet", "-c", "other"]).await;
    repo.commit_file("c.txt", "otro\n", "Otro c").await;
    repo.checkout("main").await;
    repo.commit_file("c.txt", "mío\n", "Mi c").await;
    repo.write()
        .args(["merge", "--no-edit", "other"])
        .accept_exit_code(1)
        .output(&token())
        .await
        .expect("merge");
    let before = state(&repo).await;
    let p = preview(&repo, &a).await;
    assert_eq!(p.refused, Some(InteractiveRefusal::InProgress));
    assert!(p.commits.is_empty());
    assert_eq!(state(&repo).await, before);

    // A plan for a refused preview is refused too.
    let simulated = simulate(&repo, &plan(&p, vec![])).await;
    assert_eq!(simulated.problem, Some(PlanProblem::Refused));
}

#[tokio::test]
async fn says_what_is_published_and_what_is_in_the_way() {
    let repo = TestRepo::new().await;
    let (_, a, b, c) = three(&repo).await;
    repo.run(&["reset", "--quiet", "--hard", &b]).await;
    let _remote = repo.add_bare_remote("origin").await;
    repo.run(&["reset", "--quiet", "--hard", &c]).await;
    let p = preview(&repo, &a).await;
    let published: Vec<_> = p.commits.iter().map(|c| c.published).collect();
    assert_eq!(published, [true, true, false]);
    assert_eq!(p.published_in, ["origin/main"]);

    // Only rewriting C publishes nothing; dropping B does.
    let only_c = simulate(
        &repo,
        &plan(
            &p,
            vec![
                step(&a, StepAction::Pick),
                step(&b, StepAction::Pick),
                with_message(&c, StepAction::Reword, "C nuevo"),
            ],
        ),
    )
    .await;
    assert_eq!(only_c.problem, None);
    assert_eq!(only_c.rewritten, 1);
    assert!(only_c.published_in.is_empty());
    let drop_b = simulate(
        &repo,
        &plan(
            &p,
            vec![
                step(&a, StepAction::Pick),
                step(&b, StepAction::Drop),
                step(&c, StepAction::Pick),
            ],
        ),
    )
    .await;
    assert_eq!(drop_b.rewritten, 2);
    assert_eq!(drop_b.published_in, ["origin/main"]);

    // A change in a tracked file is in the way of any rebase.
    repo.write_file("base.txt", "cambiado\n");
    let p = preview(&repo, &a).await;
    assert_eq!(p.blocked, Some(MoveBlock::RebaseNeedsClean));
    assert_eq!(p.blocking[0].path, "base.txt");
    repo.run(&["checkout", "--quiet", "--", "base.txt"]).await;

    // An untracked file a commit of the range writes.
    repo.run(&["rm", "--quiet", "c.txt"]).await;
    repo.run(&["commit", "--quiet", "-m", "Quita c"]).await;
    repo.write_file("c.txt", "mío\n");
    let p = preview(&repo, &a).await;
    assert_eq!(p.blocked, Some(MoveBlock::Overwrite));
    assert!(p.blocking.iter().any(|f| f.path == "c.txt" && f.untracked));
}

#[tokio::test]
async fn checks_the_plan() {
    let repo = TestRepo::new().await;
    let (_, a, b, c) = three(&repo).await;
    let mut p = preview(&repo, &a).await;
    let check = |p: &InteractivePreview, steps: Vec<PlanStep>| validate_plan(p, &plan(p, steps));

    let keep = || {
        vec![
            step(&a, StepAction::Pick),
            step(&b, StepAction::Pick),
            step(&c, StepAction::Pick),
        ]
    };
    assert_eq!(check(&p, keep()), Some(PlanProblem::Unchanged));
    let same_message = vec![
        with_message(&a, StepAction::Reword, &p.commits[0].message),
        step(&b, StepAction::Pick),
        step(&c, StepAction::Pick),
    ];
    assert_eq!(check(&p, same_message), Some(PlanProblem::Unchanged));
    assert_eq!(check(&p, keep()[..2].to_vec()), Some(PlanProblem::Mismatch));
    let twice = vec![
        step(&a, StepAction::Pick),
        step(&a, StepAction::Pick),
        step(&c, StepAction::Pick),
    ];
    assert_eq!(check(&p, twice), Some(PlanProblem::Mismatch));
    let squash_first = vec![
        step(&a, StepAction::Drop),
        with_message(&b, StepAction::Squash, "b"),
        step(&c, StepAction::Pick),
    ];
    assert_eq!(check(&p, squash_first), Some(PlanProblem::SquashFirst));
    let none = vec![
        step(&a, StepAction::Drop),
        step(&b, StepAction::Drop),
        step(&c, StepAction::Drop),
    ];
    assert_eq!(check(&p, none), Some(PlanProblem::AllDropped));
    let empty = vec![
        step(&a, StepAction::Pick),
        with_message(&b, StepAction::Reword, "  \n"),
        step(&c, StepAction::Pick),
    ];
    assert_eq!(
        check(&p, empty),
        Some(PlanProblem::EmptyMessage { oid: b.clone() })
    );
    let reorder = vec![
        step(&a, StepAction::Pick),
        step(&c, StepAction::Pick),
        step(&b, StepAction::Pick),
    ];
    assert_eq!(check(&p, reorder.clone()), None);

    // git 2.30–2.31: reorder and drop, but no new messages.
    p.messages = false;
    assert_eq!(check(&p, reorder), None);
    let squash = vec![
        step(&a, StepAction::Pick),
        with_message(&b, StepAction::Squash, "a y b"),
        step(&c, StepAction::Pick),
    ];
    assert_eq!(check(&p, squash), Some(PlanProblem::MessagesNeedNewerGit));
}

#[tokio::test]
async fn predicts_reorder_squash_and_drop_like_git() {
    let repo = TestRepo::new().await;
    let (_, a, b, c) = three(&repo).await;
    let p = preview(&repo, &a).await;
    let cases = [
        // Reorder: A stays, C and B get new ids.
        (
            vec![
                step(&a, StepAction::Pick),
                step(&c, StepAction::Pick),
                step(&b, StepAction::Pick),
            ],
            3,
            2,
        ),
        // Squash C into B.
        (
            vec![
                step(&a, StepAction::Pick),
                step(&b, StepAction::Pick),
                with_message(&c, StepAction::Squash, "b y c"),
            ],
            2,
            1,
        ),
        // Drop A.
        (
            vec![
                step(&a, StepAction::Drop),
                step(&b, StepAction::Pick),
                step(&c, StepAction::Pick),
            ],
            2,
            3,
        ),
    ];
    for (steps, after, rewritten) in cases {
        let before = state(&repo).await;
        let predicted = simulate(&repo, &plan(&p, steps.clone())).await;
        assert_eq!(state(&repo).await, before, "the preview changed nothing");
        assert_eq!(predicted.problem, None);
        assert_eq!(predicted.stops_at, None);
        assert_eq!(predicted.commits_after, after);
        assert_eq!(predicted.rewritten, rewritten);

        assert!(real_rebase(&repo, &p, &steps).await, "git finished");
        assert_eq!(predicted.tree.as_deref(), Some(tree(&repo).await.as_str()));
        let count = repo
            .run(&["rev-list", "--count", &format!("{a}^..HEAD")])
            .await;
        assert_eq!(count.trim(), after.to_string());
        // Back to where it was, for the next case.
        repo.run(&["reset", "--quiet", "--hard", &c]).await;
    }
}

#[tokio::test]
async fn predicts_where_a_reorder_stops_like_git() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "1\n2\n3\n", "Base").await;
    let a = repo
        .commit_file("f.txt", "1\nA\n3\n", "A cambia la dos")
        .await;
    let b = repo
        .commit_file("f.txt", "1\nB\n3\n", "B la cambia otra vez")
        .await;
    let p = preview(&repo, &a).await;
    let steps = vec![step(&b, StepAction::Pick), step(&a, StepAction::Pick)];
    let predicted = simulate(&repo, &plan(&p, steps.clone())).await;
    let stop = predicted.stops_at.expect("stops");
    assert_eq!((stop.step, stop.of), (1, 2));
    assert_eq!(stop.commit.oid, b);
    assert_eq!(stop.files, ["f.txt"]);
    assert_eq!(predicted.tree, None);

    assert!(!real_rebase(&repo, &p, &steps).await, "git stops");
    assert_eq!(repo.run(&["rev-parse", "REBASE_HEAD"]).await.trim(), b);
    let unmerged = repo.run(&["diff", "--name-only", "--diff-filter=U"]).await;
    assert_eq!(unmerged.trim(), "f.txt");
    repo.run(&["rebase", "--abort"]).await;
}

#[tokio::test]
async fn predicts_commits_that_become_empty_like_git() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "1\n", "Base").await;
    let a = repo.commit_file("f.txt", "2\n", "A: 1 a 2").await;
    let b = repo.commit_file("f.txt", "1\n", "B: vuelve a 1").await;
    let c = repo.commit_file("f.txt", "2\n", "C: otra vez 2").await;
    let p = preview(&repo, &a).await;
    // C right after A: its change is already there.
    let steps = vec![
        step(&a, StepAction::Pick),
        step(&c, StepAction::Pick),
        step(&b, StepAction::Pick),
    ];
    let predicted = simulate(&repo, &plan(&p, steps.clone())).await;
    assert_eq!(predicted.stops_at, None);
    let empty: Vec<_> = predicted.empty.iter().map(|c| c.oid.as_str()).collect();
    assert_eq!(empty, [c.as_str()]);
    assert_eq!(predicted.commits_after, 2);

    assert!(real_rebase(&repo, &p, &steps).await);
    assert_eq!(predicted.tree.as_deref(), Some(tree(&repo).await.as_str()));
    let count = repo
        .run(&["rev-list", "--count", &format!("{a}^..HEAD")])
        .await;
    assert_eq!(count.trim(), "2");
}

#[tokio::test]
async fn from_the_root_commit_like_git() {
    let repo = TestRepo::new().await;
    let (base, a, b, c) = three(&repo).await;
    let p = preview(&repo, &base).await;
    let steps = vec![
        step(&a, StepAction::Pick),
        step(&base, StepAction::Pick),
        step(&b, StepAction::Pick),
        step(&c, StepAction::Pick),
    ];
    let predicted = simulate(&repo, &plan(&p, steps.clone())).await;
    assert_eq!(predicted.problem, None);
    assert_eq!(predicted.stops_at, None);
    assert_eq!(predicted.rewritten, 4);
    assert!(real_rebase(&repo, &p, &steps).await);
    assert_eq!(predicted.tree.as_deref(), Some(tree(&repo).await.as_str()));
}

#[tokio::test]
async fn a_plan_for_an_old_preview_does_nothing() {
    let repo = TestRepo::new().await;
    let (_, a, b, c) = three(&repo).await;
    let p = preview(&repo, &a).await;
    repo.commit_file("d.txt", "d\n", "Añade d").await;
    let steps = vec![
        step(&a, StepAction::Pick),
        step(&c, StepAction::Pick),
        step(&b, StepAction::Pick),
    ];
    let predicted = simulate(&repo, &plan(&p, steps)).await;
    assert_eq!(predicted.problem, Some(PlanProblem::Changed));
}

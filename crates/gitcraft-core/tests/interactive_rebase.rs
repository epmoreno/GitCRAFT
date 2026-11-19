//! Interactive rebase, read-only part (task 13.1): the range, the refusals
//! and the prediction of a plan, compared with what `git rebase -i` does.

use std::path::Path;

use gitcraft_core::CancellationToken;
use gitcraft_core::ops::in_progress::{
    ConflictSides, ContinueOutcome, RebaseProgress, commit_steps, conflict_sides,
    continue_in_progress, skip_current, todo_after_skip,
};
use gitcraft_core::ops::interactive_rebase::{
    InteractiveOutcome, InteractivePreview, InteractiveRefusal, InteractiveRequest, PlanPreview,
    PlanProblem, PlanStep, RebasePlan, StepAction, TOKEN_ENV, TodoHelperError, apply_todo,
    interactive_rebase, interactive_rebase_preview, sequence_editor, simulate_plan,
    todo_helper_main, todo_text, validate_plan,
};
use gitcraft_core::ops::journal::{OpKind, Saved, record, snapshot};
use gitcraft_core::ops::merge_rebase::MoveBlock;
use gitcraft_core::ops::remote::abort_in_progress;
use gitcraft_core::ops::undo::{Direction, UndoOutcome, UndoRequest, undo, undo_preview};
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

// --- applying a plan (13.2) ---------------------------------------------------

const HELPER: &str = env!("CARGO_BIN_EXE_rebase-todo");

async fn apply(
    repo: &TestRepo,
    p: &InteractivePreview,
    steps: Vec<PlanStep>,
) -> InteractiveOutcome {
    apply_with(repo, plan(p, steps), false, false).await
}

async fn apply_with(
    repo: &TestRepo,
    plan: RebasePlan,
    save_changes: bool,
    allow_conflicts: bool,
) -> InteractiveOutcome {
    let request = InteractiveRequest {
        plan,
        save_changes,
        allow_conflicts,
    };
    interactive_rebase(
        repo.git(),
        repo.path(),
        &request,
        Path::new(HELPER),
        &token(),
    )
    .await
    .expect("interactive rebase")
}

/// `author | message` of each commit after `since`, newest first.
async fn log_since(repo: &TestRepo, since: &str) -> Vec<String> {
    repo.run(&["log", "--format=%an|%B%x00", &format!("{since}..HEAD")])
        .await
        .split('\0')
        .map(|r| r.trim_matches('\n').to_owned())
        .filter(|r| !r.is_empty())
        .collect()
}

fn no_plan_file(repo: &TestRepo) {
    assert!(!repo.path().join(".git/gitcraft-rebase-plan").exists());
}

#[tokio::test]
async fn applies_reword_squash_drop_and_reorder() {
    let repo = TestRepo::new().await;
    let (base, a, b, c) = three(&repo).await;
    let p = preview(&repo, &a).await;
    let steps = vec![
        // A gets a new message with accents, several lines and a "#".
        with_message(
            &a,
            StepAction::Reword,
            "Añade «a», bien\n\nPrimera línea.\n# No es un comentario.",
        ),
        // C goes before B, and B is combined into it.
        step(&c, StepAction::Pick),
        with_message(&b, StepAction::Squash, "C y B juntos\n\nCuerpo combinado."),
    ];
    let predicted = simulate(&repo, &plan(&p, steps.clone())).await;
    let outcome = apply(&repo, &p, steps).await;
    let InteractiveOutcome::Done { head, saved: None } = outcome else {
        panic!("not done: {outcome:?}");
    };
    assert_eq!(head, repo.head().await);
    assert_eq!(predicted.tree.as_deref(), Some(tree(&repo).await.as_str()));
    assert_eq!(
        log_since(&repo, &base).await,
        [
            "GitCRAFT Test|C y B juntos\n\nCuerpo combinado.",
            "Ana Pérez|Añade «a», bien\n\nPrimera línea.\n# No es un comentario.",
        ]
    );
    no_plan_file(&repo);

    // Drop only.
    let p = preview(&repo, &head).await;
    let outcome = apply(&repo, &p, vec![step(&head, StepAction::Drop)]).await;
    assert!(
        matches!(outcome, InteractiveOutcome::NotDone { .. }),
        "all dropped"
    );
    let reworded = repo.rev_parse("HEAD~1").await;
    let p = preview(&repo, &reworded).await;
    let outcome = apply(
        &repo,
        &p,
        vec![
            step(&reworded, StepAction::Drop),
            step(&head, StepAction::Pick),
        ],
    )
    .await;
    assert!(matches!(outcome, InteractiveOutcome::Done { .. }));
    assert_eq!(log_since(&repo, &base).await.len(), 1);
    assert!(!repo.path().join("a.txt").exists());
}

#[tokio::test]
async fn stops_on_conflicts_then_continues_skips_or_aborts() {
    async fn sample() -> (TestRepo, String, String, String) {
        let repo = TestRepo::new().await;
        repo.commit_file("f.txt", "1\n2\n3\n", "Base").await;
        let a = repo
            .commit_file("f.txt", "1\nA\n3\n", "A cambia la dos")
            .await;
        let b = repo
            .commit_file("f.txt", "1\nB\n3\n", "B la cambia otra vez")
            .await;
        let c = repo.commit_file("g.txt", "g\n", "C otro archivo").await;
        (repo, a, b, c)
    }
    // B first: it stops at B; C has a new message after the conflict.
    let steps = |a: &str, b: &str, c: &str| {
        vec![
            step(b, StepAction::Pick),
            step(a, StepAction::Pick),
            with_message(c, StepAction::Reword, "C renombrado"),
        ]
    };

    // Not without asking.
    let (repo, a, b, c) = sample().await;
    let p = preview(&repo, &a).await;
    let before = state(&repo).await;
    let outcome = apply(&repo, &p, steps(&a, &b, &c)).await;
    let InteractiveOutcome::NotDone {
        plan,
        changed: false,
        ..
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    assert!(plan.stops_at.is_some());
    assert_eq!(state(&repo).await, before);

    // Resolved and continued: the new message still applies.
    let outcome = apply_with(&repo, plan_for(&p, steps(&a, &b, &c)), false, true).await;
    let InteractiveOutcome::Conflicted { files, .. } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(files, ["f.txt"]);
    no_plan_file(&repo);
    // Task 9's notice counts commits (3), not todo lines (4 with the
    // `fixup -C` of the new message).
    assert_eq!(
        progress(&repo).await,
        Some(RebaseProgress {
            current: 1,
            total: 3
        })
    );
    repo.write_file("f.txt", "1\nB\n3\n");
    repo.run(&["add", "f.txt"]).await;
    let next = continue_in_progress(repo.git(), repo.path(), None, &token())
        .await
        .expect("continue");
    assert!(
        matches!(next, ContinueOutcome::StoppedAgain { .. }),
        "{next:?}"
    );
    assert_eq!(
        progress(&repo).await,
        Some(RebaseProgress {
            current: 2,
            total: 3
        })
    );
    repo.write_file("f.txt", "1\nA\n3\n");
    repo.run(&["add", "f.txt"]).await;
    let done = continue_in_progress(repo.git(), repo.path(), None, &token())
        .await
        .expect("continue");
    assert_eq!(done, ContinueOutcome::Done);
    let subjects = repo.run(&["log", "--format=%s", "-3"]).await;
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["C renombrado", "A cambia la dos", "B la cambia otra vez"]
    );

    // Skipped.
    let (repo, a, b, c) = sample().await;
    let p = preview(&repo, &a).await;
    apply_with(&repo, plan_for(&p, steps(&a, &b, &c)), false, true).await;
    let skipped = skip_current(repo.git(), repo.path(), &token())
        .await
        .expect("skip");
    assert_eq!(skipped, ContinueOutcome::Done);
    let subjects = repo.run(&["log", "--format=%s", "-2"]).await;
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["C renombrado", "A cambia la dos"]
    );

    // Aborted: everything exactly as it was.
    let (repo, a, b, c) = sample().await;
    let p = preview(&repo, &a).await;
    let before = state(&repo).await;
    apply_with(&repo, plan_for(&p, steps(&a, &b, &c)), false, true).await;
    abort_in_progress(repo.git(), repo.path(), &token())
        .await
        .expect("abort");
    assert_eq!(state(&repo).await, before);
}

async fn progress(repo: &TestRepo) -> Option<RebaseProgress> {
    match conflict_sides(repo.git(), repo.path(), &token()).await {
        Ok(Some(ConflictSides::Rebase {
            progress,
            interactive,
            ..
        })) => {
            assert!(interactive, "GitCRAFT's interactive rebase");
            progress
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn skipping_rewrites_only_what_depends_on_the_skipped_commit() {
    let todo = "fixup -C h1\npick a\nfixup b\nfixup c\nfixup -C h2\n";
    // Its own new message goes away with it.
    assert_eq!(
        todo_after_skip("pick x", todo),
        "pick a\nfixup b\nfixup c\nfixup -C h2\n"
    );
    // A combination passes to the first combined commit.
    let todo = "fixup b\nfixup c\nfixup -C h2\npick d\n";
    assert_eq!(
        todo_after_skip("pick a", todo),
        "pick b\nfixup c\nfixup -C h2\npick d\n"
    );
    // Nothing depends on it, or it was itself combined: as it is.
    assert_eq!(todo_after_skip("pick a", "pick d\n"), "pick d\n");
    assert_eq!(todo_after_skip("fixup b", todo), todo);
}

#[test]
fn counts_commits_not_todo_lines() {
    let list =
        "pick 1a\nfixup 2b\nfixup -C 3c\n# comment\nexec make\ndrop 4d\nreword 5e\nf -c 6f\n";
    assert_eq!(commit_steps(list), 3);
}

fn plan_for(p: &InteractivePreview, steps: Vec<PlanStep>) -> RebasePlan {
    plan(p, steps)
}

#[tokio::test]
async fn does_nothing_if_the_repository_changed_or_changes_are_in_the_way() {
    let repo = TestRepo::new().await;
    let (_, a, b, c) = three(&repo).await;
    let reorder = || {
        vec![
            step(&a, StepAction::Pick),
            step(&c, StepAction::Pick),
            step(&b, StepAction::Pick),
        ]
    };
    let p = preview(&repo, &a).await;
    repo.write_file("base.txt", "cambiado\n");
    let before = state(&repo).await;
    let outcome = apply(&repo, &p, reorder()).await;
    assert!(
        matches!(outcome, InteractiveOutcome::NotDone { changed: true, .. }),
        "{outcome:?}"
    );
    assert_eq!(state(&repo).await, before);

    // In the way: not without saving them.
    let p = preview(&repo, &a).await;
    let outcome = apply(&repo, &p, reorder()).await;
    let InteractiveOutcome::NotDone {
        preview: seen,
        changed: false,
        saved: None,
        ..
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    assert_eq!(seen.blocked, Some(MoveBlock::RebaseNeedsClean));
    assert_eq!(state(&repo).await, before);

    // Saved first, then applied; the change is in the stash.
    let outcome = apply_with(&repo, plan(&p, reorder()), true, false).await;
    let InteractiveOutcome::Done {
        saved: Some(entry), ..
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    assert!(!entry.is_empty());
    let stashes = repo.run(&["stash", "list", "--format=%s"]).await;
    assert!(stashes.contains("GitCRAFT: antes del rebase interactivo de main"));
    assert_eq!(
        std::fs::read_to_string(repo.path().join("base.txt")).expect("file"),
        "base\n"
    );
}

#[tokio::test]
async fn is_undone_exactly() {
    let repo = TestRepo::new().await;
    let (_, a, b, c) = three(&repo).await;
    let p = preview(&repo, &a).await;
    let before = state(&repo).await;
    let snap = snapshot(repo.git(), repo.path(), &token())
        .await
        .expect("snapshot");
    apply(
        &repo,
        &p,
        vec![
            step(&a, StepAction::Drop),
            step(&c, StepAction::Pick),
            with_message(&b, StepAction::Reword, "B nuevo"),
        ],
    )
    .await;
    let after = state(&repo).await;
    let kind = OpKind::InteractiveRebase {
        branch: "main".into(),
    };
    let rec = record(
        repo.git(),
        repo.path(),
        snap,
        kind,
        Saved::default(),
        &token(),
    )
    .await
    .expect("record")
    .expect("it moved the branch");
    let undo_preview = undo_preview(repo.git(), repo.path(), &rec, Direction::Undo, &token())
        .await
        .expect("undo preview");
    let request = UndoRequest {
        direction: Direction::Undo,
        expected: undo_preview.fingerprint,
        save_changes: false,
    };
    let outcome = undo(repo.git(), repo.path(), &rec, &request, &token())
        .await
        .expect("undo");
    assert!(matches!(outcome, UndoOutcome::Done { .. }), "{outcome:?}");
    assert_eq!(state(&repo).await, before);
    assert_ne!(before, after);
}

// --- the todo helper ------------------------------------------------------------

#[test]
fn the_helper_only_works_with_its_token_and_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    let plan_file = dir.path().join("plan");
    let todo_file = dir.path().join("todo");
    let full = "a".repeat(40);
    let plan_text = format!("gitcraft-rebase-plan 1\nsecret\n{full}\ndrop {full}\n");
    std::fs::write(&plan_file, &plan_text).expect("file");
    std::fs::write(&todo_file, "pick aaaaaaa Uno\n# comment\n").expect("file");

    // No token, or another one: nothing is touched.
    for token in [None, Some(""), Some("other")] {
        assert_eq!(
            apply_todo(&plan_file, &todo_file, token),
            Err(TodoHelperError::NotOurs)
        );
        assert_eq!(
            std::fs::read_to_string(&plan_file).expect("file"),
            plan_text
        );
        assert_eq!(
            std::fs::read_to_string(&todo_file).expect("file"),
            "pick aaaaaaa Uno\n# comment\n"
        );
    }
    // With it: the plan replaces git's todo, and the plan file is gone.
    assert_eq!(apply_todo(&plan_file, &todo_file, Some("secret")), Ok(()));
    assert_eq!(
        std::fs::read_to_string(&todo_file).expect("file"),
        format!("drop {full}\n")
    );
    assert!(!plan_file.exists());
    // A second time: no plan any more.
    assert_eq!(
        apply_todo(&plan_file, &todo_file, Some("secret")),
        Err(TodoHelperError::NotOurs)
    );

    // git proposing other commits: refused, git's todo left alone.
    std::fs::write(&plan_file, &plan_text).expect("file");
    for proposed in [
        "pick bbbbbbb Otro\n",
        "pick aaaaaaa Uno\npick bbbbbbb Dos\n",
        "exec x\n",
    ] {
        std::fs::write(&plan_file, &plan_text).expect("file");
        std::fs::write(&todo_file, proposed).expect("file");
        assert_eq!(
            apply_todo(&plan_file, &todo_file, Some("secret")),
            Err(TodoHelperError::Mismatch),
            "{proposed}"
        );
        assert_eq!(std::fs::read_to_string(&todo_file).expect("file"), proposed);
    }

    // The command line: not its arguments, so the program goes on.
    assert_eq!(todo_helper_main(&["--other".into()]), None);
    assert_eq!(todo_helper_main(&[]), None);
}

#[tokio::test]
async fn git_does_not_start_if_its_list_is_not_the_preview() {
    let repo = TestRepo::new().await;
    let (base, a, _, _) = three(&repo).await;
    let before = state(&repo).await;
    let plan_file = repo.path().join(".git/gitcraft-rebase-plan");
    // A plan that expects other commits.
    let other = "b".repeat(40);
    std::fs::write(
        &plan_file,
        format!("gitcraft-rebase-plan 1\nsecret\n{other}\ndrop {a}\n"),
    )
    .expect("file");
    let out = repo
        .write()
        .args(["rebase", "-i", &base])
        .env(
            "GIT_SEQUENCE_EDITOR",
            sequence_editor(Path::new(HELPER), &plan_file),
        )
        .env(TOKEN_ENV, "secret")
        .accept_exit_code(1)
        .output(&token())
        .await
        .expect("rebase -i");
    assert_ne!(out.exit_code, Some(0));
    assert!(
        out.stderr.contains("other commits than GitCRAFT's preview"),
        "{}",
        out.stderr
    );
    assert_eq!(state(&repo).await, before);
    assert!(!repo.path().join(".git/rebase-merge").exists());
}

#[tokio::test]
async fn the_helper_runs_from_a_path_with_spaces_and_quotes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let folder = dir.path().join("Mis programas (x86) 'GitCRAFT' año");
    std::fs::create_dir_all(&folder).expect("file");
    let name = if cfg!(windows) {
        "rebase todo.exe"
    } else {
        "rebase todo"
    };
    let program = folder.join(name);
    std::fs::copy(HELPER, &program).expect("file");

    let repo = TestRepo::new().await;
    let (_, a, b, c) = three(&repo).await;
    let p = preview(&repo, &a).await;
    let request = InteractiveRequest {
        plan: plan(
            &p,
            vec![
                step(&a, StepAction::Pick),
                step(&c, StepAction::Pick),
                step(&b, StepAction::Pick),
            ],
        ),
        save_changes: false,
        allow_conflicts: false,
    };
    let outcome = interactive_rebase(repo.git(), repo.path(), &request, &program, &token())
        .await
        .expect("interactive rebase");
    assert!(
        matches!(outcome, InteractiveOutcome::Done { .. }),
        "{outcome:?}"
    );
    let subjects = repo.run(&["log", "--format=%s", "-2"]).await;
    assert_eq!(subjects.lines().collect::<Vec<_>>(), ["Añade b", "Añade c"]);
}

/// Skipping a commit that had a new message, or that others were combined
/// into, never gives its message to the commit before it.
#[tokio::test]
async fn skipping_a_commit_takes_its_new_message_with_it() {
    async fn sample() -> (TestRepo, String, String, String) {
        let repo = TestRepo::new().await;
        repo.commit_file("f.txt", "1\n2\n3\n", "Base").await;
        let a = repo
            .commit_file("f.txt", "1\nA\n3\n", "A cambia la dos")
            .await;
        let b = repo
            .commit_file("f.txt", "1\nB\n3\n", "B la cambia otra vez")
            .await;
        let c = repo.commit_file("g.txt", "g\n", "C otro archivo").await;
        (repo, a, b, c)
    }

    // B first, with a new message: it stops at B, and B is skipped.
    let (repo, a, b, c) = sample().await;
    let p = preview(&repo, &a).await;
    let steps = vec![
        with_message(&b, StepAction::Reword, "B renombrado"),
        step(&a, StepAction::Pick),
        step(&c, StepAction::Pick),
    ];
    let outcome = apply_with(&repo, plan_for(&p, steps), false, true).await;
    assert!(
        matches!(outcome, InteractiveOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    let skipped = skip_current(repo.git(), repo.path(), &token())
        .await
        .expect("skip");
    assert_eq!(skipped, ContinueOutcome::Done);
    let subjects = repo.run(&["log", "--format=%s"]).await;
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["C otro archivo", "A cambia la dos", "Base"]
    );

    // B receives C, with a combined message: skipping B keeps C as a
    // commit of its own, with that message.
    let (repo, a, b, c) = sample().await;
    let p = preview(&repo, &a).await;
    let steps = vec![
        step(&b, StepAction::Pick),
        with_message(&c, StepAction::Squash, "B y C juntos"),
        step(&a, StepAction::Pick),
    ];
    let outcome = apply_with(&repo, plan_for(&p, steps), false, true).await;
    assert!(
        matches!(outcome, InteractiveOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    let skipped = skip_current(repo.git(), repo.path(), &token())
        .await
        .expect("skip");
    assert_eq!(skipped, ContinueOutcome::Done);
    let subjects = repo.run(&["log", "--format=%s"]).await;
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["A cambia la dos", "B y C juntos", "Base"]
    );
    assert_eq!(
        repo.run(&["show", "--format=", "--name-only", "HEAD~1"])
            .await
            .trim(),
        "g.txt"
    );
}

#[test]
fn the_todo_carries_new_messages_in_fixup_c() {
    let oid = |n: char| n.to_string().repeat(40);
    let (a, b, c, d) = (oid('a'), oid('b'), oid('c'), oid('d'));
    let p = RebasePlan {
        from: a.clone(),
        expected: String::new(),
        steps: vec![
            with_message(&a, StepAction::Reword, "A nuevo"),
            step(&b, StepAction::Drop),
            step(&c, StepAction::Pick),
            with_message(&d, StepAction::Squash, "C y D"),
        ],
    };
    let messages = [(a.clone(), "1".repeat(40)), (d.clone(), "2".repeat(40))]
        .into_iter()
        .collect();
    assert_eq!(
        todo_text(&p, &messages),
        format!(
            "pick {a}\nfixup -C {}\npick {c}\nfixup {d}\nfixup -C {}\ndrop {b}\n",
            "1".repeat(40),
            "2".repeat(40)
        )
    );
}

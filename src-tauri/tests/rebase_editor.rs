//! The app's own executable as the interactive rebase's sequence editor
//! (task 13.3): it does nothing without GitCRAFT's one-time token, opens no
//! window, and does the whole rebase when GitCRAFT starts it.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use gitcraft_core::CancellationToken;
use gitcraft_core::ops::interactive_rebase::{
    InteractiveOutcome, InteractiveRequest, PlanStep, RebasePlan, StepAction, TODO_HELPER_FLAG,
    TOKEN_ENV, interactive_rebase, interactive_rebase_preview,
};
use gitcraft_core::testing::TestRepo;

const APP: &str = env!("CARGO_BIN_EXE_gitcraft");

/// Runs the app with `args`; it must end by itself (no window) quickly.
fn run_app(args: &[&Path], token: Option<&str>) -> i32 {
    let mut cmd = Command::new(APP);
    cmd.arg(TODO_HELPER_FLAG).args(args).env_remove(TOKEN_ENV);
    if let Some(token) = token {
        cmd.env(TOKEN_ENV, token);
    }
    let mut child = cmd.spawn().expect("start the app");
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("wait") {
            return status.code().unwrap_or(-1);
        }
        if started.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            panic!("the app did not end by itself: it must not open a window");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn without_the_token_it_does_nothing_and_opens_no_window() {
    let dir = tempfile::tempdir().expect("temp dir");
    let plan = dir.path().join("plan");
    let todo = dir.path().join("todo");
    let full = "a".repeat(40);
    let plan_text = format!("gitcraft-rebase-plan 1\nsecret\n{full}\ndrop {full}\n");
    std::fs::write(&plan, &plan_text).expect("write plan");
    std::fs::write(&todo, "pick aaaaaaa Uno\n").expect("write todo");

    assert_eq!(run_app(&[&plan, &todo], None), 1);
    assert_eq!(run_app(&[&plan, &todo], Some("other")), 1);
    assert_eq!(std::fs::read_to_string(&plan).expect("plan"), plan_text);
    assert_eq!(
        std::fs::read_to_string(&todo).expect("todo"),
        "pick aaaaaaa Uno\n"
    );
    // Missing arguments: an error, not the app.
    assert_eq!(run_app(&[], None), 1);
}

#[tokio::test]
async fn gitcraft_starts_the_whole_rebase_with_it() {
    let repo = TestRepo::new().await;
    repo.commit_file("base.txt", "base\n", "Base").await;
    let a = repo.commit_file("a.txt", "a\n", "A").await;
    let b = repo.commit_file("b.txt", "b\n", "B").await;
    let cancel = CancellationToken::new();
    let preview = interactive_rebase_preview(repo.git(), repo.path(), &a, &cancel)
        .await
        .expect("preview");
    let request = InteractiveRequest {
        plan: RebasePlan {
            from: preview.from.clone(),
            steps: vec![
                PlanStep {
                    oid: b,
                    action: StepAction::Pick,
                    message: None,
                },
                PlanStep {
                    oid: a,
                    action: StepAction::Reword,
                    message: Some("A, ahora el último".into()),
                },
            ],
            expected: preview.fingerprint,
        },
        save_changes: false,
        allow_conflicts: false,
    };
    let outcome = interactive_rebase(repo.git(), repo.path(), &request, Path::new(APP), &cancel)
        .await
        .expect("interactive rebase");
    assert!(
        matches!(outcome, InteractiveOutcome::Done { .. }),
        "{outcome:?}"
    );
    let subjects = repo.run(&["log", "--format=%s", "-2"]).await;
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["A, ahora el último", "B"]
    );
}

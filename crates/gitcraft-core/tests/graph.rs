//! Graph layout over real git history.

use gitcraft_core::graph::{GraphLayout, GraphRow};
use gitcraft_core::ops::log::stream_commits;
use gitcraft_core::parse::log::Commit;
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, Git};

async fn laid_out(repo: &TestRepo) -> Vec<(Commit, GraphRow)> {
    let git = Git::discover().expect("git must be installed to run the tests");
    let mut graph = GraphLayout::new();
    let mut rows = Vec::new();
    stream_commits(&git, repo.path(), false, &CancellationToken::new(), |c| {
        let row = graph.push(&c.oid, &c.parents);
        rows.push((c, row));
    })
    .await
    .expect("stream");
    assert_eq!(graph.width(), 0, "every line reaches its commit");
    rows
}

/// One text line per row: `*` node, `|` passing line, then the subject.
/// Merges/forks show as `\`/`/` markers after the columns.
fn ascii(rows: &[(Commit, GraphRow)]) -> String {
    let width = rows
        .iter()
        .flat_map(|(_, r)| {
            r.passing
                .iter()
                .chain(&r.incoming)
                .chain(&r.outgoing)
                .map(|l| l.column)
                .chain([r.column])
        })
        .max()
        .map_or(0, |m| usize::from(m) + 1);
    rows.iter()
        .map(|(commit, row)| {
            let mut cells = vec![' '; width];
            for lane in &row.passing {
                cells[usize::from(lane.column)] = '|';
            }
            cells[usize::from(row.column)] = '*';
            let fork = row.outgoing.iter().any(|l| l.column != row.column);
            let join = row.incoming.iter().any(|l| l.column != row.column);
            let marks = match (join, fork) {
                (true, true) => "/\\",
                (true, false) => "/ ",
                (false, true) => "\\ ",
                (false, false) => "  ",
            };
            format!(
                "{} {marks} {}",
                cells.iter().collect::<String>(),
                commit.summary
            )
            .trim_end()
            .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn sample_history_layout() {
    let (repo, ids) = TestRepo::sample_history().await;
    let rows = laid_out(&repo).await;

    // main stays in column 0, feature in 1, the unmerged topic in 2;
    // all three lines converge at the root.
    let expected = "\
*      E
*   \\  Merge branch 'feature'
|*     D
|*     C
*|     B
||*    F
*   /  A";
    assert_eq!(ascii(&rows), expected);

    let row = |oid: &str| {
        &rows
            .iter()
            .find(|(c, _)| c.oid == oid)
            .expect("commit in graph")
            .1
    };
    let columns = |lanes: &[gitcraft_core::graph::Lane]| -> Vec<u16> {
        lanes.iter().map(|l| l.column).collect()
    };
    assert_eq!(columns(&row(&ids["M"]).outgoing), [0, 1]);
    assert_eq!(columns(&row(&ids["A"]).incoming), [0, 1, 2]);
    // Lines keep their colour: feature's commits share one, distinct from main.
    assert_eq!(row(&ids["D"]).color, row(&ids["C"]).color);
    assert_ne!(row(&ids["D"]).color, row(&ids["E"]).color);
    assert_ne!(row(&ids["F"]).color, row(&ids["D"]).color);
}

#[tokio::test]
async fn linear_history_is_a_single_column() {
    let repo = TestRepo::new().await;
    for i in 0..5 {
        repo.commit_file("f.txt", &i.to_string(), &format!("c{i}"))
            .await;
    }
    let rows = laid_out(&repo).await;
    assert!(
        rows.iter()
            .all(|(_, r)| r.column == 0 && r.passing.is_empty())
    );
}

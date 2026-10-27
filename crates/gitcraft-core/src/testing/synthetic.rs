//! Large synthetic histories for benchmarks, built with `git fast-import`.

use std::fmt::Write as _;

const FEATURE_SLOTS: usize = 4;

/// A `git fast-import` stream with exactly `commits` commits: a `main`
/// line plus up to four overlapping feature branches that fork, run in
/// parallel and are merged back (one in three is abandoned unmerged), and a
/// tag every 1000 commits. Commits carry no file changes, so it is fast.
pub fn fast_import_stream(commits: usize) -> String {
    let mut stream = Stream {
        out: String::with_capacity(commits * 130),
        mark: 0,
        time: 1_600_000_000,
    };
    let mut main_tip: Option<usize> = None;
    // Per slot: (tip mark, commits left before it ends, merge it at the end?)
    let mut features: [Option<(usize, usize, bool)>; FEATURE_SLOTS] = [None; FEATURE_SLOTS];

    let mut step = 0usize;
    while stream.mark < commits {
        step += 1;
        let slot = step % FEATURE_SLOTS;
        let branch = format!("feature-{slot}");
        features[slot] = match features[slot] {
            None if step.is_multiple_of(7) && main_tip.is_some() => {
                let tip = stream.commit(&branch, main_tip, None);
                Some((tip, 3 + step % 9, !step.is_multiple_of(3)))
            }
            Some((tip, left, merge)) if left > 0 => {
                let tip = stream.commit(&branch, Some(tip), None);
                Some((tip, left - 1, merge))
            }
            Some((tip, _, true)) => {
                main_tip = Some(stream.commit("main", main_tip, Some(tip)));
                None
            }
            Some((tip, _, false)) => {
                // Keep the abandoned branch reachable under its own name
                // before its slot (and branch name) is reused.
                let _ = write!(
                    stream.out,
                    "reset refs/heads/abandoned-{tip}\nfrom :{tip}\n\n"
                );
                main_tip = Some(stream.commit("main", main_tip, None));
                None
            }
            None => {
                main_tip = Some(stream.commit("main", main_tip, None));
                None
            }
        };
        if stream.mark.is_multiple_of(1000) {
            let mark = stream.mark;
            let _ = write!(stream.out, "reset refs/tags/bench-{mark}\nfrom :{mark}\n\n");
        }
    }
    stream.out
}

struct Stream {
    out: String,
    mark: usize,
    time: u64,
}

impl Stream {
    fn commit(&mut self, branch: &str, from: Option<usize>, merge: Option<usize>) -> usize {
        self.mark += 1;
        self.time += 60;
        let message = format!("{branch}: change {}", self.mark);
        let _ = write!(
            self.out,
            "commit refs/heads/{branch}\nmark :{}\ncommitter Bench <bench@gitcraft.invalid> {} +0000\ndata {}\n{message}\n",
            self.mark,
            self.time,
            message.len()
        );
        if let Some(from) = from {
            let _ = writeln!(self.out, "from :{from}");
        }
        if let Some(merge) = merge {
            let _ = writeln!(self.out, "merge :{merge}");
        }
        self.out.push('\n');
        self.mark
    }
}

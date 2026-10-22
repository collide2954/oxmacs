#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Frame {
    pub function: String,
    pub place: String,
}

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

#[expect(
    clippy::cast_precision_loss,
    reason = "sample counts stay far below 2^52"
)]
fn percent(part: usize, total: usize) -> f64 {
    part as f64 * 100.0 / total.max(1) as f64
}

fn section(
    out: &mut String,
    title: &str,
    counts: HashMap<String, usize>,
    total: usize,
    rows: usize,
) {
    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let _ = writeln!(out, "{title}");
    for (name, hits) in ranked.into_iter().take(rows) {
        let _ = writeln!(out, "{:>6.1}%  {name}", percent(hits, total));
    }
}

pub fn report(samples: &[Vec<Frame>], rows: usize) -> String {
    let mut functions: HashMap<String, usize> = HashMap::new();
    let mut lines: HashMap<String, usize> = HashMap::new();
    let mut totals: HashMap<String, usize> = HashMap::new();
    for stack in samples {
        if let Some(leaf) = stack.first() {
            *functions.entry(leaf.function.clone()).or_default() += 1;
            let line = if leaf.place.is_empty() {
                leaf.function.clone()
            } else {
                format!("{}  {}", leaf.function, leaf.place)
            };
            *lines.entry(line).or_default() += 1;
        }
        let mut seen = HashSet::new();
        for frame in stack {
            if seen.insert(frame.function.as_str()) {
                *totals.entry(frame.function.clone()).or_default() += 1;
            }
        }
    }
    let total = samples.len();
    let mut out = String::new();
    section(
        &mut out,
        &format!("Self time by function ({total} samples)"),
        functions,
        total,
        rows,
    );
    out.push('\n');
    section(&mut out, "Self time by line", lines, total, rows);
    out.push('\n');
    section(&mut out, "Total time by function", totals, total, rows);
    out
}

#[cfg(test)]
mod tests {
    use super::{Frame, report};

    fn frame(function: &str, place: &str) -> Frame {
        Frame {
            function: function.to_owned(),
            place: place.to_owned(),
        }
    }

    #[test]
    fn reports_rank_self_time_by_function_and_line_and_total_time_once_per_sample() {
        let add = frame("add", "math.rs:3");
        let add_later = frame("add", "math.rs:4");
        let walk = frame("walk", "walk.rs:9");
        let main = frame("main", "main.rs:1");
        let samples = vec![
            vec![add.clone(), walk.clone(), main.clone()],
            vec![add.clone(), walk.clone(), walk.clone(), main.clone()],
            vec![add_later, main.clone()],
            vec![walk, main],
        ];

        assert_eq!(
            report(&samples, 2),
            "Self time by function (4 samples)
  75.0%  add
  25.0%  walk

Self time by line
  50.0%  add  math.rs:3
  25.0%  add  math.rs:4

Total time by function
 100.0%  main
  75.0%  add
"
        );
    }
}

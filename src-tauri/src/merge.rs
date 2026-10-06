//! Merges assignments that appear on both Canvas and Gradescope.
//!
//! Rules (discussion Round 2): a duplicate becomes one entry; the Gradescope
//! deadline and status win (Gradescope is where the work is handed in), and a
//! differing Canvas deadline is kept for display. When it is unclear whether
//! two entries are the same assignment, both are kept.

use time::Duration;

use crate::domain::{Assignment, Source};

/// Deadlines further apart than this are not considered the same assignment
/// unless the Canvas entry explicitly launches Gradescope.
const MAX_DUE_GAP: Duration = Duration::hours(24);
/// Differences below this are rounding noise, not a real mismatch.
const MISMATCH_TOLERANCE: Duration = Duration::minutes(1);

/// Lowercased title with only letters and digits, so "HW 1" and "hw-1" match.
fn normalize(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn same_assignment(canvas: &Assignment, gradescope: &Assignment) -> bool {
    let title = normalize(&canvas.title);
    if title.is_empty() || title != normalize(&gradescope.title) {
        return false;
    }
    canvas.links_to_gradescope || (canvas.due_at - gradescope.due_at).abs() <= MAX_DUE_GAP
}

/// Returns the list with Canvas duplicates of Gradescope assignments folded
/// into the Gradescope entry. Each entry is merged at most once.
pub fn merge(assignments: Vec<Assignment>) -> Vec<Assignment> {
    let (mut gradescope, canvas): (Vec<_>, Vec<_>) = assignments
        .into_iter()
        .partition(|a| a.source == Source::Gradescope);
    let mut taken = vec![false; gradescope.len()];
    let mut out = Vec::with_capacity(canvas.len() + gradescope.len());

    for c in canvas {
        // Unambiguous only: exactly one unmerged Gradescope candidate.
        let candidates: Vec<usize> = (0..gradescope.len())
            .filter(|&i| !taken[i] && same_assignment(&c, &gradescope[i]))
            .collect();
        if let [i] = candidates[..] {
            taken[i] = true;
            let g = &mut gradescope[i];
            g.also_in_canvas = true;
            if (c.due_at - g.due_at).abs() > MISMATCH_TOLERANCE {
                g.canvas_due_at = Some(c.due_at);
            }
        } else {
            out.push(c);
        }
    }
    out.extend(gradescope);
    out.sort_by(|a, b| a.due_at.cmp(&b.due_at).then_with(|| a.id.cmp(&b.id)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Status, SubmissionKind};
    use time::macros::datetime;
    use time::OffsetDateTime;

    fn a(id: &str, source: Source, title: &str, due: OffsetDateTime) -> Assignment {
        Assignment {
            id: id.into(),
            course_id: "c".into(),
            title: title.into(),
            due_at: due,
            late_due_at: None,
            url: None,
            kind: SubmissionKind::Online,
            status: Status::NotSubmitted,
            source,
            links_to_gradescope: false,
            also_in_canvas: false,
            canvas_due_at: None,
        }
    }

    const DUE: OffsetDateTime = datetime!(2026-10-10 06:59 UTC);

    #[test]
    fn merges_same_title_and_close_deadline() {
        let out = merge(vec![
            a("1", Source::Canvas, "HW 1", DUE),
            a("gs:1:9", Source::Gradescope, "hw-1", DUE),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "gs:1:9");
        assert!(out[0].also_in_canvas);
        assert_eq!(out[0].canvas_due_at, None);
    }

    #[test]
    fn records_a_deadline_mismatch_and_keeps_gradescope_time() {
        let canvas_due = DUE + Duration::hours(2);
        let out = merge(vec![
            a("1", Source::Canvas, "Lab 2", canvas_due),
            a("gs:1:9", Source::Gradescope, "Lab 2", DUE),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].due_at, DUE);
        assert_eq!(out[0].canvas_due_at, Some(canvas_due));
    }

    #[test]
    fn far_apart_deadlines_need_an_explicit_gradescope_link() {
        let far = DUE + Duration::days(7);
        let unlinked = merge(vec![
            a("1", Source::Canvas, "Quiz", far),
            a("gs:1:9", Source::Gradescope, "Quiz", DUE),
        ]);
        assert_eq!(unlinked.len(), 2);

        let mut linked = a("1", Source::Canvas, "Quiz", far);
        linked.links_to_gradescope = true;
        let merged = merge(vec![linked, a("gs:1:9", Source::Gradescope, "Quiz", DUE)]);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn different_titles_are_kept_apart() {
        let out = merge(vec![
            a("1", Source::Canvas, "Homework 1", DUE),
            a("gs:1:9", Source::Gradescope, "Homework 2", DUE),
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn ambiguous_matches_are_kept_apart() {
        let out = merge(vec![
            a("1", Source::Canvas, "Reading", DUE),
            a("gs:1:8", Source::Gradescope, "Reading", DUE),
            a(
                "gs:2:9",
                Source::Gradescope,
                "Reading",
                DUE + Duration::hours(1),
            ),
        ]);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn each_gradescope_entry_merges_once() {
        let out = merge(vec![
            a("1", Source::Canvas, "Essay", DUE),
            a("2", Source::Canvas, "Essay", DUE),
            a("gs:1:9", Source::Gradescope, "Essay", DUE),
        ]);
        // The first Canvas entry takes the only candidate; the second stays separate.
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn canvas_only_and_gradescope_only_pass_through_sorted() {
        let out = merge(vec![
            a(
                "gs:1:9",
                Source::Gradescope,
                "Late one",
                DUE + Duration::days(1),
            ),
            a("1", Source::Canvas, "Early one", DUE),
        ]);
        assert_eq!(
            out.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
            ["1", "gs:1:9"]
        );
    }
}

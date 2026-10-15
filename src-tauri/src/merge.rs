//! Merges assignments that appear on both Canvas and Gradescope.
//!
//! Rules (discussion Round 2): a duplicate becomes one entry; the Gradescope
//! deadline and status win (Gradescope is where the work is handed in), and a
//! differing Canvas deadline is kept for display. When it is unclear whether
//! two entries are the same assignment, both are kept.

use std::collections::HashMap;

use time::Duration;

use crate::domain::{Assignment, Course, Source};

/// Deadlines further apart than this are not considered the same assignment
/// unless the Canvas entry explicitly launches Gradescope.
const MAX_DUE_GAP: Duration = Duration::hours(24);
/// Differences below this are rounding noise, not a real mismatch.
const MISMATCH_TOLERANCE: Duration = Duration::minutes(1);

/// Lowercased text with only letters and digits, so "HW 1" and "hw-1" match.
fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Each course's normalized code and name, by course ID.
type CourseLabels = HashMap<String, Vec<String>>;

fn course_labels(courses: &[Course]) -> CourseLabels {
    courses
        .iter()
        .map(|c| {
            let mut labels: Vec<String> = [c.code.as_str(), c.name.as_str()]
                .into_iter()
                .map(normalize)
                .filter(|s| !s.is_empty())
                .collect();
            labels.dedup();
            (c.id.clone(), labels)
        })
        .collect()
}

/// Canvas and Gradescope share no course ID, so two courses count as the
/// same when a code or name of one contains a code or name of the other:
/// "CSE110_FA26_A00" on Canvas contains "CSE 110" on Gradescope. A very
/// short label can match more than it should; that only falls back to
/// matching on title and deadline alone.
fn same_course(labels: &CourseLabels, canvas_course: &str, gradescope_course: &str) -> bool {
    let (Some(a), Some(b)) = (labels.get(canvas_course), labels.get(gradescope_course)) else {
        return false;
    };
    a.iter().any(|x| {
        b.iter()
            .any(|y| x.contains(y.as_str()) || y.contains(x.as_str()))
    })
}

fn same_assignment(labels: &CourseLabels, canvas: &Assignment, gradescope: &Assignment) -> bool {
    let title = normalize(&canvas.title);
    if title.is_empty() || title != normalize(&gradescope.title) {
        return false;
    }
    if !same_course(labels, &canvas.course_id, &gradescope.course_id) {
        return false;
    }
    canvas.links_to_gradescope || (canvas.due_at - gradescope.due_at).abs() <= MAX_DUE_GAP
}

/// Returns the list with Canvas duplicates of Gradescope assignments folded
/// into the Gradescope entry. Only unambiguous pairs merge: the Canvas entry
/// has one Gradescope candidate, and no other Canvas entry looks like it.
pub fn merge(courses: &[Course], assignments: Vec<Assignment>) -> Vec<Assignment> {
    let labels = course_labels(courses);
    let (mut gradescope, canvas): (Vec<_>, Vec<_>) = assignments
        .into_iter()
        .partition(|a| a.source == Source::Gradescope);
    let candidates: Vec<Vec<usize>> = canvas
        .iter()
        .map(|c| {
            (0..gradescope.len())
                .filter(|&i| same_assignment(&labels, c, &gradescope[i]))
                .collect()
        })
        .collect();
    let mut claims = vec![0usize; gradescope.len()];
    for &i in candidates.iter().flatten() {
        claims[i] += 1;
    }
    let mut out = Vec::with_capacity(canvas.len() + gradescope.len());

    for (c, found) in canvas.into_iter().zip(candidates) {
        match found[..] {
            [i] if claims[i] == 1 => {
                let g = &mut gradescope[i];
                g.also_in_canvas = true;
                if (c.due_at - g.due_at).abs() > MISMATCH_TOLERANCE {
                    g.canvas_due_at = Some(c.due_at);
                }
            }
            _ => out.push(c),
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

    fn course(id: &str, code: &str, name: &str, source: Source) -> Course {
        Course {
            id: id.into(),
            code: code.into(),
            name: name.into(),
            source,
        }
    }

    /// Canvas course "1" and Gradescope course "gs:9" are the same course;
    /// "2" and "gs:8" are another one.
    fn courses() -> Vec<Course> {
        vec![
            course("1", "ABC 1", "Intro to Things", Source::Canvas),
            course("2", "XYZ 2", "More Things", Source::Canvas),
            course("gs:9", "ABC 1", "Intro to Things", Source::Gradescope),
            course("gs:8", "XYZ 2", "More Things", Source::Gradescope),
        ]
    }

    fn a(id: &str, source: Source, title: &str, due: OffsetDateTime) -> Assignment {
        Assignment {
            id: id.into(),
            course_id: match source {
                Source::Canvas => "1".into(),
                Source::Gradescope => "gs:9".into(),
            },
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

    fn pair() -> Vec<Assignment> {
        vec![
            a("1", Source::Canvas, "HW 1", DUE),
            a("gs:1:9", Source::Gradescope, "hw-1", DUE),
        ]
    }

    #[test]
    fn merges_same_title_and_close_deadline() {
        let out = merge(&courses(), pair());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "gs:1:9");
        assert!(out[0].also_in_canvas);
        assert_eq!(out[0].canvas_due_at, None);
    }

    #[test]
    fn records_a_deadline_mismatch_and_keeps_gradescope_time() {
        let canvas_due = DUE + Duration::hours(2);
        let out = merge(
            &courses(),
            vec![
                a("1", Source::Canvas, "Lab 2", canvas_due),
                a("gs:1:9", Source::Gradescope, "Lab 2", DUE),
            ],
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].due_at, DUE);
        assert_eq!(out[0].canvas_due_at, Some(canvas_due));
    }

    #[test]
    fn far_apart_deadlines_need_an_explicit_gradescope_link() {
        let far = DUE + Duration::days(7);
        let unlinked = merge(
            &courses(),
            vec![
                a("1", Source::Canvas, "Quiz", far),
                a("gs:1:9", Source::Gradescope, "Quiz", DUE),
            ],
        );
        assert_eq!(unlinked.len(), 2);

        let mut linked = a("1", Source::Canvas, "Quiz", far);
        linked.links_to_gradescope = true;
        let merged = merge(
            &courses(),
            vec![linked, a("gs:1:9", Source::Gradescope, "Quiz", DUE)],
        );
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn different_titles_are_kept_apart() {
        let out = merge(
            &courses(),
            vec![
                a("1", Source::Canvas, "Homework 1", DUE),
                a("gs:1:9", Source::Gradescope, "Homework 2", DUE),
            ],
        );
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn same_title_in_another_course_is_kept_apart() {
        // "Homework 3" in a Canvas-only course must not vanish into another
        // course's Gradescope entry that happens to be due the same day.
        let mut other_course = a("1", Source::Canvas, "Homework 3", DUE);
        other_course.course_id = "2".into();
        let out = merge(
            &courses(),
            vec![
                other_course,
                a("gs:1:9", Source::Gradescope, "Homework 3", DUE),
            ],
        );
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|x| !x.also_in_canvas));
    }

    #[test]
    fn courses_match_when_a_code_or_name_contains_the_other() {
        let coded = vec![
            course("1", "ABC1_FA26_A00", "Intro to Things", Source::Canvas),
            course("gs:9", "ABC 1", "Fall 2026", Source::Gradescope),
        ];
        assert_eq!(merge(&coded, pair()).len(), 1);

        // Codes that say nothing about the course, names that agree.
        let named = vec![
            course("1", "Section A", "Intro to Things", Source::Canvas),
            course(
                "gs:9",
                "Fall 2026",
                "Intro to Things (Fall 2026)",
                Source::Gradescope,
            ),
        ];
        assert_eq!(merge(&named, pair()).len(), 1);

        // Nothing in common: kept apart rather than guessed.
        let unrelated = vec![
            course("1", "ABC 1", "Intro to Things", Source::Canvas),
            course("gs:9", "DEF 2", "Other Things", Source::Gradescope),
        ];
        assert_eq!(merge(&unrelated, pair()).len(), 2);
    }

    #[test]
    fn ambiguous_matches_are_kept_apart() {
        let out = merge(
            &courses(),
            vec![
                a("1", Source::Canvas, "Reading", DUE),
                a("gs:1:8", Source::Gradescope, "Reading", DUE),
                a(
                    "gs:2:9",
                    Source::Gradescope,
                    "Reading",
                    DUE + Duration::hours(1),
                ),
            ],
        );
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn several_canvas_candidates_are_ambiguous_too() {
        let out = merge(
            &courses(),
            vec![
                a("1", Source::Canvas, "Essay", DUE),
                a("2", Source::Canvas, "Essay", DUE),
                a("gs:1:9", Source::Gradescope, "Essay", DUE),
            ],
        );
        // Neither Canvas entry is clearly the duplicate, so nothing merges.
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn canvas_only_and_gradescope_only_pass_through_sorted() {
        let out = merge(
            &courses(),
            vec![
                a(
                    "gs:1:9",
                    Source::Gradescope,
                    "Late one",
                    DUE + Duration::days(1),
                ),
                a("1", Source::Canvas, "Early one", DUE),
            ],
        );
        assert_eq!(
            out.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
            ["1", "gs:1:9"]
        );
    }
}

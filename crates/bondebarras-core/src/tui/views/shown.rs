//! The resources column's subject: which repository it shows, and whether
//! that repository's listing is there yet — spec §3, and the ruling carried
//! from Task 4's review.
//!
//! With three columns, once the cursors moved nothing on screen said whose
//! resources the column listed. The column now names its repository on its
//! first lines, and puts `(chargement…)` or `(échec du chargement)` in place
//! of a listing it does not have — never an empty list, which would read as
//! "this repository holds nothing".

use crate::tui::app::Shown;
use crate::tui::theme;
use ratatui::text::{Line, Span};

/// The resources column's title while it has no listing to count: "0
/// éléments" over a listing still on its way would say the repository is
/// empty.
pub(crate) const BARE_TITLE: &str = " RESSOURCES ";

/// In place of the list while its pause runs or its load is in flight.
const LOADING: &str = "(chargement…)";

/// In place of the list once its load failed: the status line says why, and
/// `Entrée` retries.
const FAILED: &str = "(échec du chargement)";

/// The repository `shown` is about, as `(org, repo)`: what the resources
/// column names on its first lines, and what the header recalls once the
/// repos column is hidden (`views::hidden_context`) — one identity for both,
/// in every state. `None` for `Shown::Nothing`.
pub(crate) fn subject(shown: &Shown) -> Option<(&str, &str)> {
    match shown {
        Shown::Nothing => None,
        Shown::Listing { org, repo }
        | Shown::Loading { org, repo }
        | Shown::Failed { org, repo } => Some((org, repo)),
    }
}

/// The lines the resources column opens with, `width` cells wide: the name
/// of the repository `shown` is about (`subject`), then — when there is no
/// listing to draw under it — what stands in for one. Nothing for
/// `Shown::Nothing`.
pub(crate) fn head_lines(shown: &Shown, width: u16) -> Vec<Line<'static>> {
    let Some((org, repo)) = subject(shown) else {
        return Vec::new();
    };
    let stand_in = match shown {
        Shown::Loading { .. } => Some(LOADING),
        Shown::Failed { .. } => Some(FAILED),
        Shown::Nothing | Shown::Listing { .. } => None,
    };
    let mut lines: Vec<Line<'static>> = name_lines(org, repo, usize::from(width))
        .into_iter()
        .map(|line| Line::from(Span::styled(line, theme::title_style())))
        .collect();
    if let Some(text) = stand_in {
        lines.push(Line::from(Span::styled(text, theme::muted())));
    }
    lines
}

/// `org/repo` over as many lines of at most `width` characters as it takes,
/// never clipped: on one line when it fits, otherwise broken after the `/`,
/// and a part still wider than `width` broken where it reaches it. GitHub
/// org and repository names are ASCII, so a character is a cell.
fn name_lines(org: &str, repo: &str, width: usize) -> Vec<String> {
    let whole = format!("{org}/{repo}");
    if width == 0 || whole.chars().count() <= width {
        return vec![whole];
    }
    let mut lines = pieces(&format!("{org}/"), width);
    lines.extend(pieces(repo, width));
    lines
}

/// `text` cut into pieces of `width` characters, the last one shorter.
fn pieces(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(width)
        .map(|piece| piece.iter().collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::model::{OrgSummary, RepoSummary, Resource, ResourceKind};
    use crate::repos::RepoClass;
    use crate::tui::app::{App, Focus};
    use crate::tui::views::testing;
    use std::time::{Duration, Instant};

    const ORG: &str = "SecondBrain-organisation";
    const LONG: &str = "claudine-landing-positioning";

    fn repo(name: &str) -> RepoSummary {
        RepoSummary {
            name: name.into(),
            cache_bytes: 0,
            cache_count: 0,
            private: false,
            age_days: 5,
            class: RepoClass::Archivable,
        }
    }

    /// A cache whose size, `273.7 Mo`, survives the narrowest resources
    /// column (`tui::views::tests::the_resources_column_survives_every_width`)
    /// — the cell to look for to tell whether a listing is on screen.
    fn cache(id: u64) -> Resource {
        Resource {
            kind: ResourceKind::Cache,
            id,
            label: "v0-rust-coverage-Linux-x64".into(),
            size_bytes: 273_678_336,
            age_days: 40,
            git_ref: None,
            stale_pr: false,
            protected: false,
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        }
    }

    /// One org whose login and first repository together (53 characters)
    /// overflow the resources column at its narrowest (38 cells inside its
    /// borders): a name drawn on one clipped line loses the end of the
    /// repository's.
    fn app() -> App {
        App::new(vec![OrgSummary {
            login: ORG.into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![repo(LONG), repo("josephine"), repo("lokiprint")],
            billing: None,
            ..Default::default()
        }])
    }

    /// The ruling carried from Task 4's review: once the cursors move,
    /// nothing else on screen says whose resources the column lists, so the
    /// column says it. Read from the column's real rect at every width from
    /// 60 to 200: the org and the repository both survive whole beside the
    /// listing.
    #[test]
    fn the_resources_column_names_the_repository_it_lists_at_every_width() {
        let mut app = app();
        app.remember((ORG.into(), LONG.into()), vec![cache(1)]);
        assert!(app.follow_cursor(Instant::now()).is_none());

        for width in 60..=200u16 {
            let (_, column) = testing::focused_column(&mut app, Focus::Resources, width, 16);
            assert!(
                column.contains("274Mo"),
                "the cached listing is not on screen at width {width}:\n{column}"
            );
            assert!(
                column.contains(ORG) && column.contains(LONG),
                "the column does not name {ORG}/{LONG} whole at width {width}:\n{column}"
            );
        }
    }

    /// The other half of the ruling, through each state the cursor can leave
    /// a listing in. Josephine's listing is on screen; the cursor moves to
    /// the long-named repository, never loaded — during its pause, then with
    /// its load in flight — then on to lokiprint, while that superseded load
    /// lands. At every width from 60 to 200 the column names the repository
    /// under the cursor, says `(chargement…)`, lists no row of the one it
    /// left, and does not count "0 éléments" for a listing it does not have.
    #[test]
    fn the_resources_column_never_lists_a_repository_the_cursor_left_at_every_width() {
        let mut app = app();
        let t0 = Instant::now();
        app.remember((ORG.into(), "josephine".into()), vec![cache(1)]);
        app.repo_cursor = 1;
        assert!(app.follow_cursor(t0).is_none());
        assert_eq!(app.loaded, Some((ORG.into(), "josephine".into())));

        let sweep = |app: &mut App, name: &str, phase: &str| {
            for width in 60..=200u16 {
                let (_, column) = testing::focused_column(app, Focus::Resources, width, 16);
                assert!(
                    column.contains(name) && column.contains("(chargement…)"),
                    "{phase}: the column does not say {name} is loading at width {width}:\n{column}"
                );
                assert!(
                    !column.contains("274Mo") && !column.contains("éléments"),
                    "{phase}: the column lists another repository at width {width}:\n{column}"
                );
            }
        };

        app.repo_cursor = 0;
        assert!(app.follow_cursor(t0 + Duration::from_millis(100)).is_none());
        sweep(&mut app, LONG, "during the pause");

        let superseded = app
            .follow_cursor(t0 + Duration::from_millis(400))
            .expect("the long-named repository's load starts");
        sweep(&mut app, LONG, "load in flight");

        app.repo_cursor = 2;
        assert!(app.follow_cursor(t0 + Duration::from_millis(500)).is_none());
        app.land_load(superseded, Ok((vec![cache(2)], vec![])));
        sweep(&mut app, "lokiprint", "superseded load landed");
    }

    /// A load that failed says so in place of the list, under the name of
    /// the repository it failed for — not `(chargement…)` for ever.
    #[test]
    fn a_failed_load_says_so_under_the_repositorys_name_at_every_width() {
        let mut app = app();
        let t0 = Instant::now();
        assert!(app.follow_cursor(t0).is_none());
        let load = app.force_load().expect("Entrée starts the load");
        app.land_load(load, Err(anyhow::anyhow!("503")));

        for width in 60..=200u16 {
            let (_, column) = testing::focused_column(&mut app, Focus::Resources, width, 16);
            assert!(
                column.contains(LONG) && column.contains("(échec du chargement)"),
                "the failure is not said under {LONG} at width {width}:\n{column}"
            );
            assert!(
                !column.contains("(chargement…)"),
                "a failed load still reads as loading at width {width}:\n{column}"
            );
        }
    }
}

//! Reading a board: what a scan finds, and the stamps that say it need not
//! run again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{
    Board, CardFacts, PastReview, Source, build_log, discovery, files, reviews, workspace,
};

/// A board, the facts its cards show, and the last review of each of its
/// issues that has one.
pub(super) type Scanned = (Board, CardFacts, HashMap<PathBuf, PastReview>);

/// Reads the board of `source` and its reviews, or says why it could not.
/// Each project's reviews are read from its own issues folder: names are only
/// unique within one.
pub(super) fn scan(source: &Source) -> Result<Scanned, String> {
    let board = match source {
        Source::Project(located) => discovery::build(located)?,
        Source::Group(group, file) => {
            let order = workspace::Order::load(file.as_deref());
            workspace::build(group, &workspace::virtual_root(&group.name), &order)
        }
    };
    let roots: Vec<PathBuf> = match board.repos.as_slice() {
        [] => vec![board.root.clone()],
        repos => repos.iter().map(|repo| repo.root.clone()).collect(),
    };
    let facts = CardFacts::read(&board);
    let mut reviews = HashMap::new();
    for root in roots {
        reviews.extend(past_reviews(&board, &root, &reviews::load_all(&root)));
    }
    Ok((board, facts, reviews))
}

/// What a scan of `source` reads: the folders and files of its projects, the
/// reviews and build logs beside their issues, and the git files their done
/// dates come from. Cheap enough to check every couple of seconds.
pub(super) fn scan_stamps(source: &Source) -> files::Stamps {
    match source {
        Source::Project(located) => {
            let mut stamps = discovery::stamps(located);
            stamp_sidecars(&mut stamps, &located.root);
            stamps
        }
        Source::Group(group, order) => {
            let mut stamps = workspace::stamps(group, order.as_deref());
            for root in workspace::roots(group) {
                stamp_sidecars(&mut stamps, &root);
            }
            stamps
        }
    }
}

/// Adds the reviews and build logs a scan reads beside the issues in `root`.
fn stamp_sidecars(stamps: &mut files::Stamps, root: &Path) {
    for dir in [reviews::DIR, build_log::DIR] {
        stamps.add_folder(&root.join(dir));
    }
}

/// The last review of every issue of `board` under `root` that has one.
fn past_reviews(
    board: &Board,
    root: &Path,
    sidecars: &HashMap<String, reviews::Sidecar>,
) -> HashMap<PathBuf, PastReview> {
    board
        .columns
        .iter()
        .flat_map(|column| &column.issues)
        .filter(|issue| board.root_of(&issue.path) == root)
        .filter_map(|issue| {
            let stored = sidecars.get(&reviews::slug(&issue.path))?.review.as_ref()?;
            Some((issue.path.clone(), PastReview::new(stored, &issue.markdown)))
        })
        .collect()
}

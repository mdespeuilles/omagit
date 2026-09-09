//! Commit-graph topology: which lane a commit sits in, and what connects to it.
//!
//! SPEC §15 calls this the hardest algorithm in the project, and asks for three
//! things: isolate it, test it on pathological histories, never couple it to
//! rendering. So this module knows nothing about pixels, colours or the screen.
//! It takes commits and produces lane indices and links. The colours are
//! generated separately and never read the theme (DESIGN-TOKENS §6); the
//! drawing is `omagit-ui`'s.
//!
//! ## Incremental by construction
//!
//! SPEC §11 wants the graph computed incrementally on the background thread, and
//! §12 wants 100 000 commits without slowing down. Both fall out of the shape
//! here: [`Graph::push`] takes one commit and returns one row, and the only
//! state it carries between rows is what each open lane is waiting for. A page
//! that arrives later resumes from that, with no re-walk and no second pass.
//!
//! ## What a lane is
//!
//! A column in the gutter. A lane is *open* while some commit already drawn is
//! still waiting for one of its parents; it closes when that parent is drawn, or
//! when the history runs out. Lanes are never compacted: a lane that closes is
//! reused by the next branch that needs one, which keeps a long-running branch
//! in the same column for its whole life. Compacting would make the graph
//! narrower and much harder to follow.

use std::collections::HashMap;

use crate::{Commit, ObjectId};

/// One row of the graph — one commit, and everything drawn beside it.
///
/// The three lists are what a renderer needs and nothing more: a line that
/// passes by, a line that arrives at the node, a line that leaves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: ObjectId,
    /// The column the node is drawn in.
    pub lane: usize,
    /// Lanes crossing this row without touching the node, drawn as a straight
    /// vertical line.
    pub passing: Vec<usize>,
    /// Lanes coming down from the row above into this node: this commit is a
    /// parent of something already drawn. More than one means several branches
    /// converge here — which is *not* the same as this commit being a merge.
    pub incoming: Vec<usize>,
    /// Lanes leaving this node downward, one per parent. Empty for a root.
    pub outgoing: Vec<usize>,
    /// How many lanes are open across this row, so the gutter can be sized
    /// without a second pass.
    pub width: usize,
}

impl Row {
    /// Whether anything below still refers to this row's node.
    pub fn is_tip(&self) -> bool {
        self.incoming.is_empty()
    }

    /// A commit with no parents: the bottom of a line of history. A repository
    /// can have several (SPEC §13 asks for a history with fifty).
    pub fn is_root(&self) -> bool {
        self.outgoing.is_empty()
    }
}

/// The lane assignment, built one commit at a time.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    /// What each lane is waiting for. `None` is a free column.
    lanes: Vec<Option<ObjectId>>,
    rows: usize,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many rows have been placed.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// How many lanes are open right now — what the next row will need at most.
    pub fn open_lanes(&self) -> usize {
        self.lanes.iter().filter(|lane| lane.is_some()).count()
    }

    /// Place one commit and describe its row.
    ///
    /// Commits must arrive in the order the history is drawn in — newest first,
    /// as [`crate::history::Walk`] produces them. A commit whose child has not
    /// been drawn yet simply starts a new lane, which is what a second branch
    /// tip is.
    pub fn push(&mut self, commit: &Commit) -> Row {
        // Every lane waiting for this commit. More than one means branches that
        // diverged earlier meet again here.
        let arrivals: Vec<usize> = self
            .lanes
            .iter()
            .enumerate()
            .filter(|(_, waiting)| waiting.as_ref() == Some(&commit.id))
            .map(|(index, _)| index)
            .collect();

        // The leftmost lane that wanted it keeps it, so a branch stays in the
        // column it has been in. Nothing wanted it: it is a tip, and takes the
        // leftmost free column.
        let lane = match arrivals.first() {
            Some(first) => *first,
            None => self.claim(),
        };

        // Lanes that were waiting for this commit and are not its lane end
        // here; freeing them before assigning parents lets a merge reuse the
        // columns its own branches just vacated.
        for other in arrivals.iter().skip(1) {
            self.lanes[*other] = None;
        }

        // Anything still open elsewhere passes this row by.
        let passing: Vec<usize> = self
            .lanes
            .iter()
            .enumerate()
            .filter(|(index, waiting)| *index != lane && waiting.is_some())
            .map(|(index, _)| index)
            .collect();

        // The first parent continues in this commit's own lane; the rest take
        // columns of their own. A parent already waited for elsewhere does not
        // get a second lane — it is the same commit, and drawing it twice is
        // how a graph stops being readable.
        let mut outgoing = Vec::with_capacity(commit.parents.len());
        self.lanes[lane] = None;
        for (position, parent) in commit.parents.iter().enumerate() {
            if let Some(existing) = self.waiting_for(parent) {
                outgoing.push(existing);
                continue;
            }
            let target = if position == 0 { lane } else { self.claim() };
            self.lanes[target] = Some(*parent);
            outgoing.push(target);
        }

        self.rows += 1;
        Row {
            id: commit.id,
            lane,
            width: self.width(lane, &passing, &outgoing),
            passing,
            incoming: arrivals,
            outgoing,
        }
    }

    /// The leftmost free column, growing the gutter only when none is free.
    fn claim(&mut self) -> usize {
        match self.lanes.iter().position(Option::is_none) {
            Some(free) => free,
            None => {
                self.lanes.push(None);
                self.lanes.len() - 1
            }
        }
    }

    fn waiting_for(&self, id: &ObjectId) -> Option<usize> {
        self.lanes
            .iter()
            .position(|waiting| waiting.as_ref() == Some(id))
    }

    /// The gutter width this row needs: every column it draws in, above and
    /// below.
    fn width(&self, lane: usize, passing: &[usize], outgoing: &[usize]) -> usize {
        let widest = passing
            .iter()
            .chain(outgoing.iter())
            .copied()
            .chain(std::iter::once(lane))
            .max()
            .unwrap_or(0);
        widest + 1
    }
}

/// Place a whole page at once, for callers that have one.
///
/// The same computation as calling [`Graph::push`] in a loop; it exists so a
/// background task can hand a page over in one call.
pub fn extend(graph: &mut Graph, commits: &[Commit]) -> Vec<Row> {
    commits.iter().map(|commit| graph.push(commit)).collect()
}

/// Row indices by commit, for a caller that needs to jump to a parent.
pub fn index_of(rows: &[Row]) -> HashMap<ObjectId, usize> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| (row.id, index))
        .collect()
}

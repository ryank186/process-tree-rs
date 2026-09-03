use std::collections::{HashMap, VecDeque};
use std::fmt;

/// One row of process data: a pid, its parent pid, and a name.
///
/// A `ppid` of `0` is treated as the reserved "no parent" sentinel
/// (matching the Unix convention that no real, schedulable process
/// holds pid 0), so a record with `ppid: 0` is always a root of the
/// tree regardless of [`Options`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRecord {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
}

impl ProcessRecord {
    pub fn new(pid: u32, ppid: u32, name: impl Into<String>) -> Self {
        ProcessRecord {
            pid,
            ppid,
            name: name.into(),
        }
    }
}

/// Controls how [`ProcessTree::build`] reacts to malformed input.
///
/// The default, [`Options::strict`], treats any inconsistency as an
/// error. [`Options::lenient`] repairs the same inconsistencies
/// instead of rejecting them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    lenient: bool,
}

impl Options {
    /// Fail on duplicate pids, references to a parent that isn't in
    /// the input, and cycles. Equivalent to `Options::default()`.
    pub fn strict() -> Self {
        Options { lenient: false }
    }

    /// Repair rather than reject:
    ///
    /// - a duplicate pid is dropped, keeping the first occurrence;
    /// - a record whose `ppid` isn't present in the input becomes a
    ///   root instead of raising [`TreeError::UnknownParent`];
    /// - a cycle is broken by promoting one of its members to a root
    ///   instead of raising [`TreeError::Cycle`].
    pub fn lenient() -> Self {
        Options { lenient: true }
    }

    pub fn is_lenient(&self) -> bool {
        self.lenient
    }
}

/// An error raised by [`ProcessTree::build`] under [`Options::strict`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeError {
    /// The same pid appeared twice in the input.
    DuplicatePid(u32),
    /// A record's `ppid` doesn't match any pid in the input.
    UnknownParent { pid: u32, ppid: u32 },
    /// Following parent links from some pid eventually loops back on
    /// itself. Lists the pids that make up the loop.
    Cycle(Vec<u32>),
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeError::DuplicatePid(pid) => write!(f, "duplicate pid {pid}"),
            TreeError::UnknownParent { pid, ppid } => write!(
                f,
                "pid {pid} has ppid {ppid}, which is not present in the input"
            ),
            TreeError::Cycle(members) => write!(f, "parent cycle among pids: {members:?}"),
        }
    }
}

impl std::error::Error for TreeError {}

/// A validated forest of processes, built from a flat list of
/// [`ProcessRecord`]s by [`ProcessTree::build`].
#[derive(Debug)]
pub struct ProcessTree {
    records: HashMap<u32, ProcessRecord>,
    parent: HashMap<u32, u32>,
    children: HashMap<u32, Vec<u32>>,
    roots: Vec<u32>,
}

impl ProcessTree {
    /// Build a tree from `records`, validating (or repairing, under
    /// [`Options::lenient`]) as described on [`Options`].
    pub fn build(records: Vec<ProcessRecord>, options: Options) -> Result<Self, TreeError> {
        let mut by_pid: HashMap<u32, ProcessRecord> = HashMap::with_capacity(records.len());
        let mut order: Vec<u32> = Vec::with_capacity(records.len());

        for record in records {
            if by_pid.contains_key(&record.pid) {
                if options.is_lenient() {
                    continue;
                }
                return Err(TreeError::DuplicatePid(record.pid));
            }
            order.push(record.pid);
            by_pid.insert(record.pid, record);
        }

        let mut parent_link: HashMap<u32, u32> = HashMap::new();
        for &pid in &order {
            let ppid = by_pid[&pid].ppid;
            if ppid == 0 {
                continue;
            }
            if !by_pid.contains_key(&ppid) {
                if options.is_lenient() {
                    continue;
                }
                return Err(TreeError::UnknownParent { pid, ppid });
            }
            parent_link.insert(pid, ppid);
        }

        break_or_reject_cycles(&order, &mut parent_link, options)?;

        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut roots = Vec::new();
        for &pid in &order {
            match parent_link.get(&pid) {
                Some(&ppid) => children.entry(ppid).or_default().push(pid),
                None => roots.push(pid),
            }
        }

        Ok(ProcessTree {
            records: by_pid,
            parent: parent_link,
            children,
            roots,
        })
    }

    /// Number of processes in the tree.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The record for `pid`, if it's in the tree.
    pub fn get(&self, pid: u32) -> Option<&ProcessRecord> {
        self.records.get(&pid)
    }

    /// Pids with no parent in the tree, in input order.
    pub fn roots(&self) -> &[u32] {
        &self.roots
    }

    /// Direct children of `pid`, in input order. Empty if `pid` isn't
    /// in the tree or has no children.
    pub fn children_of(&self, pid: u32) -> &[u32] {
        self.children.get(&pid).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The parent of `pid`, or `None` if `pid` is a root or isn't in
    /// the tree.
    pub fn parent_of(&self, pid: u32) -> Option<u32> {
        self.parent.get(&pid).copied()
    }

    /// Ancestors of `pid`, nearest parent first, ending at a root.
    pub fn ancestors(&self, pid: u32) -> Vec<u32> {
        let mut result = Vec::new();
        let mut cur = pid;
        while let Some(&parent) = self.parent.get(&cur) {
            result.push(parent);
            cur = parent;
        }
        result
    }

    /// Every descendant of `pid`, in breadth-first order.
    pub fn descendants(&self, pid: u32) -> Vec<u32> {
        let mut result = Vec::new();
        let mut queue: VecDeque<u32> = self.children_of(pid).iter().copied().collect();
        while let Some(next) = queue.pop_front() {
            result.push(next);
            queue.extend(self.children_of(next).iter().copied());
        }
        result
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Unvisited,
    InProgress,
    Done,
}

/// Walk every pid's parent chain looking for cycles.
///
/// Each pid has at most one outgoing edge (its parent), so the graph
/// formed by `parent_link` is a functional graph: a walk from any
/// node either reaches a root (no entry in `parent_link`), merges
/// into an already-resolved chain, or comes back around to a node
/// still on the current walk, which is the cycle.
fn break_or_reject_cycles(
    order: &[u32],
    parent_link: &mut HashMap<u32, u32>,
    options: Options,
) -> Result<(), TreeError> {
    let mut mark: HashMap<u32, Mark> = order.iter().map(|&pid| (pid, Mark::Unvisited)).collect();

    for &start in order {
        if mark[&start] != Mark::Unvisited {
            continue;
        }

        let mut path = Vec::new();
        let mut cur = start;

        loop {
            match mark[&cur] {
                Mark::Done => {
                    for &pid in &path {
                        mark.insert(pid, Mark::Done);
                    }
                    break;
                }
                Mark::InProgress => {
                    let cycle_start = path
                        .iter()
                        .position(|&p| p == cur)
                        .expect("a node marked in-progress must be on the current path");
                    let cycle = path[cycle_start..].to_vec();

                    if !options.is_lenient() {
                        return Err(TreeError::Cycle(cycle));
                    }

                    // Cut the loop by promoting `cur` to a root; the
                    // rest of the path still resolves through it.
                    parent_link.remove(&cur);
                    for &pid in &path {
                        mark.insert(pid, Mark::Done);
                    }
                    break;
                }
                Mark::Unvisited => {
                    mark.insert(cur, Mark::InProgress);
                    path.push(cur);
                    match parent_link.get(&cur) {
                        Some(&next) => cur = next,
                        None => {
                            for &pid in &path {
                                mark.insert(pid, Mark::Done);
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_simple_tree() {
        let records = vec![
            ProcessRecord::new(1, 0, "init"),
            ProcessRecord::new(2, 1, "agent"),
            ProcessRecord::new(3, 2, "worker"),
        ];
        let tree = ProcessTree::build(records, Options::strict()).unwrap();

        assert_eq!(tree.roots(), &[1]);
        assert_eq!(tree.children_of(1), &[2]);
        assert_eq!(tree.parent_of(3), Some(2));
        assert_eq!(tree.ancestors(3), vec![2, 1]);
        assert_eq!(tree.descendants(1), vec![2, 3]);
    }

    #[test]
    fn strict_rejects_duplicate_pid() {
        let records = vec![
            ProcessRecord::new(1, 0, "init"),
            ProcessRecord::new(1, 0, "init-again"),
        ];
        let err = ProcessTree::build(records, Options::strict()).unwrap_err();
        assert_eq!(err, TreeError::DuplicatePid(1));
    }

    #[test]
    fn strict_rejects_unknown_parent() {
        let records = vec![ProcessRecord::new(2, 1, "orphan")];
        let err = ProcessTree::build(records, Options::strict()).unwrap_err();
        assert_eq!(err, TreeError::UnknownParent { pid: 2, ppid: 1 });
    }

    #[test]
    fn strict_rejects_cycle() {
        let records = vec![
            ProcessRecord::new(1, 2, "a"),
            ProcessRecord::new(2, 1, "b"),
        ];
        let err = ProcessTree::build(records, Options::strict()).unwrap_err();
        match err {
            TreeError::Cycle(members) => {
                assert_eq!(members.len(), 2);
                assert!(members.contains(&1));
                assert!(members.contains(&2));
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn lenient_keeps_first_duplicate() {
        let records = vec![
            ProcessRecord::new(1, 0, "first"),
            ProcessRecord::new(1, 0, "second"),
        ];
        let tree = ProcessTree::build(records, Options::lenient()).unwrap();
        assert_eq!(tree.get(1).unwrap().name, "first");
    }

    #[test]
    fn lenient_promotes_missing_parent_to_root() {
        let records = vec![ProcessRecord::new(2, 1, "orphan")];
        let tree = ProcessTree::build(records, Options::lenient()).unwrap();
        assert_eq!(tree.roots(), &[2]);
    }

    #[test]
    fn lenient_breaks_cycle() {
        let records = vec![
            ProcessRecord::new(1, 2, "a"),
            ProcessRecord::new(2, 1, "b"),
        ];
        let tree = ProcessTree::build(records, Options::lenient()).unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.roots().len(), 1);
    }
}

# proctree

A small Rust library that turns a flat list of `(pid, ppid, name)` records
into a validated process tree.

## The problem

Process listings are flat tables, but the thing you usually want is the
tree: who spawned whom. Building that tree means trusting every `ppid`
points somewhere real, and in practice it often doesn't:

- a listing taken mid-fork can have a child before its parent shows up
- a pid can get reused between two reads of the same source
- a `/proc` scrape can race a process exiting, leaving a dangling parent
- input you don't control can claim any parent it likes, including one
  that closes a loop back on itself

Silently accepting any of that gives you a tree that lies about the
process hierarchy. `proctree` is strict by default: duplicate pids,
references to a parent that isn't in the input, and cycles are all
errors. When you're working with messy or adversarial input and would
rather get a best-effort tree than a failure, opt into `Options::lenient()`
explicitly.

## Usage

```rust
use proctree::{Options, ProcessRecord, ProcessTree};

let records = vec![
    ProcessRecord::new(1, 0, "init"),
    ProcessRecord::new(42, 1, "sshd"),
    ProcessRecord::new(99, 42, "bash"),
];

let tree = ProcessTree::build(records, Options::strict())?;

assert_eq!(tree.roots(), &[1]);
assert_eq!(tree.children_of(1), &[42]);
assert_eq!(tree.ancestors(99), vec![42, 1]);
assert_eq!(tree.descendants(1), vec![42, 99]);
# Ok::<(), proctree::TreeError>(())
```

Strict mode rejects anything that doesn't add up:

```rust
use proctree::{Options, ProcessRecord, ProcessTree, TreeError};

let records = vec![ProcessRecord::new(2, 1, "orphan")]; // no pid 1 in the input

let err = ProcessTree::build(records, Options::strict()).unwrap_err();
assert_eq!(err, TreeError::UnknownParent { pid: 2, ppid: 1 });
```

`Options::lenient()` repairs the same input instead of rejecting it,
by promoting the orphan to a root:

```rust
use proctree::{Options, ProcessRecord, ProcessTree};

let records = vec![ProcessRecord::new(2, 1, "orphan")];

let tree = ProcessTree::build(records, Options::lenient()).unwrap();
assert_eq!(tree.roots(), &[2]);
```

See the [`Options`] docs for the full set of repairs lenient mode makes
(duplicate pids, dangling parents, cycles).

## Status

Early skeleton: tree construction, validation, and basic traversal
(`ancestors`, `descendants`, `children_of`) are implemented and tested.
On Linux, `proctree::linux::read_all()` snapshots the live process table
from `/proc` into `ProcessRecord`s (pair it with `Options::lenient()`,
since a live system can be caught mid-fork or mid-exit).
`proctree::ps::parse_ps_output()` parses the text you get back from
running `ps -eo pid,ppid,comm` yourself, so it works on any platform
that has a `ps` - this crate never runs the command for you.

## License

MIT, see [LICENSE](LICENSE).

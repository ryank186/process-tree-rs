//! Build and query process trees from flat pid/ppid records.
//!
//! A process listing (from `/proc`, `ps -eo pid,ppid,comm`, a container
//! runtime snapshot, whatever) is just a flat table. Turning it into a
//! tree means trusting that every `ppid` points somewhere sane. In
//! practice it often doesn't: a scrape can race a fork or an exit, a
//! pid can get reused between two reads of the same source, and
//! untrusted input can claim any parent it likes, including one that
//! creates a cycle.
//!
//! [`ProcessTree::build`] is strict by default: duplicate pids,
//! dangling parent references, and cycles are all errors. Pass
//! [`Options::lenient`] when you'd rather get a best-effort tree than
//! a failure - see that constructor's docs for the exact rules it
//! applies.
//!
//! ```
//! use proctree::{Options, ProcessRecord, ProcessTree};
//!
//! let records = vec![
//!     ProcessRecord::new(1, 0, "init"),
//!     ProcessRecord::new(42, 1, "sshd"),
//!     ProcessRecord::new(99, 42, "bash"),
//! ];
//!
//! let tree = ProcessTree::build(records, Options::strict()).unwrap();
//! assert_eq!(tree.roots(), &[1]);
//! assert_eq!(tree.ancestors(99), vec![42, 1]);
//! ```

mod tree;

pub mod linux;

pub use tree::{Options, ProcessRecord, ProcessTree, TreeError};

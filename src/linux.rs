//! Read the live process table from `/proc` on Linux.
//!
//! [`read_all`] is only compiled for `target_os = "linux"`; the parsing
//! logic ([`parse_stat_line`]) has no OS dependency and is tested on
//! every platform.

use crate::ProcessRecord;

#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::io;

/// Snapshot every process currently visible under `/proc`.
///
/// Each numeric entry of `/proc` is read as `/proc/<pid>/stat`. A pid
/// that disappears between listing the directory and reading its
/// stat file (it exited) is skipped rather than treated as an error,
/// since that race is normal and not a sign of bad input. A stat file
/// that can't be parsed is likewise skipped.
///
/// The resulting records are not validated - feed them to
/// [`crate::ProcessTree::build`] for that, most likely with
/// [`crate::Options::lenient`] since a live system can be caught
/// mid-fork or mid-exit.
#[cfg(target_os = "linux")]
pub fn read_all() -> io::Result<Vec<ProcessRecord>> {
    let mut out = Vec::new();

    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };

        let stat_path = entry.path().join("stat");
        let Ok(contents) = fs::read_to_string(&stat_path) else {
            continue;
        };

        if let Some(record) = parse_stat_line(pid, contents.trim_end()) {
            out.push(record);
        }
    }

    Ok(out)
}

/// Parse one line of `/proc/<pid>/stat` into a [`ProcessRecord`].
///
/// The format is `pid (comm) state ppid ...`. `comm` is the executable
/// name in parentheses and may itself contain spaces or parentheses
/// (a process can rename itself to almost anything), so the name is
/// taken as everything between the first `(` and the *last* `)` on the
/// line rather than by splitting on whitespace.
///
/// `pid` is taken from the caller (normally the `/proc` directory
/// entry name) rather than re-parsed from the line, since the two
/// always agree for a real `/proc` read and the directory name is
/// available first.
pub fn parse_stat_line(pid: u32, line: &str) -> Option<ProcessRecord> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close <= open {
        return None;
    }

    let name = &line[open + 1..close];
    let mut fields = line[close + 1..].split_whitespace();
    let _state = fields.next()?;
    let ppid: u32 = fields.next()?.parse().ok()?;

    Some(ProcessRecord::new(pid, ppid, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_line() {
        let line = "1 (systemd) S 0 1 1 0 -1 4194560 55654 6929399 24 1240 1042 351 6621 3903 20 0 1";
        let record = parse_stat_line(1, line).unwrap();
        assert_eq!(record.pid, 1);
        assert_eq!(record.ppid, 0);
        assert_eq!(record.name, "systemd");
    }

    #[test]
    fn handles_comm_with_spaces_and_parens() {
        let line = "42 (some (weird) prog) S 1 42 42 0 -1 4194304 100 0 0 0 0 0 0 0 20 0 1";
        let record = parse_stat_line(42, line).unwrap();
        assert_eq!(record.ppid, 1);
        assert_eq!(record.name, "some (weird) prog");
    }

    #[test]
    fn rejects_line_with_no_parens() {
        assert!(parse_stat_line(1, "1 systemd S 0 1 1").is_none());
    }

    #[test]
    fn rejects_truncated_line() {
        assert!(parse_stat_line(1, "1 (systemd) S").is_none());
    }
}

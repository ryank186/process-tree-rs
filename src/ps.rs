//! Parse the text table produced by `ps -eo pid,ppid,comm`.
//!
//! This module only parses text; it doesn't run `ps` itself, so it
//! works the same on every platform regardless of whether `/proc` or
//! any other OS-specific source is available. Feed it whatever you
//! captured from running `ps -eo pid,ppid,comm` (or the equivalent
//! with a header line included - it's skipped automatically).

use crate::ProcessRecord;

/// Parse the full output of `ps -eo pid,ppid,comm`, including its
/// header line.
///
/// Each line is handled independently by [`parse_ps_line`]. Lines
/// that don't parse - the `PID PPID COMMAND` header, blank lines, a
/// truncated last line from a stream that got cut off - are skipped
/// rather than treated as an error, since none of that indicates the
/// process data itself is bad.
pub fn parse_ps_output(input: &str) -> Vec<ProcessRecord> {
    input.lines().filter_map(parse_ps_line).collect()
}

/// Parse one data line of `ps -eo pid,ppid,comm` output, e.g.
/// `"   42     1 sshd"`.
///
/// `ps` right-aligns the numeric columns with variable padding, so
/// the columns can't be recovered by a fixed byte offset - splitting
/// on whitespace is the only portable option. `comm` is the
/// executable name only (no arguments) and in practice never contains
/// spaces, but on the chance one does, every field after `ppid` is
/// rejoined with a single space rather than dropped.
///
/// Returns `None` for anything that isn't at least two numeric fields
/// followed by a name, which includes the header line - `"PID"` isn't
/// a valid pid, so callers don't need to special-case it.
pub fn parse_ps_line(line: &str) -> Option<ProcessRecord> {
    let mut fields = line.split_whitespace();
    let pid: u32 = fields.next()?.parse().ok()?;
    let ppid: u32 = fields.next()?.parse().ok()?;
    let name_parts: Vec<&str> = fields.collect();
    if name_parts.is_empty() {
        return None;
    }
    Some(ProcessRecord::new(pid, ppid, name_parts.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_line() {
        let record = parse_ps_line("   42     1 sshd").unwrap();
        assert_eq!(record.pid, 42);
        assert_eq!(record.ppid, 1);
        assert_eq!(record.name, "sshd");
    }

    #[test]
    fn skips_header_line() {
        assert!(parse_ps_line("  PID  PPID COMMAND").is_none());
    }

    #[test]
    fn skips_blank_and_truncated_lines() {
        assert!(parse_ps_line("").is_none());
        assert!(parse_ps_line("   42     1").is_none());
        assert!(parse_ps_line("   42").is_none());
    }

    #[test]
    fn rejoins_name_with_internal_spaces() {
        let record = parse_ps_line("  7  1 some weird name").unwrap();
        assert_eq!(record.name, "some weird name");
    }

    #[test]
    fn parses_full_output_and_skips_header() {
        let output = "  PID  PPID COMMAND\n    1     0 systemd\n   42     1 sshd\n   99    42 bash\n";
        let records = parse_ps_output(output);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].pid, 1);
        assert_eq!(records[1].pid, 42);
        assert_eq!(records[2].pid, 99);
    }

    #[test]
    fn parses_full_output_with_trailing_blank_lines() {
        let output = "PID PPID COMMAND\n1 0 init\n\n";
        let records = parse_ps_output(output);
        assert_eq!(records, vec![ProcessRecord::new(1, 0, "init")]);
    }
}

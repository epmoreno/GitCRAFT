/// Splits a byte stream that arrives in arbitrary chunks into delimited records.
///
/// Used for NUL-separated stdout (`-z`) and for `\r`/`\n`-separated progress
/// lines on stderr.
pub(crate) struct Splitter {
    buf: Vec<u8>,
    is_delim: fn(u8) -> bool,
}

impl Splitter {
    pub(crate) fn new(is_delim: fn(u8) -> bool) -> Self {
        Self {
            buf: Vec::new(),
            is_delim,
        }
    }

    /// Feeds a chunk and emits every record it completes. Records may be empty.
    pub(crate) fn push(&mut self, chunk: &[u8], mut emit: impl FnMut(&[u8])) {
        let mut rest = chunk;
        while let Some(i) = rest.iter().position(|&b| (self.is_delim)(b)) {
            if self.buf.is_empty() {
                emit(&rest[..i]);
            } else {
                self.buf.extend_from_slice(&rest[..i]);
                emit(&self.buf);
                self.buf.clear();
            }
            rest = &rest[i + 1..];
        }
        self.buf.extend_from_slice(rest);
    }

    /// Emits the trailing record if the stream did not end with a delimiter.
    pub(crate) fn finish(&mut self, mut emit: impl FnMut(&[u8])) {
        if !self.buf.is_empty() {
            emit(&self.buf);
            self.buf.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(chunks: &[&[u8]], is_delim: fn(u8) -> bool) -> Vec<String> {
        let mut out = Vec::new();
        let mut splitter = Splitter::new(is_delim);
        for chunk in chunks {
            splitter.push(chunk, |r| out.push(String::from_utf8_lossy(r).into_owned()));
        }
        splitter.finish(|r| out.push(String::from_utf8_lossy(r).into_owned()));
        out
    }

    fn nul(b: u8) -> bool {
        b == 0
    }

    #[test]
    fn splits_records_within_one_chunk() {
        assert_eq!(split(&[b"a\0bb\0ccc\0"], nul), ["a", "bb", "ccc"]);
    }

    #[test]
    fn joins_records_spanning_chunks() {
        assert_eq!(split(&[b"ab", b"c\0d", b"e", b"\0"], nul), ["abc", "de"]);
    }

    #[test]
    fn emits_trailing_record_without_delimiter() {
        assert_eq!(split(&[b"a\0tail"], nul), ["a", "tail"]);
    }

    #[test]
    fn keeps_empty_records() {
        assert_eq!(split(&[b"\0a\0\0"], nul), ["", "a", ""]);
    }

    #[test]
    fn handles_empty_stream() {
        assert!(split(&[], nul).is_empty());
        assert!(split(&[b""], nul).is_empty());
    }

    #[test]
    fn splits_progress_on_carriage_returns_and_newlines() {
        let lines = split(&[b"Receiving 10%\rReceiving 5", b"0%\r\ndone\n"], |b| {
            b == b'\r' || b == b'\n'
        });
        assert_eq!(lines, ["Receiving 10%", "Receiving 50%", "", "done"]);
    }

    #[test]
    fn preserves_multibyte_utf8_split_across_chunks() {
        let text = "ñandú\0".as_bytes();
        let (a, b) = text.split_at(2);
        assert_eq!(split(&[a, b], nul), ["ñandú"]);
    }
}

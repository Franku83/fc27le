//! Bounded byte-pattern matching with `??` wildcards.
//!
//! Patterns are written the way the existing research reports write them, for
//! example `48 89 5C 24 ?? 48 89 74 24 ??`. Matching is always performed against
//! a caller-supplied, already-bounded buffer.

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    text: String,
    values: Vec<u8>,
    wildcards: Vec<bool>,
}

impl Pattern {
    pub fn parse(input: &str) -> Result<Self> {
        let tokens: Vec<&str> = input.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(Error::InvalidPattern("pattern is empty".to_string()));
        }

        let mut values = Vec::with_capacity(tokens.len());
        let mut wildcards = Vec::with_capacity(tokens.len());

        for (index, token) in tokens.iter().enumerate() {
            match *token {
                "?" | "??" => {
                    values.push(0);
                    wildcards.push(true);
                }
                token => {
                    if token.len() != 2 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err(Error::InvalidPattern(format!(
                            "token {index} ({token:?}) is not a two-digit hex byte or a wildcard"
                        )));
                    }
                    let value = u8::from_str_radix(token, 16).map_err(|error| {
                        Error::InvalidPattern(format!("token {index} ({token:?}): {error}"))
                    })?;
                    values.push(value);
                    wildcards.push(false);
                }
            }
        }

        Ok(Self {
            text: input.split_whitespace().collect::<Vec<_>>().join(" "),
            values,
            wildcards,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Number of fixed (non-wildcard) bytes. Useful for a quick first-byte filter.
    pub fn fixed_bytes(&self) -> usize {
        self.wildcards.iter().filter(|wildcard| !**wildcard).count()
    }

    pub fn first_fixed_byte(&self) -> Option<(usize, u8)> {
        self.wildcards
            .iter()
            .zip(&self.values)
            .position(|(wildcard, _)| !*wildcard)
            .map(|index| (index, self.values[index]))
    }

    pub fn matches_at(&self, haystack: &[u8], offset: usize) -> bool {
        let Some(candidate) = haystack.get(offset..offset + self.len()) else {
            return false;
        };
        candidate
            .iter()
            .zip(self.wildcards.iter().zip(&self.values))
            .all(|(actual, (wildcard, expected))| *wildcard || actual == expected)
    }

    /// Return up to `limit` offsets at which the pattern matches.
    pub fn find_all(&self, haystack: &[u8], limit: usize) -> Vec<usize> {
        let mut matches = Vec::new();
        if self.len() > haystack.len() || limit == 0 {
            return matches;
        }

        let last_start = haystack.len() - self.len();
        let mut offset = 0;
        while offset <= last_start {
            if self.matches_at(haystack, offset) {
                matches.push(offset);
                if matches.len() >= limit {
                    break;
                }
            }
            offset += 1;
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_matches_wildcards() {
        let pattern = Pattern::parse("48 89 ?? 24").unwrap();
        assert_eq!(pattern.len(), 4);
        assert!(pattern.matches_at(&[0x48, 0x89, 0xaa, 0x24], 0));
        assert!(!pattern.matches_at(&[0x48, 0x88, 0xaa, 0x24], 0));
    }

    #[test]
    fn rejects_bad_tokens() {
        assert!(Pattern::parse("48 8G").is_err());
        assert!(Pattern::parse("4889").is_err());
        assert!(Pattern::parse("").is_err());
    }

    #[test]
    fn finds_all_offsets() {
        let pattern = Pattern::parse("aa bb").unwrap();
        let haystack = [0xaa, 0xbb, 0x00, 0xaa, 0xbb, 0xaa, 0xbb];
        assert_eq!(pattern.find_all(&haystack, 8), vec![0, 3, 5]);
        assert_eq!(pattern.find_all(&haystack, 2), vec![0, 3]);
    }

    #[test]
    fn fixed_byte_helpers() {
        let pattern = Pattern::parse("?? 12 34").unwrap();
        assert_eq!(pattern.fixed_bytes(), 2);
        assert_eq!(pattern.first_fixed_byte(), Some((1, 0x12)));
    }
}

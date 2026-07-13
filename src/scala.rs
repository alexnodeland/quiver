//! Scala (`.scl`) scale file parser for microtuning (Q146).
//!
//! Parses the [Scala scale file format](https://www.huygens-fokker.org/scala/scl_format.html):
//! a description line, a note count, and one pitch per line given either as a
//! ratio (`3/2`, or a bare integer `2` meaning `2/1`) or as a cents value (any
//! token containing a `.`, e.g. `701.955`). Lines beginning with `!` are comments
//! and are ignored. Parsing never panics; malformed input returns [`ScalaError`].
//!
//! Alloc-tier: this module is only compiled with the `alloc` feature (it produces
//! heap-allocated `Vec`/`String` scale data), mirroring the crate's other
//! alloc-gated conveniences.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use libm::Libm;

/// Error returned when a `.scl` file cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalaError {
    /// The file contained no non-comment content.
    Empty,
    /// The note-count line was missing.
    MissingCount,
    /// The note-count line was not a valid non-negative integer.
    BadCount(String),
    /// A pitch line could not be parsed (carries the offending token).
    BadPitch(String),
    /// The number of pitch lines did not match the declared count.
    CountMismatch {
        /// Declared count from the header.
        expected: usize,
        /// Pitch lines actually found.
        found: usize,
    },
}

impl core::fmt::Display for ScalaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ScalaError::Empty => write!(f, "scala: empty file"),
            ScalaError::MissingCount => write!(f, "scala: missing note count"),
            ScalaError::BadCount(s) => write!(f, "scala: invalid note count '{s}'"),
            ScalaError::BadPitch(s) => write!(f, "scala: invalid pitch '{s}'"),
            ScalaError::CountMismatch { expected, found } => {
                write!(f, "scala: expected {expected} pitches, found {found}")
            }
        }
    }
}

/// A parsed Scala scale.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalaScale {
    /// Free-text description from the first non-comment line.
    pub description: String,
    /// Pitches exactly as listed, converted to cents above `1/1`. The final entry
    /// is normally the period (e.g. `1200.0` for an octave).
    pub pitches_cents: Vec<f64>,
}

impl ScalaScale {
    /// Parse a `.scl` file body.
    pub fn parse(source: &str) -> Result<Self, ScalaError> {
        // Non-comment, meaningful lines. A `!` at the very start of a (trimmed)
        // line is a comment.
        let mut lines = source
            .lines()
            .map(|l| l.trim_end())
            .filter(|l| !l.trim_start().starts_with('!'));

        let description = lines.next().ok_or(ScalaError::Empty)?.trim().to_string();

        let count_line = lines.next().ok_or(ScalaError::MissingCount)?.trim();
        let count: usize = count_line
            .split_whitespace()
            .next()
            .ok_or_else(|| ScalaError::BadCount(count_line.to_string()))?
            .parse()
            .map_err(|_| ScalaError::BadCount(count_line.to_string()))?;

        let mut pitches_cents = Vec::with_capacity(count);
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            pitches_cents.push(parse_pitch(trimmed)?);
            if pitches_cents.len() == count {
                break;
            }
        }

        if pitches_cents.len() != count {
            return Err(ScalaError::CountMismatch {
                expected: count,
                found: pitches_cents.len(),
            });
        }

        Ok(Self {
            description,
            pitches_cents,
        })
    }

    /// The scale degrees reduced into a single octave `[0, 1200)` cents, sorted and
    /// de-duplicated, with the implicit `1/1` (0 cents) included.
    ///
    /// This is the form consumed by
    /// [`ScaleQuantizer::set_custom_scale`](crate::modules::ScaleQuantizer::set_custom_scale).
    pub fn degrees_within_octave(&self) -> Vec<f64> {
        let mut degrees: Vec<f64> = Vec::with_capacity(self.pitches_cents.len() + 1);
        degrees.push(0.0);
        for &c in &self.pitches_cents {
            // Octave-reduce into [0, 1200).
            let mut r = Libm::<f64>::fmod(c, 1200.0);
            if r < 0.0 {
                r += 1200.0;
            }
            degrees.push(r);
        }
        degrees.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        // De-duplicate near-equal degrees (e.g. a listed 1200.0 octave folds onto 0).
        degrees.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        degrees
    }
}

/// Parse a single Scala pitch token to cents above `1/1`.
///
/// A token containing `.` is a cents value; otherwise it is a ratio (`a/b`, or a
/// bare integer `n` = `n/1`). Any trailing content after the first whitespace is
/// ignored (Scala allows inline comments after the value).
fn parse_pitch(token: &str) -> Result<f64, ScalaError> {
    let first = token
        .split_whitespace()
        .next()
        .ok_or_else(|| ScalaError::BadPitch(token.to_string()))?;

    if first.contains('.') {
        // Cents value.
        first
            .parse::<f64>()
            .map_err(|_| ScalaError::BadPitch(first.to_string()))
    } else if let Some((num, den)) = first.split_once('/') {
        let n: f64 = num
            .trim()
            .parse()
            .map_err(|_| ScalaError::BadPitch(first.to_string()))?;
        let d: f64 = den
            .trim()
            .parse()
            .map_err(|_| ScalaError::BadPitch(first.to_string()))?;
        if n <= 0.0 || d <= 0.0 {
            return Err(ScalaError::BadPitch(first.to_string()));
        }
        Ok(1200.0 * Libm::<f64>::log2(n / d))
    } else {
        // Bare integer ratio n/1.
        let n: f64 = first
            .parse()
            .map_err(|_| ScalaError::BadPitch(first.to_string()))?;
        if n <= 0.0 {
            return Err(ScalaError::BadPitch(first.to_string()));
        }
        Ok(1200.0 * Libm::<f64>::log2(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWELVE_TET: &str = "\
! meantone.scl
12-tone equal temperament
 12
!
 100.0
 200.0
 300.0
 400.0
 500.0
 600.0
 700.0
 800.0
 900.0
 1000.0
 1100.0
 2/1
";

    // Ptolemy's intense diatonic (7-note just major).
    const JUST_MAJOR: &str = "\
! ptolemy.scl
Ptolemy intense diatonic
7
9/8
5/4
4/3
3/2
5/3
15/8
2/1
";

    #[test]
    fn test_parse_12tet() {
        let scale = ScalaScale::parse(TWELVE_TET).unwrap();
        assert_eq!(scale.description, "12-tone equal temperament");
        assert_eq!(scale.pitches_cents.len(), 12);
        // Last entry is the octave 2/1 = 1200 cents.
        assert!((scale.pitches_cents[11] - 1200.0).abs() < 1e-6);
        assert!((scale.pitches_cents[0] - 100.0).abs() < 1e-6);

        let degrees = scale.degrees_within_octave();
        // 12 unique degrees (2/1 folds onto 0).
        assert_eq!(degrees.len(), 12);
        assert!((degrees[0]).abs() < 1e-6);
        assert!((degrees[1] - 100.0).abs() < 1e-6);
        assert!((degrees[11] - 1100.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_just_major() {
        let scale = ScalaScale::parse(JUST_MAJOR).unwrap();
        assert_eq!(scale.pitches_cents.len(), 7);
        // 3/2 == 701.955 cents.
        assert!((scale.pitches_cents[3] - 701.955).abs() < 0.01);
        let degrees = scale.degrees_within_octave();
        assert_eq!(degrees.len(), 7); // 0, 9/8, 5/4, 4/3, 3/2, 5/3, 15/8
        assert!((degrees[0]).abs() < 1e-6);
        // 9/8 = 203.91 cents is the second degree.
        assert!((degrees[1] - 203.91).abs() < 0.01);
    }

    #[test]
    fn test_bare_integer_ratio() {
        // "3" means 3/1 = 1901.955 cents.
        assert!((parse_pitch("3").unwrap() - 1901.955).abs() < 0.01);
    }

    #[test]
    fn test_malformed_errors_do_not_panic() {
        assert_eq!(ScalaScale::parse(""), Err(ScalaError::Empty));
        assert_eq!(
            ScalaScale::parse("only a description"),
            Err(ScalaError::MissingCount)
        );
        assert!(matches!(
            ScalaScale::parse("desc\nnotanumber\n"),
            Err(ScalaError::BadCount(_))
        ));
        assert!(matches!(
            ScalaScale::parse("desc\n2\n100.0\n"),
            Err(ScalaError::CountMismatch {
                expected: 2,
                found: 1
            })
        ));
        assert!(matches!(
            ScalaScale::parse("desc\n1\nnonsense\n"),
            Err(ScalaError::BadPitch(_))
        ));
        // Negative / zero ratios are rejected.
        assert!(matches!(parse_pitch("0/1"), Err(ScalaError::BadPitch(_))));
        assert!(matches!(parse_pitch("-3/2"), Err(ScalaError::BadPitch(_))));
    }

    #[test]
    fn test_error_display() {
        let e = ScalaError::BadPitch("x".to_string());
        assert!(e.to_string().contains("x"));
    }
}

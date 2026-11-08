//! Seeded mutations for the engine contract tests: a small random number generator, the edits that turn an engine sample into a half-edited file, and shrinking a failing input.
//!
//! Dependency-free on purpose: a fuzzing crate (proptest, cargo-fuzz) would add many
//! dependencies, or a nightly toolchain, for what a few dozen lines do here. The same seed
//! always makes the same inputs, so a failure reported with its seed reproduces with
//! `just fuzz` (docs/testing.md).

/// A xorshift64* generator (Marsaglia's xorshift with Vigna's multiplier): tiny, fast, and
/// random enough to pick edits. Not for anything that must be unpredictable.
pub(super) struct Random(u64);

impl Random {
    /// The generator of one mutated input, from the run's `seed` and the input's numbers
    /// (`stream`: which engine; `index`: which of its inputs), so any input can be rebuilt on
    /// its own. `SplitMix64`'s mixing gives unrelated streams for neighbouring numbers.
    pub(super) fn for_input(seed: u64, stream: usize, index: usize) -> Self {
        let mut state = split_mix(seed);
        for number in [stream, index] {
            state = split_mix(state ^ u64::try_from(number).expect("a usize fits in a u64"));
        }
        // Xorshift maps 0 to 0 forever, so the state must not start there.
        Self(state.max(1))
    }

    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.0 = state;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A number in `0..bound`, for a positive `bound`. The modulo's bias is negligible for the
    /// small bounds used here.
    pub(super) fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound).expect("a usize fits in a u64");
        usize::try_from(self.next() % bound).expect("less than a usize bound")
    }

    /// One of `items`, which must not be empty.
    pub(super) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// A character boundary of `text`, from its start to its end.
    fn position(&mut self, text: &str) -> usize {
        text.floor_char_boundary(self.below(text.len() + 1))
    }
}

/// `SplitMix64`'s output function, which scrambles every bit of its input into every bit of its
/// output.
fn split_mix(value: u64) -> u64 {
    let mut mixed = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}

/// One edit of a file's text, as typing, deleting, pasting, or a cut-off write makes them.
#[derive(Debug, Clone, Copy)]
enum Edit {
    DeleteSpan,
    InsertToken,
    /// Inserts a token many times over, as generated or pathological files repeat one: deep
    /// nesting, long runs of unclosed brackets.
    RepeatToken,
    TruncateFront,
    TruncateBack,
    /// Pastes text from another engine sample, so each engine sees the others' syntax.
    SpliceOther,
    DuplicateLine,
}

const EDITS: [Edit; 7] = [
    Edit::DeleteSpan,
    Edit::InsertToken,
    Edit::RepeatToken,
    Edit::TruncateFront,
    Edit::TruncateBack,
    Edit::SpliceOther,
    Edit::DuplicateLine,
];

/// The most edits one mutated input gets.
const MAX_EDITS: usize = 3;

/// The longest span an edit deletes or pastes, in bytes: about a line, so the result stays
/// close to the sample, as a half-edited file stays close to the file it was.
const MAX_SPAN_BYTES: usize = 80;

/// The most times [`Edit::RepeatToken`] repeats its token: past the 254 levels of nesting that
/// once aborted the YAML grammar's scanner, while each input still outlines in milliseconds.
const MAX_REPEATS: usize = 300;

/// What an [`Edit`] inserts: the tokens it picks from, and the texts it pastes from.
pub(super) struct Material<'a> {
    pub(super) tokens: &'a [&'a str],
    pub(super) others: &'a [String],
}

/// Applies one to [`MAX_EDITS`] random edits to `text`.
pub(super) fn mutate(text: &mut String, material: &Material<'_>, random: &mut Random) {
    for _ in 0..=random.below(MAX_EDITS) {
        apply(*random.pick(&EDITS), text, material, random);
    }
}

fn apply(edit: Edit, text: &mut String, material: &Material<'_>, random: &mut Random) {
    let at = random.position(text);
    match edit {
        Edit::DeleteSpan => {
            let end = text.ceil_char_boundary(at + 1 + random.below(MAX_SPAN_BYTES));
            text.replace_range(at..end, "");
        }
        Edit::InsertToken => text.insert_str(at, random.pick(material.tokens)),
        Edit::RepeatToken => {
            let token = random.pick(material.tokens);
            text.insert_str(at, &token.repeat(1 + random.below(MAX_REPEATS)));
        }
        Edit::TruncateFront => {
            text.drain(..at);
        }
        Edit::TruncateBack => text.truncate(at),
        Edit::SpliceOther => {
            let other = random.pick(material.others);
            let start = random.position(other);
            let end = other.ceil_char_boundary(start + 1 + random.below(MAX_SPAN_BYTES));
            text.insert_str(at, &other[start..end]);
        }
        Edit::DuplicateLine => {
            let line_start = text[..at].rfind('\n').map_or(0, |newline| newline + 1);
            let line_end = text[at..]
                .find('\n')
                .map_or(text.len(), |newline| at + newline + 1);
            let line = text[line_start..line_end].to_owned();
            text.insert_str(line_end, &line);
        }
    }
}

/// A small input that still fails, found from `input` by deleting parts of it while
/// `still_fails`: whole lines first, then chunks of half its length, a quarter, and so on down
/// to single characters, until no deletion still fails. This is a greedy form of delta
/// debugging (Zeller's ddmin); it may end at a different failure than it started with.
pub(super) fn shrink(input: &str, still_fails: impl Fn(&str) -> bool) -> String {
    let mut current = input.to_owned();
    loop {
        let before = current.len();
        delete_lines(&mut current, &still_fails);
        let mut chunk = current.len().div_ceil(2);
        while chunk > 0 {
            delete_chunks(&mut current, chunk, &still_fails);
            chunk /= 2;
        }
        if current.len() == before {
            return current;
        }
    }
}

/// Deletes each line of `text` whose deletion still fails.
fn delete_lines(text: &mut String, still_fails: &impl Fn(&str) -> bool) {
    let mut start = 0;
    while start < text.len() {
        let end = text[start..]
            .find('\n')
            .map_or(text.len(), |newline| start + newline + 1);
        if !try_delete(text, start, end, still_fails) {
            start = end;
        }
    }
}

/// Deletes each run of `chunk` bytes (widened to whole characters) whose deletion still fails.
fn delete_chunks(text: &mut String, chunk: usize, still_fails: &impl Fn(&str) -> bool) {
    let mut start = 0;
    while start < text.len() {
        let end = text.ceil_char_boundary(start + chunk);
        if !try_delete(text, start, end, still_fails) {
            start = end;
        }
    }
}

/// Deletes `start..end` from `text` if the result still fails, and says whether it did.
fn try_delete(
    text: &mut String,
    start: usize,
    end: usize,
    still_fails: &impl Fn(&str) -> bool,
) -> bool {
    let candidate = [&text[..start], &text[end..]].concat();
    let fails = still_fails(&candidate);
    if fails {
        *text = candidate;
    }
    fails
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_makes_the_same_mutations() {
        let others = ["pasted text\n".to_owned()];
        let material = Material {
            tokens: &["{", "\n"],
            others: &others,
        };
        let mutated = |seed| {
            let mut text = "fn a() {}\nfn b() {}\n".to_owned();
            mutate(&mut text, &material, &mut Random::for_input(seed, 0, 7));
            text
        };
        assert_eq!(mutated(1), mutated(1));
        let differing = (2..20).filter(|&seed| mutated(seed) != mutated(1)).count();
        assert!(differing > 10, "seeds barely change the mutations");
    }

    #[test]
    fn shrink_keeps_only_what_the_failure_needs() {
        let input = "first line\nsecond { line\nthird line é\n";
        assert_eq!(shrink(input, |text| text.contains('{')), "{");
        assert_eq!(shrink(input, |text| text.contains("é\n")), "é\n");
    }
}

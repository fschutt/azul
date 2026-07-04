pub trait SyllableChar {
    fn char(&self) -> char;
}

/// Matches nothing, consumes nothing, always succeeds
pub fn match_unit<T: SyllableChar>() -> impl Fn(&[T]) -> Option<usize> {
    |_cs: &[T]| Some(0)
}

/// Matches against a single character
pub fn match_one<T: SyllableChar>(f: impl Fn(char) -> bool) -> impl Fn(&[T]) -> Option<usize> {
    move |cs: &[T]| match cs.first() {
        Some(c) if f(c.char()) => Some(1),
        _ => None,
    }
}

/// Succeeds if the input is non empty
pub fn match_nonempty<T: SyllableChar>(
    f: impl Fn(&[T]) -> Option<usize>,
) -> impl Fn(&[T]) -> Option<usize> {
    move |cs: &[T]| f(cs).filter(|&n| n > 0)
}

/// Succeeds if `f` succeeds otherwise consumes nothing
pub fn match_optional<T: SyllableChar>(
    f: impl Fn(&[T]) -> Option<usize>,
) -> impl Fn(&[T]) -> Option<usize> {
    move |cs: &[T]| f(cs).or(Some(0))
}

/// `f? g`: matches either `g` or `f g`
pub fn match_optional_seq<T: SyllableChar>(
    f: impl Fn(&[T]) -> Option<usize>,
    g: impl Fn(&[T]) -> Option<usize>,
) -> impl Fn(&[T]) -> Option<usize> {
    move |cs: &[T]| match_either(&g, match_seq(&f, &g))(cs)
}

#[allow(dead_code)]
pub fn match_repeat_num<T: SyllableChar>(
    num: usize,
    f: impl Fn(&[T]) -> Option<usize>,
) -> impl Fn(&[T]) -> Option<usize> {
    move |mut cs: &[T]| {
        let mut total = 0;
        for _ in 0..num {
            let n = f(cs)?;
            total += n;
            cs = &cs[n..];
        }
        Some(total)
    }
}

/// Match up to `max` instances of `f`, followed by `g`
pub fn match_repeat_upto<T: SyllableChar>(
    max: usize,
    f: impl Fn(&[T]) -> Option<usize>,
    g: impl Fn(&[T]) -> Option<usize>,
) -> impl Fn(&[T]) -> Option<usize> {
    move |mut cs: &[T]| {
        // Initial case: zero f matches
        let mut best = g(cs);
        let mut nf = 0;
        for _ in 0..max {
            // Match up to max instances of f
            if let Some(n) = f(cs) {
                nf += n;
                cs = &cs[n..];
                // If f is followed by g update matching range
                if let Some(ng) = g(cs) {
                    best = Some(nf + ng);
                }
            } else {
                break;
            }
        }
        best
    }
}

/// Match `f1` followed by `f2`.
///
/// Fails if `f1` or `f2` fail.
pub fn match_seq<T: SyllableChar>(
    f1: impl Fn(&[T]) -> Option<usize>,
    f2: impl Fn(&[T]) -> Option<usize>,
) -> impl Fn(&[T]) -> Option<usize> {
    move |cs: &[T]| {
        let n1 = f1(cs)?;
        let n2 = f2(&cs[n1..])?;
        Some(n1 + n2)
    }
}

/// Matches whichever of `f1` or `f2` match the most input.
///
/// Uses `f2`'s match if they match the same input
pub fn match_either<T: SyllableChar>(
    f1: impl Fn(&[T]) -> Option<usize>,
    f2: impl Fn(&[T]) -> Option<usize>,
) -> impl Fn(&[T]) -> Option<usize> {
    move |cs: &[T]| {
        let n1 = f1(cs);
        let n2 = f2(cs);
        std::cmp::max(n1, n2)
    }
}

pub fn match_either_seq<T: SyllableChar>(
    f1: impl Fn(&[T]) -> Option<usize>,
    f2: impl Fn(&[T]) -> Option<usize>,
    g: impl Fn(&[T]) -> Option<usize>,
) -> impl Fn(&[T]) -> Option<usize> {
    move |cs: &[T]| {
        let n1 = match_seq(&f1, &g)(cs);
        let n2 = match_seq(&f2, &g)(cs);
        std::cmp::max(n1, n2)
    }
}

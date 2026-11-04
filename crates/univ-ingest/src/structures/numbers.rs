//! Numbers in text: the standalone numbers of a sentence, and the value a number written as
//! published stands for, such as `14h50m` or `-50°02′`.

use univ_model::units::Unit;

/// The standalone numbers of `text`, with their signs.
///
/// A number is a whole token: not inside a longer number, and not glued to letters, as in
/// `Lacerta-2.4`, `U1.11`, `DR17`, `2dF` or `G069.39+68.05`, nor part of a power such as
/// `10^5`. Thousands separators (`16,000`, `18 000`, or a thin space) are read, a unicode minus
/// is a minus, and a sign must touch its digits (`±4,500` and `(+0.003, -0.007)` give 4500,
/// 0.003 and -0.007).
#[must_use]
pub fn numbers(text: &str) -> Vec<f64> {
    located(text).into_iter().map(|(_, x)| x).collect()
}

/// Words after which a number names a part of a paper or a catalogued object rather than
/// stating a value.
const NAMING: [&str; 22] = [
    "table", "tables", "figure", "figures", "fig.", "figs.", "section", "sec.", "eq.", "equation",
    "footnote", "page", "abell", "aco", "ngc", "ugc", "pgc", "hip", "hd", "hr", "tic", "source",
];

/// The standalone numbers of free text that state values: [`numbers`] without those that name
/// a part of a paper or a catalogued object, as in `Table 1`, `Figure 2` or `Abell 2631`.
#[must_use]
pub fn stated_numbers(text: &str) -> Vec<f64> {
    let chars: Vec<char> = text.chars().collect();
    located(text)
        .into_iter()
        .filter(|(at, _)| {
            let before: String = chars[..*at].iter().collect();
            let word = before
                .trim_end()
                .rsplit(char::is_whitespace)
                .next()
                .unwrap_or("")
                .trim_start_matches(['(', '['])
                .to_lowercase();
            !NAMING.contains(&word.as_str())
        })
        .map(|(_, x)| x)
        .collect()
}

/// The standalone numbers of `text`, each with the index of its first digit in its chars.
fn located(text: &str) -> Vec<(usize, f64)> {
    let chars: Vec<char> = text
        .chars()
        .map(|c| if c == '\u{2212}' { '-' } else { c })
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let before = i.checked_sub(1).map(|p| chars[p]);
        if before.is_some_and(continues) {
            i = past_word(&chars, i);
            continue;
        }
        let (end, digits) = read_number(&chars, i);
        // A sign touches its digits and follows no digit: `300-400` is a range.
        let signed = before.is_some_and(|c| matches!(c, '-' | '+'))
            && !i.checked_sub(2).is_some_and(|p| chars[p].is_ascii_digit());
        let glued_before =
            before.is_some_and(|c| matches!(c, '-' | '+' | '/')) && sign_is_glued(&chars, i - 1);
        // An asymmetric error printed after its value, `168^{+27}_{-15}`, is no power.
        let error_follows = asymmetric_error_at(&chars, end);
        let glued_after =
            !error_follows && chars.get(end).is_some_and(|c| continues(*c) && *c != '.');
        if !glued_before
            && !glued_after
            && let Ok(x) = digits.parse::<f64>()
        {
            out.push((i, if signed && before == Some('-') { -x } else { x }));
        }
        i = past_word(&chars, end);
    }
    out
}

/// Whether an asymmetric error starts at `at`, as in `168^{+27}_{-15}`: a signed superscript
/// and a signed subscript, in either order, and not a lone power such as `10^{-3}`.
fn asymmetric_error_at(chars: &[char], at: usize) -> bool {
    let group = |at: usize| -> Option<(char, usize)> {
        let mark = *chars.get(at)?;
        let opens = matches!(mark, '^' | '_')
            && chars.get(at + 1) == Some(&'{')
            && matches!(chars.get(at + 2), Some('+' | '-'));
        let close = chars.get(at..)?.iter().position(|c| *c == '}')? + at;
        opens.then_some((mark, close + 1))
    };
    group(at)
        .and_then(|(first, next)| group(next).filter(|(second, _)| *second != first))
        .is_some()
}

/// Whether a number starting after `c` would continue a token: a letter, a digit, a decimal
/// point or a power.
fn continues(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '^' | '_')
}

/// Whether the sign or slash at `p` belongs to a name, as in `Lacerta-2.4` or `A2147/2152`, or
/// follows a power, as in `h^-1`: a letter or `^` before it, through any digits, points, signs
/// and slashes.
fn sign_is_glued(chars: &[char], p: usize) -> bool {
    let mut q = p;
    while q > 0 {
        let c = chars[q - 1];
        if c.is_alphabetic() || c == '^' || c == '_' {
            return true;
        }
        if !(c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | '/')) {
            return false;
        }
        q -= 1;
    }
    false
}

/// Past the letters, digits and points of the token that runs on at `i`.
fn past_word(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && continues(chars[i]) {
        i += 1;
    }
    i
}

/// The number starting at `i`, without its thousands separators, and where it ends.
fn read_number(chars: &[char], i: usize) -> (usize, String) {
    let mut digits = String::new();
    let mut j = i;
    while j < chars.len() && chars[j].is_ascii_digit() {
        digits.push(chars[j]);
        j += 1;
    }
    // A thousands group: a separator and exactly three digits, after a lead of one to three.
    let mut grouped = digits.len() <= 3;
    while grouped
        && matches!(chars.get(j), Some(',' | ' ' | '\u{2009}' | '\u{202F}'))
        && chars[j + 1..]
            .iter()
            .take(3)
            .filter(|c| c.is_ascii_digit())
            .count()
            == 3
        && !chars.get(j + 4).is_some_and(char::is_ascii_digit)
    {
        digits.extend(&chars[j + 1..j + 4]);
        j += 4;
        grouped = true;
    }
    if chars.get(j) == Some(&'.') && chars.get(j + 1).is_some_and(char::is_ascii_digit) {
        digits.push('.');
        j += 1;
        while j < chars.len() && chars[j].is_ascii_digit() {
            digits.push(chars[j]);
            j += 1;
        }
    }
    (j, digits)
}

/// Whether `text` holds `x` as a standalone number.
#[must_use]
pub fn has_number(text: &str, x: f64) -> bool {
    numbers(text).contains(&x)
}

/// Whether `text` holds `x` as a standalone number of either sign, as an error is printed.
#[must_use]
#[expect(
    clippy::float_cmp,
    reason = "the same number as written, not a near one"
)]
pub fn has_magnitude(text: &str, x: f64) -> bool {
    numbers(text).iter().any(|n| n.abs() == x.abs())
}

/// Whether `text` holds `needle` as a whole: not inside a longer number or word.
#[must_use]
pub fn contains_whole(text: &str, needle: &str) -> bool {
    let (text, needle) = (
        text.replace('\u{2212}', "-"),
        needle.replace('\u{2212}', "-"),
    );
    let word = |c: char| c.is_alphanumeric() || c == '.';
    let (first, last) = (needle.chars().next(), needle.chars().next_back());
    text.match_indices(&needle).any(|(at, _)| {
        let rest = &text[at + needle.len()..];
        let before = text[..at].chars().next_back();
        let mut after = rest.chars();
        let next = after.next();
        // A point after the needle ends a sentence unless a digit follows it.
        let next_joins = next.is_some_and(|c| {
            c.is_alphanumeric() || (c == '.' && after.next().is_some_and(|d| d.is_ascii_digit()))
        });
        let joined_before = before.is_some_and(word) && first.is_some_and(word);
        let joined_after = next_joins && last.is_some_and(word);
        !joined_before && !joined_after
    })
}

/// A number as published: its value, half a step of its last digit, and what it is in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Published {
    pub value: f64,
    pub half_step: f64,
    pub unit: Option<Unit>,
    /// Whether it stands for the value and its negative alike: a bound on a magnitude, such as
    /// `|b| < 10°`, the bound of both ends of a range about zero.
    pub either_sign: bool,
}

/// The number `text` stands for: a decimal (`0.060`, `~1 Gpc`), hours of right ascension
/// (`16.7h`, `13h01m`, `α = 03h19m`), degrees with minutes and seconds (`-50°02′`, `38◦45′`),
/// or a celestial pole (latitude ±90°). A label up to `=` and a leading `~` are skipped. A bound
/// on a magnitude, `|b| < 10°` or `|b| ≤ 10°`, stands for 10° and −10° alike.
#[must_use]
pub fn published(text: &str) -> Option<Published> {
    if let Some(bound) = text
        .trim()
        .strip_prefix('|')
        .and_then(|t| t.split_once('|'))
        .map(|(_, rest)| rest.trim())
        .and_then(|rest| ["≤", "<=", "<"].iter().find_map(|op| rest.strip_prefix(op)))
    {
        return published(bound).map(|p| Published {
            either_sign: true,
            ..p
        });
    }
    let lower = text.to_lowercase();
    if lower.contains("pole") {
        let value = match (lower.contains("south"), lower.contains("north")) {
            (true, false) => -90.0,
            (false, true) => 90.0,
            _ => return None,
        };
        return Some(Published {
            value,
            half_step: 0.0,
            unit: Some(Unit::Degree),
            either_sign: false,
        });
    }
    let text = text.rsplit_once('=').map_or(text, |(_, t)| t);
    let text = text
        .trim()
        .trim_start_matches(['~', '≈'])
        .trim()
        .replace('\u{2212}', "-");
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(&text)),
    };
    let signed = |p: Published| Published {
        value: if negative { -p.value } else { p.value },
        ..p
    };
    if let Some(p) = sexagesimal(body, &['h', 'm', 's'], 15.0) {
        return Some(signed(p));
    }
    if let Some(p) = sexagesimal(body, &['°', '′', '″'], 1.0)
        .or_else(|| sexagesimal(&body.replace('◦', "°"), &['°', '′', '″'], 1.0))
    {
        return Some(signed(p));
    }
    let (number, unit) = body
        .split_once(' ')
        .map_or((body, ""), |(n, u)| (n, u.trim()));
    let unit = match unit {
        "" => None,
        "Gpc" => Some((Unit::Megaparsec, 1.0e3)),
        "Mpc" => Some((Unit::Megaparsec, 1.0)),
        "kpc" => Some((Unit::Kiloparsec, 1.0)),
        _ => return None,
    };
    let (value, half_step) = decimal(&number.replace(',', ""))?;
    let (unit, factor) = unit.map_or((None, 1.0), |(u, f)| (Some(u), f));
    Some(signed(Published {
        value: value * factor,
        half_step: half_step * factor,
        unit,
        either_sign: false,
    }))
}

/// A plain decimal and half a step of its last digit.
fn decimal(text: &str) -> Option<(f64, f64)> {
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let value = text.parse::<f64>().ok()?;
    let places = text.split_once('.').map_or(0, |(_, f)| f.len());
    let places = i32::try_from(places).ok()?;
    Some((value, 0.5 * 10f64.powi(-places)))
}

/// `a<u0>b<u1>c<u2>` with any trailing parts left out and the last part decimal, such as
/// `13h01m` or `16.7h`, in degrees: `scale` degrees to the first unit.
fn sexagesimal(text: &str, units: &[char; 3], scale: f64) -> Option<Published> {
    let mut rest = text.trim();
    let (mut value, mut half_step) = (0.0, 0.0);
    let mut parts = 0;
    for (n, unit) in units.iter().enumerate() {
        let Some((part, after)) = rest.split_once(*unit) else {
            break;
        };
        let (x, half) = decimal(part.trim())?;
        let divisor = 60f64.powi(i32::try_from(n).ok()?);
        value += x / divisor;
        half_step = half / divisor;
        rest = after.trim();
        parts += 1;
    }
    (parts > 0 && rest.is_empty()).then_some(Published {
        value: value * scale,
        half_step: half_step * scale,
        unit: Some(Unit::Degree),
        either_sign: false,
    })
}

/// Whether a value in `unit` equals what `published` stands for, within the rounding of its
/// last digit. A published number without a unit is in the value's unit.
#[must_use]
pub fn consistent(value: f64, unit: Unit, published: &Published) -> bool {
    let in_published = match published.unit {
        None => Some(value),
        Some(p) if p.dimension() == unit.dimension() => unit.factor_to(p).ok().map(|f| value * f),
        Some(_) => None,
    };
    let close = |v: f64| (v - published.value).abs() <= published.half_step * (1.0 + 1e-9) + 1e-12;
    in_published.is_some_and(|v| close(v) || (published.either_sign && close(-v)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_whole_tokens_with_their_separators_and_signs() {
        assert_eq!(numbers("about 1650 km/s"), [1650.0]);
        assert!(!has_number("about 1650 km/s", 165.0));
        assert_eq!(numbers("16,000±4,500 km s-1"), [16000.0, 4500.0]);
        assert_eq!(
            numbers("cz ~ 18 000 km s-1 in 22 2dF fields"),
            [18000.0, 22.0]
        );
        assert_eq!(
            numbers("[11,000, \u{2212}6,000, 10,000]"),
            [11000.0, -6000.0, 10000.0]
        );
        assert_eq!(
            numbers("z = 0.068 (+0.003, -0.007)"),
            [0.068, 0.003, -0.007]
        );
        assert_eq!(
            numbers("Lacerta-2.4, U1.11, DR17, 2dF, h^-1 Mpc, 10^5"),
            [] as [f64; 0]
        );
        assert_eq!(numbers("G069.39+68.05 (z = 0.763)"), [0.763]);
        assert_eq!(
            numbers("a distance of $168^{+27}_{-15}$ pc"),
            [168.0, 27.0, -15.0]
        );
        assert!(!numbers("a contrast of 10^{-3} or 10^{+5}").contains(&10.0));
        assert_eq!(numbers("A2147/2152 cover 1/8 of it"), [1.0, 8.0]);
        assert_eq!(
            numbers("~300 - 400 Mpc, 12°, 5200,3000,4500"),
            [300.0, 400.0, 12.0, 5200.0, 3000.0, 4500.0]
        );
        assert_eq!(numbers("z = 0.14, 0.26 and 0.30."), [0.14, 0.26, 0.3]);
        assert_eq!(
            stated_numbers("Table 1 of the paper lists 38 quasars (Fig. 2)."),
            [38.0]
        );
        assert_eq!(
            stated_numbers("HIP 9640 is HD 12533 and Gaia source 4933453324590295000: 8.30 mas"),
            [8.3]
        );
    }

    #[test]
    fn a_published_form_is_read_with_the_rounding_of_its_last_digit() {
        let p = published("16.7h").unwrap();
        assert!((p.value - 250.5).abs() < 1e-12 && (p.half_step - 0.75).abs() < 1e-12);
        let p = published("α = 03h19m").unwrap();
        assert!((p.value - 49.75).abs() < 1e-12);
        let p = published("δ = -50°02′").unwrap();
        assert!((p.value + 50.0 + 2.0 / 60.0).abs() < 1e-12);
        assert!((published("38◦45′").unwrap().value - 38.75).abs() < 1e-12);
        let p = published("~1 Gpc").unwrap();
        assert_eq!(
            (p.value, p.half_step, p.unit),
            (1000.0, 500.0, Some(Unit::Megaparsec))
        );
        assert_eq!(published("celestial South Pole").unwrap().value, -90.0);
        assert!(consistent(
            0.06,
            Unit::Dimensionless,
            &published("0.060").unwrap()
        ));
        assert!(!consistent(
            9999.0,
            Unit::Dimensionless,
            &published("1650").unwrap()
        ));
        assert!(consistent(135.0, Unit::Degree, &published("9h").unwrap()));
        assert!(contains_whole("R.A. = 13h25m.", "13h25m"));
        assert!(!contains_whole("about 1650 km/s", "165"));
        assert!(!contains_whole("about 2165 km/s", "165"));
        assert_eq!(published("2 x 10^5"), None);
    }

    #[test]
    fn a_bound_on_a_magnitude_stands_for_both_ends_of_its_range() {
        for text in ["|b| < 10°", "|b| ≤ 10°"] {
            let p = published(text).unwrap();
            assert!(consistent(10.0, Unit::Degree, &p), "{text}");
            assert!(consistent(-10.0, Unit::Degree, &p), "{text}");
            assert!(!consistent(-9.0, Unit::Degree, &p), "{text}");
        }
        // A plain number keeps its sign.
        let ten = published("10°").unwrap();
        assert!(!consistent(-10.0, Unit::Degree, &ten));
    }
}

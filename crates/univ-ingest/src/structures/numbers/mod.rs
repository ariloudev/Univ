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
    every_number(text).contains(&x)
}

/// The numbers `text` states as a citation shows them: [`numbers`], those LaTeX glues to a
/// macro (`$2.0\pm0.2$`, `\narreq1.4`), and those written with a power of ten
/// ([`scaled_numbers`]).
fn every_number(text: &str) -> Vec<f64> {
    let mut out = numbers(text);
    if text.contains('\\') {
        out.extend(numbers(&macros_apart(text)));
    }
    out.extend(scaled_numbers(text));
    out
}

/// `text` with a space after each LaTeX macro's name, such as `\pm` or `\narreq`, so that a
/// number the macro is glued to stands as a number.
fn macros_apart(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut in_name = false;
    for c in text.chars() {
        if in_name && !c.is_ascii_alphabetic() {
            out.push(' ');
            in_name = false;
        }
        if c == '\\' {
            in_name = true;
        }
        out.push(c);
    }
    out
}

/// The numbers `text` writes with a power of ten, as papers write masses: `A × 10^C`,
/// `A\times 10^{C}`, and `A ± B × 10^C`, which states both `A·10^C` and `B·10^C`, `\pm` or
/// `{\small\pm}` standing for the plus-minus sign as LaTeX writes it.
#[must_use]
pub fn scaled_numbers(text: &str) -> Vec<f64> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices("10^") {
        let rest = &text[at + 3..];
        let exponent: String = match rest.strip_prefix('{') {
            Some(inner) => inner
                .split('}')
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned(),
            None => rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '\u{2212}'))
                .collect(),
        };
        let exponent = exponent.replace('\u{2212}', "-").replace('+', "");
        if exponent.parse::<i32>().is_err() {
            continue;
        }
        let Some(before) = ["\\times", "×", "\\cdot", "·"]
            .iter()
            .find_map(|m| text[..at].trim_end().strip_suffix(m))
        else {
            continue;
        };
        let Some((b, rest)) = trailing_number(before.trim_end()) else {
            continue;
        };
        let scaled = |mantissa: &str| format!("{mantissa}e{exponent}").parse::<f64>().ok();
        // A value before a plus-minus sign shares the power of the error after it: the value
        // first, as written.
        let rest = rest.trim_end().trim_end_matches('}').trim_end();
        let value = ["\\pm", "±"]
            .iter()
            .find_map(|m| rest.strip_suffix(m))
            .map(|before| {
                before
                    .trim_end()
                    .trim_end_matches("\\small")
                    .trim_end()
                    .trim_end_matches('{')
                    .trim_end()
            })
            .and_then(trailing_number)
            .and_then(|(a, _)| scaled(a));
        out.extend(value);
        out.extend(scaled(b));
    }
    out
}

/// The number that ends `text`, digits and a point, and what comes before it.
fn trailing_number(text: &str) -> Option<(&str, &str)> {
    let start = text
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit() || *c == '.')
        .last()
        .map(|(i, _)| i)?;
    let number = &text[start..];
    number
        .chars()
        .any(|c| c.is_ascii_digit())
        .then_some((number, &text[..start]))
}

/// Whether `text` holds `x` as a standalone number of either sign, as an error is printed.
#[must_use]
#[expect(
    clippy::float_cmp,
    reason = "the same number as written, not a near one"
)]
pub fn has_magnitude(text: &str, x: f64) -> bool {
    every_number(text).into_iter().any(|n| n.abs() == x.abs())
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

/// The figures of `text` in order, as a value printed in several parts is compared with a cell
/// that prints it otherwise: each run of digits and points with the minus before it,
/// whatever letters or marks part them: `18h12m02.6822s` and `18 12 02.6822` give 18, 12 and
/// 02.6822, `−18°40′25.002″` gives −18, 40 and 25.002.
#[must_use]
pub fn figures(text: &str) -> Vec<String> {
    let text = text.replace('\u{2212}', "-");
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut minus = false;
    // A point that ends a figure, such as a full stop after it, is not part of it.
    let end = |current: &mut String, out: &mut Vec<String>| {
        let figure = current.trim_end_matches('.');
        if !figure.is_empty() && figure != "-" {
            out.push(figure.to_owned());
        }
        current.clear();
    };
    for c in text.chars() {
        if c.is_ascii_digit() || (c == '.' && !current.is_empty()) {
            if current.is_empty() && minus {
                current.push('-');
            }
            current.push(c);
        } else {
            end(&mut current, &mut out);
            minus = c == '-';
        }
    }
    end(&mut current, &mut out);
    out
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
mod tests;

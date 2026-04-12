use bigdecimal::{num_bigint::BigUint, BigDecimal, Zero};
use ratatui::{
    style::Stylize,
    text::{Line, Span},
};
use std::cmp::min;

/// Number formatting. Takes into consideration the actual width of the display,
/// the required base and whether the user wants additional spacing between groups
/// of digits for readability.
pub fn format_number<'b>(n: &BigDecimal, width: u64, separator: bool, base: u32) -> Line<'b> {
    if base != 10 {
        format_number_in_base(n, width, separator, base)
    } else {
        format_number_in_base_10(n, width, separator)
    }
}

/// Format in base 10: unlike other bases, actual digits after the decimal point are shown,
/// truncated with `~` only when necessary.
fn format_number_in_base_10<'b>(n: &BigDecimal, width: u64, separator: bool) -> Line<'b> {
    let repr = n.normalized().to_plain_string();
    let total = repr.len() as u64;
    // Trivial case: the representation already fits the display.
    if total <= width {
        if !separator {
            return Line::raw(repr);
        }
        let separated_repr = add_separators(&repr, 3);
        // It's probably still better to remove the separators than to switch to
        // extended representation if the size is a bit tight.
        if separated_repr.len() as u64 <= width {
            return Line::raw(separated_repr);
        }
        return Line::raw(repr);
    }

    let digits_after_dot = if let Some(idx) = repr.find('.') {
        (total - idx as u64 - 1) as i64
    } else {
        0
    };
    // digits_to_dot: length of the string up to and including the '.', e.g. 9 for "12345678."
    let digits_to_dot = total as i64 - digits_after_dot;
    let digits_before_dot = digits_to_dot - if digits_after_dot > 0 { 1 } else { 0 };

    // Simple case: the integer part fits; just truncate decimal digits and append '~'.
    let extra_precision = width as i64 - digits_to_dot - 1;
    if digits_after_dot > 0 && extra_precision >= 0 {
        return Line::from(vec![
            Span::from(repr[..(digits_to_dot + extra_precision) as usize].to_string()),
            Span::from("~").yellow(),
        ]);
    }

    // Complex case: show [sign][MSB]~<magnitude>~[LSB][.decimal?] so that both the
    // order-of-magnitude and the fine detail are visible.
    let sign_len = if n < &BigDecimal::zero() { 1i64 } else { 0i64 };
    let mut budget = width as i64 - sign_len;
    let mut parts = 2i64;
    if digits_after_dot > 0 {
        parts += 1; // LSB section straddles the decimal point
        budget -= 1; // the '.' itself occupies one character
    }
    let pow = format!("~{}~", digits_before_dot - sign_len);
    budget -= pow.len() as i64;

    let Some((msb, lsb)) = split_budget(budget, parts) else {
        return Line::from(Span::from("~").red());
    };
    let lsb_str = if digits_after_dot > 0 {
        &repr[digits_to_dot as usize - lsb - 1..min(digits_to_dot as usize + lsb, total as usize)]
    } else {
        &repr[total as usize - lsb..]
    };
    assemble_truncated(repr[..msb + sign_len as usize].to_string(), pow, lsb_str, vec![])
}

/// Format in an arbitrary base: the fractional part (if any) is always shown as `.~` because
/// the base conversion only handles the integer portion.
fn format_number_in_base<'b>(n: &BigDecimal, width: u64, separator: bool, base: u32) -> Line<'b> {
    let repr = n.normalized().to_plain_string();
    let (sign, unsigned_repr) = if let Some(s) = repr.strip_prefix('-') {
        ("-", s)
    } else {
        ("", repr.as_str())
    };
    let (int_str, has_fraction) = if let Some(idx) = unsigned_repr.find('.') {
        (&unsigned_repr[..idx], true)
    } else {
        (unsigned_repr, false)
    };

    // Parse the decimal integer string and re-encode in the target base.
    let int_val: BigUint = int_str.parse().unwrap_or_default();
    let base_repr = int_val.to_str_radix(base);

    let group_size: usize = match base {
        2 | 16 => 4,
        _ => 3,
    };

    let sign_len = sign.len() as u64;
    let base_len = base_repr.len() as u64;
    let frac_extra: u64 = if has_fraction { 2 } else { 0 }; // for the trailing '.~'
    let total = sign_len + base_len + frac_extra;

    let trailing_tilde = || Span::from(".~").yellow();

    if total <= width {
        let base_with_sign = format!("{}{}", sign, base_repr);
        if separator {
            let separated = add_separators(&base_with_sign, group_size);
            if separated.len() as u64 + frac_extra <= width {
                if has_fraction {
                    return Line::from(vec![Span::raw(separated), trailing_tilde()]);
                }
                return Line::raw(separated);
            }
        }
        if has_fraction {
            return Line::from(vec![Span::raw(base_with_sign), trailing_tilde()]);
        }
        return Line::raw(base_with_sign);
    }

    // Truncation: [sign][MSB]~<digit_count>~[LSB][.~]
    let pow = format!("~{}~", base_len);
    let budget = width as i64 - sign_len as i64 - pow.len() as i64 - frac_extra as i64;

    let Some((msb, lsb)) = split_budget(budget, 2) else {
        return Line::from(Span::from("~").red());
    };
    let suffix = if has_fraction { vec![trailing_tilde()] } else { vec![] };
    assemble_truncated(
        format!("{}{}", sign, &base_repr[..msb]),
        pow,
        &base_repr[base_repr.len() - lsb..],
        suffix,
    )
}

/// Split a display budget into MSB and LSB character counts.
/// MSB receives any remainder so it shows the most significant information.
/// Returns `None` when the budget is too small to fill all parts.
fn split_budget(budget: i64, parts: i64) -> Option<(usize, usize)> {
    if budget < parts {
        return None;
    }
    Some(((budget / parts + budget % parts) as usize, (budget / parts) as usize))
}

/// Assemble the `[sign+MSB][~magnitude~][LSB][suffix…]` spans used when a number is truncated.
fn assemble_truncated<'b>(
    sign_and_msb: String,
    magnitude: String,
    lsb: &str,
    suffix: Vec<Span<'b>>,
) -> Line<'b> {
    let mut spans = vec![
        Span::raw(sign_and_msb),
        Span::from(magnitude).yellow(),
        Span::raw(lsb.to_string()),
    ];
    spans.extend(suffix);
    Line::from(spans)
}

fn add_separators(repr: &str, group: usize) -> String {
    let (sign, rest) = if let Some(number) = repr.strip_prefix('-') {
        ("-", number)
    } else {
        ("", repr)
    };
    let (digits, rest) = if let Some(idx) = rest.find('.') {
        (&rest[..idx], &rest[idx..])
    } else {
        (rest, "")
    };
    let mut result = String::new();
    let len = digits.len();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i) % group == 0 {
            result.push(' ');
        }
        result.push(ch);
    }
    format!("{}{}{}", sign, result, rest)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn format_regular_number() {
        let n: BigDecimal = "12345".parse().unwrap();
        assert_eq!(format_number(&n, 10, false, 10).to_string(), "12345");
    }

    #[test]
    fn format_regular_number_with_separators() {
        let n: BigDecimal = "12345".parse().unwrap();
        assert_eq!(format_number(&n, 10, true, 10).to_string(), "12 345");
    }

    #[test]
    fn negative_number_with_separators() {
        let n: BigDecimal = "-12345".parse().unwrap();
        assert_eq!(format_number(&n, 10, true, 10).to_string(), "-12 345");
    }

    #[test]
    fn negative_number_with_separators_and_decimals() {
        let n: BigDecimal = "-12345.6789".parse().unwrap();
        assert_eq!(format_number(&n, 15, true, 10).to_string(), "-12 345.6789");
    }

    #[test]
    fn drop_separators_under_pressure() {
        let n: BigDecimal = "123456789".parse().unwrap();
        assert_eq!(format_number(&n, 10, true, 10).to_string(), "123456789");
    }

    #[test]
    fn format_long_number() {
        let n: BigDecimal = "123456789098".parse().unwrap();
        assert_eq!(format_number(&n, 10, false, 10).to_string(), "123~12~098");
        assert_eq!(format_number(&n, 11, false, 10).to_string(), "1234~12~098");
    }

    #[test]
    fn format_long_negative_number() {
        let n: BigDecimal = "-123456789098".parse().unwrap();
        assert_eq!(format_number(&n, 8, false, 10).to_string(), "-12~12~8");
        assert_eq!(format_number(&n, 7, false, 10).to_string(), "-1~12~8");
        // We need at least 7 characters for this...
        assert_eq!(format_number(&n, 6, false, 10).to_string(), "~");
    }

    #[test]
    fn format_long_decimal_number() {
        let n: BigDecimal = "12345678.34567".parse().unwrap();
        assert_eq!(format_number(&n, 7, false, 10).to_string(), "1~8~8.3");
    }

    #[test]
    fn format_dont_overflow_decimal() {
        let n: BigDecimal = "12345678909876543.21".parse().unwrap();
        assert_eq!(
            format_number(&n, 18, false, 10).to_string(),
            "12345~17~6543.21"
        );
    }

    #[test]
    fn format_long_negative_decimal_number() {
        let n: BigDecimal = "-12345678.34567".parse().unwrap();
        assert_eq!(format_number(&n, 8, false, 10).to_string(), "-1~8~8.3");
    }

    #[test]
    fn truncate_decimal_part() {
        let n: BigDecimal = "0.123456789".parse().unwrap();
        assert_eq!(format_number(&n, 4, false, 10).to_string(), "0.1~");
        let n: BigDecimal = "10.12345678".parse().unwrap();
        assert_eq!(format_number(&n, 4, false, 10).to_string(), "10.~");
    }

    #[test]
    fn handle_negative_scale() {
        let n: BigDecimal = "100000000000".parse().unwrap();
        let n = n.normalized();
        assert_eq!(format_number(&n, 10, false, 10).to_string(), "100~12~000");
    }

    #[test]
    fn trim_unneeded_zeros() {
        let n: BigDecimal = "0.000100000".parse().unwrap();
        assert_eq!(format_number(&n, 10, false, 10).to_string(), "0.0001");
        let n: BigDecimal = "1e100".parse().unwrap();
        assert_eq!(format_number(&n, 10, false, 10).to_string(), "100~101~00");
    }

    #[test]
    fn format_hex() {
        let n: BigDecimal = "255".parse().unwrap();
        assert_eq!(format_number(&n, 10, false, 16).to_string(), "ff");
    }

    #[test]
    fn format_binary() {
        let n: BigDecimal = "10".parse().unwrap();
        assert_eq!(format_number(&n, 10, false, 2).to_string(), "1010");
    }

    #[test]
    fn format_octal() {
        let n: BigDecimal = "8".parse().unwrap();
        assert_eq!(format_number(&n, 10, false, 8).to_string(), "10");
    }

    #[test]
    fn format_base_truncates_fraction() {
        let n: BigDecimal = "255.5".parse().unwrap();
        assert_eq!(format_number(&n, 10, false, 16).to_string(), "ff.~");
    }

    #[test]
    fn format_base_with_separators() {
        let n: BigDecimal = "65535".parse().unwrap();
        // ffff with group-of-4 separator -> "ff ff" but only 4 digits so no separator
        assert_eq!(format_number(&n, 10, true, 16).to_string(), "ffff");
        let n: BigDecimal = "16711935".parse().unwrap(); // 0xff_00ff
        assert_eq!(format_number(&n, 10, true, 16).to_string(), "ff 00ff");
    }

    #[test]
    fn format_binary_with_separators() {
        let n: BigDecimal = "255".parse().unwrap(); // 1111 1111
        assert_eq!(format_number(&n, 12, true, 2).to_string(), "1111 1111");
    }

    #[test]
    fn format_long_hex() {
        // 256^4 = 2^32 = 0x1_0000_0000 (9 hex digits)
        let n: BigDecimal = "4294967296".parse().unwrap();
        assert_eq!(format_number(&n, 8, false, 16).to_string(), "100~9~00");
    }

    #[test]
    fn format_long_hex_with_decimals() {
        // 256^4 = 2^32 = 0x1_0000_0000 (9 hex digits)
        let n: BigDecimal = "4294967296.333".parse().unwrap();
        assert_eq!(format_number(&n, 8, false, 16).to_string(), "10~9~0.~");
    }

    #[test]
    fn format_negative_hex() {
        let n: BigDecimal = "-255".parse().unwrap();
        assert_eq!(format_number(&n, 10, false, 16).to_string(), "-ff");
    }

    #[test]
    fn format_decimal_hex() {
        let n: BigDecimal = "255.333".parse().unwrap();
        assert_eq!(format_number(&n, 10, false, 16).to_string(), "ff.~");
    }
}

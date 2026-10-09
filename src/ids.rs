//! Checksums for Polish tax ids (NIP) and bank account numbers (NRB / IBAN), plus random test NIPs.

const NIP_WEIGHTS: [u32; 9] = [6, 5, 7, 2, 3, 4, 5, 6, 7];

/// Digits only: `526-587-76-35` and `PL5265877635` both become `5265877635`.
pub fn digits(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

pub fn nip_is_valid(nip: &str) -> bool {
    let d: Vec<u32> = nip.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() != 10 || nip.chars().count() != 10 || d[0] == 0 {
        return false;
    }
    nip_check_digit(&d[..9]) == Some(d[9])
}

fn nip_check_digit(first_nine: &[u32]) -> Option<u32> {
    let sum: u32 = first_nine.iter().zip(NIP_WEIGHTS).map(|(d, w)| d * w).sum();
    match sum % 11 {
        10 => None,
        check => Some(check),
    }
}

/// A random NIP that passes the checksum, for the shared KSeF test environment (never use a real one there).
pub fn random_nip(mut next: impl FnMut() -> u32) -> String {
    loop {
        let mut d: Vec<u32> = (0..9).map(|_| next() % 10).collect();
        if d[0] == 0 {
            d[0] = 1;
        }
        // KSeF also rejects NIPs whose 2nd and 3rd digits are both 0.
        if d[1] == 0 && d[2] == 0 {
            continue;
        }
        if let Some(check) = nip_check_digit(&d) {
            d.push(check);
            return d.iter().map(|d| char::from_digit(*d, 10).unwrap()).collect();
        }
    }
}

/// Polish account numbers: 26 digits, optionally written with spaces or a `PL` prefix; checked as IBAN.
pub fn account_is_valid(account: &str) -> bool {
    let compact: String = account.chars().filter(|c| !c.is_whitespace()).collect();
    let number = compact.strip_prefix("PL").unwrap_or(&compact);
    if number.len() != 26 || !number.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // IBAN mod 97: move "PL" + check digits to the end, letters as numbers (P=25, L=21).
    let rearranged = format!("{}2521{}", &number[2..], &number[..2]);
    rearranged
        .chars()
        .fold(0u32, |acc, c| (acc * 10 + c.to_digit(10).unwrap()) % 97)
        == 1
}

/// For FA(3) `NrRB`: digits only, or the IBAN without spaces when written with its country prefix
/// (`PL61 1090 ...` -> `PL61109010140000071219812874`).
pub fn compact_account(account: &str) -> String {
    let compact: String = account
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    if compact.chars().take(2).all(|c| c.is_ascii_alphabetic()) && compact.len() > 2 {
        compact.to_uppercase()
    } else {
        digits(account)
    }
}

/// `61109010140000071219812874` -> `61 1090 1014 0000 0712 1981 2874`; with a prefix, IBAN style
/// `PL61 1090 1014 0000 0712 1981 2874`.
pub fn format_account(account: &str) -> String {
    let compact = compact_account(account);
    if let Some(prefix) = compact
        .get(..2)
        .filter(|p| p.chars().all(|c| c.is_ascii_alphabetic()))
    {
        return format!("{prefix}{}", format_account(&compact[2..]));
    }
    let d = digits(account);
    if d.len() != 26 {
        return account.trim().to_string();
    }
    let mut out = d[..2].to_string();
    for chunk in d.as_bytes()[2..].chunks(4) {
        out.push(' ');
        out.push_str(std::str::from_utf8(chunk).unwrap());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nip_checksum() {
        assert!(nip_is_valid("5265877635"));
        assert!(nip_is_valid("1111111111"));
        assert!(!nip_is_valid("5265877636"));
        assert!(!nip_is_valid("526587763"));
        assert!(!nip_is_valid("526-587-76-35"), "only bare digits go into the XML");
    }

    #[test]
    fn random_nips_are_valid() {
        let mut seed = 7u32;
        for _ in 0..50 {
            let nip = random_nip(|| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                seed >> 16
            });
            assert!(nip_is_valid(&nip), "{nip}");
        }
    }

    #[test]
    fn account_checksum_and_format() {
        let account = "61 1090 1014 0000 0712 1981 2874";
        assert!(account_is_valid(account));
        assert!(account_is_valid("PL61109010140000071219812874"));
        assert!(!account_is_valid("61 1090 1014 0000 0712 1981 2875"));
        assert_eq!(format_account("61109010140000071219812874"), account);
    }
}

use regex::Regex;
pub fn normalize_bin(raw: &str) -> Result<String, String> {
    let filtered: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'x' || *c == 'X')
        .map(|c| if c == 'X' { 'x' } else { c })
        .collect();
    if filtered.is_empty() {
        return Err("bin is empty after filtering; expected digits and/or 'x' placeholders".into());
    }
    let re = Regex::new(r"^[0-9x]+$").unwrap();
    if !re.is_match(&filtered) {
        return Err("bin contains invalid characters (allowed: digits and 'x')".into());
    }
    let mut template = filtered;
    if template.len() > 16 {
        template.truncate(16);
    } else if template.len() < 16 {
        template.push_str(&"x".repeat(16 - template.len()));
    }
    Ok(template)
}
pub fn normalize_month(raw: &str) -> Result<String, String> {
    if raw.trim().is_empty() || raw.trim() == "," {
        return Ok("xx".to_owned());
    }
    let filtered: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'x' || *c == 'X')
        .map(|c| if c == 'X' { 'x' } else { c })
        .collect();
    if filtered.is_empty() {
        return Err("expiry_month is empty/invalid; expected digits and/or 'x'".into());
    }
    let re = Regex::new(r"^[0-9x]+$").unwrap();
    if !re.is_match(&filtered) {
        return Err("expiry_month contains invalid characters (allowed: digits and 'x')".into());
    }
    let mut template = filtered;
    if template.len() > 2 {
        template.truncate(2);
    } else if template.len() < 2 {
        template.push_str(&"x".repeat(2 - template.len()));
    }
    Ok(template)
}
pub fn normalize_year(raw: &str) -> Result<String, String> {
    if raw.trim().is_empty() || raw.trim() == "," {
        return Ok("xxxx".to_owned());
    }
    let filtered: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'x' || *c == 'X')
        .map(|c| if c == 'X' { 'x' } else { c })
        .collect();
    if filtered.is_empty() {
        return Err("expiry_year is empty/invalid; expected digits and/or 'x'".into());
    }
    let re = Regex::new(r"^[0-9x]+$").unwrap();
    if !re.is_match(&filtered) {
        return Err("expiry_year contains invalid characters (allowed: digits and 'x')".into());
    }
    let mut template = filtered;
    if template.len() > 4 {
        template.truncate(4);
    } else if template.len() < 4 {
        template.push_str(&"x".repeat(4 - template.len()));
    }
    Ok(template)
}
pub fn normalize_cvv(raw: &str) -> Result<String, String> {
    if raw.trim().is_empty() || raw.trim() == "," {
        return Ok("xxx".to_owned());
    }
    let filtered: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'x' || *c == 'X')
        .map(|c| if c == 'X' { 'x' } else { c })
        .collect();
    if filtered.is_empty() {
        return Err("cvv is empty/invalid; expected digits and/or 'x'".into());
    }
    let re = Regex::new(r"^[0-9x]+$").unwrap();
    if !re.is_match(&filtered) {
        return Err("cvv contains invalid characters (allowed: digits and 'x')".into());
    }
    let mut template = filtered;
    if template.len() > 4 {
        template.truncate(4);
    } else if template.len() < 3 {
        template.push_str(&"x".repeat(3 - template.len()));
    }
    Ok(template)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_normalize_examples() {
        assert_eq!(
            normalize_bin("440393").expect("Operation failed"),
            "440393xxxxxxxxxx".to_string()
        );
        assert_eq!(
            normalize_bin("44393xx45").expect("Operation failed"),
            "44393xx45xxxxxxx".to_string()
        );
        assert_eq!(
            normalize_month("").expect("Operation failed"),
            "xx".to_string()
        );
        assert_eq!(
            normalize_month("1x").expect("Operation failed"),
            "1x".to_string()
        );
        assert_eq!(
            normalize_month("1").expect("Operation failed"),
            "1x".to_string()
        );
        assert_eq!(
            normalize_year("").expect("Operation failed"),
            "xxxx".to_string()
        );
        assert_eq!(
            normalize_year("20xx").expect("Operation failed"),
            "20xx".to_string()
        );
        assert_eq!(
            normalize_cvv("").expect("Operation failed"),
            "xxx".to_string()
        );
        assert_eq!(
            normalize_cvv("14x").expect("Operation failed"),
            "14x".to_string()
        );
    }
}

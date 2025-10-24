pub fn luhn_check(card_number: &str) -> bool {
    let digits: Vec<u32> = card_number.chars().filter_map(|c| c.to_digit(10)).collect();

    if digits.len() < 2 {
        return false;
    }

    let mut sum = 0;
    let mut is_second = false;

    for &digit in digits.iter().rev() {
        let mut value = digit;

        if is_second {
            value *= 2;
            if value > 9 {
                value = value / 10 + value % 10;
            }
        }

        sum += value;
        is_second = !is_second;
    }

    sum % 10 == 0
}

pub fn luhn_generate_check_digit(partial_number: &str) -> Option<u32> {
    let digits: Vec<u32> = partial_number
        .chars()
        .filter_map(|c| c.to_digit(10))
        .collect();

    if digits.is_empty() {
        return None;
    }

    let mut sum = 0;
    let mut is_second = true;

    for &digit in digits.iter().rev() {
        let mut value = digit;

        if is_second {
            value *= 2;
            if value > 9 {
                value = value / 10 + value % 10;
            }
        }

        sum += value;
        is_second = !is_second;
    }

    let check_digit = (10 - (sum % 10)) % 10;
    Some(check_digit)
}

pub async fn luhn_checksum(cc: &str) -> Result<bool, String> {
    Ok(luhn_check(cc))
}

use chrono::Utc;

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());

    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                let high = char::from_digit((byte >> 4) as u32, 16)
                    .expect("nibble 0-15 is always a valid hex digit")
                    .to_ascii_uppercase();
                let low = char::from_digit((byte & 0x0f) as u32, 16)
                    .expect("nibble 0-15 is always a valid hex digit")
                    .to_ascii_uppercase();
                encoded.push('%');
                encoded.push(high);
                encoded.push(low);
            }
        }
    }

    encoded
}

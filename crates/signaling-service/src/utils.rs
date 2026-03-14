use rand::Rng;

/// Generate an 8-character alphanumeric invite code.
pub fn generate_invite_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Generate a URL-friendly slug from a room name.
///
/// Lowercases, replaces non-alphanumeric characters with hyphens, trims
/// leading/trailing hyphens, and appends 4 random alphanumeric characters.
pub fn generate_slug(name: &str) -> String {
    let base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse consecutive hyphens and trim edges
    let mut slug = String::with_capacity(base.len() + 5);
    let mut prev_hyphen = true; // start true to skip leading hyphens
    for c in base.chars() {
        if c == '-' {
            if !prev_hyphen {
                slug.push('-');
            }
            prev_hyphen = true;
        } else {
            slug.push(c);
            prev_hyphen = false;
        }
    }
    // Trim trailing hyphen
    if slug.ends_with('-') {
        slug.pop();
    }

    // Append random suffix
    let suffix: String = {
        let mut rng = rand::thread_rng();
        (0..4)
            .map(|_| {
                let idx = rng.gen_range(0..36);
                if idx < 10 {
                    (b'0' + idx) as char
                } else {
                    (b'a' + idx - 10) as char
                }
            })
            .collect()
    };

    format!("{slug}-{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_code_length_and_charset() {
        let code = generate_invite_code();
        assert_eq!(code.len(), 8);
        assert!(code.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn slug_from_name() {
        let slug = generate_slug("My Cool Room!");
        // Should start with "my-cool-room-" and end with 4 alphanumeric chars
        assert!(slug.starts_with("my-cool-room-"));
        assert_eq!(slug.len(), "my-cool-room-".len() + 4);
    }
}

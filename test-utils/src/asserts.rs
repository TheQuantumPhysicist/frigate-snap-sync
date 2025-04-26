pub fn assert_str_ends_with(s1: &str, to_end_with: &str) {
    assert!(
        s1.ends_with(to_end_with),
        "String does not end with expected value. \nString: `{s1}`\nDoes not end with: `{to_end_with}`"
    );
}

pub fn assert_str_contains(s1: &str, to_contain: &str) {
    assert!(
        s1.contains(to_contain),
        "String does not contain expected value. \nString: `{s1}`\nDoes not contain: `{to_contain}`"
    );
}

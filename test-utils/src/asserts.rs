pub fn assert_str_starts_with(s: &str, to_start_with: &str) {
    assert!(
        s.starts_with(to_start_with),
        "String does not start with expected value. \nString: `{s}`\nDoes not end with: `{to_start_with}`"
    );
}

pub fn assert_str_ends_with(s: &str, to_end_with: &str) {
    assert!(
        s.ends_with(to_end_with),
        "String does not end with expected value. \nString: `{s}`\nDoes not end with: `{to_end_with}`"
    );
}

pub fn assert_str_contains(s: &str, to_contain: &str) {
    assert!(
        s.contains(to_contain),
        "String does not contain expected value. \nString: `{s}`\nDoes not contain: `{to_contain}`"
    );
}

pub fn assert_slice_contains<T: Eq + std::fmt::Debug>(v: &[T], to_contain: &T) {
    assert!(
        v.contains(to_contain),
        "String does not contain expected value. \nString: `{v:?}`\nDoes not contain: `{to_contain:?}`"
    );
}

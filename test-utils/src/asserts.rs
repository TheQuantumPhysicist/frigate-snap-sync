#[macro_export]
macro_rules! assert_str_starts_with {
    ($s:expr, $prefix:expr $(,)?) => {
        assert!(
            $s.starts_with($prefix),
            "assert_str_starts_with failed at {}:{}\nString: `{}`\nExpected prefix: `{}`",
            file!(),
            line!(),
            $s,
            $prefix
        );
    };
}

#[macro_export]
macro_rules! assert_str_ends_with {
    ($s:expr, $suffix:expr $(,)?) => {
        assert!(
            $s.ends_with($suffix),
            "assert_str_ends_with failed at {}:{}\nString: `{}`\nExpected suffix: `{}`",
            file!(),
            line!(),
            $s,
            $suffix
        );
    };
}

#[macro_export]
macro_rules! assert_str_contains {
    ($s:expr, $needle:expr $(,)?) => {
        assert!(
            $s.contains($needle),
            "assert_str_contains failed at {}:{}\nString: `{}`\nExpected to contain: `{}`",
            file!(),
            line!(),
            $s,
            $needle
        );
    };
}

#[macro_export]
macro_rules! assert_slice_contains {
    ($slice:expr, $item:expr $(,)?) => {
        assert!(
            $slice.contains($item),
            "assert_slice_contains failed at {}:{}\nSlice: `{:?}`\nExpected to contain: `{:?}`",
            file!(),
            line!(),
            $slice,
            $item
        );
    };
}

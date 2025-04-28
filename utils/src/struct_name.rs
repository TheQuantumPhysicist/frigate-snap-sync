#[macro_export]
macro_rules! struct_name {
    ($t:ty) => {
        stringify!($t)
    };
}

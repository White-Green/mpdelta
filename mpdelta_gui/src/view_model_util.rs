macro_rules! use_arc {
    () => {};
    ($a:ident$(,)?) => {
        let $a = ::std::sync::Arc::clone(&$a);
    };
    ($a:ident = $b:expr$(,)?) => {
        let $a = ::std::sync::Arc::clone(&$b);
    };
    ($a:ident, $($t:tt)*) => {
        let $a = ::std::sync::Arc::clone(&$a);
        use_arc!($($t)*);
    };
    ($a:ident = $b:expr, $($t:tt)*) => {
        let $a = ::std::sync::Arc::clone(&$b);
        use_arc!($($t)*);
    };
}

pub(crate) use use_arc;

pub type SsoString = smartstring::SmartString<smartstring::LazyCompact>;
pub type FastHashMap<K, V> = fnv::FnvHashMap<K, V>;
pub type FastHashSet<K> = fnv::FnvHashSet<K>;

#[macro_export]
macro_rules! format_sso {
    ($($arg:tt)*) => {{
        use std::fmt::Write as _;
        let mut buffer = $crate::util::typedefs::SsoString::new();   
        write!(buffer, $($arg)*).expect("unexpected formatting error");
        buffer
    }};
}

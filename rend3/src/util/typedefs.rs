/// A string which uses SmallString optimization for strings shorter than 23
/// characters.
pub type SsoString = smartstring::SmartString<smartstring::LazyCompact>;
/// Hash map designed for small keys
pub type FastHashMap<K, V> = rustc_hash::FxHashMap<K, V>;
pub type FastHashSet<K> = rustc_hash::FxHashSet<K>;
pub type FastHasher = rustc_hash::FxHasher;
pub type FastBuildHasher = std::hash::BuildHasherDefault<FastHasher>;

pub type RendererStatistics = Vec<wgpu_profiler::GpuTimerScopeResult>;

#[macro_export]
/// Similar to the [`format`] macro, but creates a [`SsoString`].
macro_rules! format_sso {
    ($($arg:tt)*) => {{
        use std::fmt::Write as _;
        let mut buffer = $crate::util::typedefs::SsoString::new();
        write!(buffer, $($arg)*).expect("unexpected formatting error");
        buffer
    }};
}

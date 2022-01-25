//! Typedefs for commonly used structures from other crates.

/// A string which uses SmallString optimization for strings shorter than 23
/// characters.
pub type SsoString = smartstring::SmartString<smartstring::LazyCompact>;
/// Hash map designed for small keys.
pub type FastHashMap<K, V> = rustc_hash::FxHashMap<K, V>;
/// Hash set designed for small keys.
pub type FastHashSet<K> = rustc_hash::FxHashSet<K>;
/// Hasher designed for small keys.
pub type FastHasher = rustc_hash::FxHasher;
/// Build hasher designed for small keys.
pub type FastBuildHasher = std::hash::BuildHasherDefault<FastHasher>;
/// Output of wgpu_profiler's code.
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

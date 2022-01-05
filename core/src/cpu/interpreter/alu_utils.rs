mod shifts_common;
use cfg_if::cfg_if;

cfg_if! {
    // TODO: The specialized x86 handlers have very questionable usefulness, maybe they should be
    // scrapped.
    // Obscure bugs this has caused so far: 1
    if #[cfg(any(target_arch = "x86_64", target_arch = "x86"))] {
        mod x86;
        pub use x86::*;
    } else {
        mod all;
        pub use all::*;
    }
}

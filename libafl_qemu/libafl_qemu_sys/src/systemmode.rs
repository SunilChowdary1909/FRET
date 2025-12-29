use paste::paste;
use crate::extern_c_checked;

extern_c_checked!(
    pub fn icount_get_raw() -> u64;
);
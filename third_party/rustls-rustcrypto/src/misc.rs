#[macro_export]
macro_rules! const_concat_slices {
    ($ty:ty, $a:expr, $b:expr $(,)*) => {{
        const A: &[$ty] = $a;
        const B: &[$ty] = $b;
        const __LEN: usize = A.len() + B.len();
        const __CONCATENATED: &[$ty; __LEN] = &{
            let mut out: [$ty; __LEN] = if __LEN == 0 {
                unsafe { core::mem::transmute([0u8; core::mem::size_of::<$ty>() * __LEN]) }
            } else if A.len() == 0 {
                [B[0]; __LEN]
            } else {
                [A[0]; __LEN]
            };
            let mut i = 0;
            while i < A.len() {
                out[i] = A[i];
                i += 1;
            }
            i = 0;
            while i < B.len() {
                out[i + A.len()] = B[i];
                i += 1;
            }
            out
        };

        __CONCATENATED
    }};
    ($ty:ty, $a:expr, $b:expr, $($c:expr), + $(,)* ) => {{
        const CON: &[$ty] = const_concat_slices!($ty, $a, $b);
        const_concat_slices!($ty, CON, $($c), +)
    }}
}

pub(crate) use const_concat_slices;

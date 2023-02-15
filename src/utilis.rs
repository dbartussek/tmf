use crate::{FloatType, IndexType, Vector3};
/// Fast method for pruning unused vertices. Finds the lowest used vertex and removes it. O(n), does **not** remove all unused vertices. Used mostly
/// in .obj loading. Fast because if not needed exits quickly.
pub(crate) fn fast_prune<T: Sized>(data: &mut Vec<T>, indices: &mut [IndexType]) {
    // Smallest index
    let mut min_index = IndexType::MAX;
    for index in indices.iter() {
        if *index == 0 {
            return;
        };
        min_index = min_index.min(*index);
    }
    data.drain(0..(min_index as usize));
    // Begin index rebasing
    for index in &mut *indices {
        *index -= min_index;
    }
}
#[allow(non_camel_case_types)]
#[cfg(feature = "fast_trig")]
type fprec = f64;
#[cfg(feature = "fast_trig")]
#[cfg(feature = "fast_trig")]
const F_PI: fprec = std::f64::consts::PI;
// https://www.gamedev.net/forums/topic/621589-extremely-fast-sin-approximation/
#[cfg(feature = "fast_trig")]
#[inline(always)]
pub(crate) fn fsin(mut x: fprec) -> fprec {
    let mut z = (x * 0.3183098861837907) + 6755399441055744.0;
    let k: i32 = unsafe { *(&z as *const _ as *const _) };
    z = (k as fprec) * F_PI;
    x -= z;
    let y = x * x;
    let z = (0.0073524681968701 * y - 0.1652891139701474) * y + 0.9996919862959676;
    x *= z;
    let mut k = k & 1;
    k += k;
    let z = (k as fprec) * x;
    return x - z;
}
pub(crate) fn magnitude(i: Vector3) -> FloatType {
    let xx = i.0 * i.0;
    let yy = i.1 * i.1;
    let zz = i.2 * i.2;
    (xx + yy + zz).sqrt()
}
pub(crate) fn normalize(i: Vector3) -> Vector3 {
    let xx = i.0 * i.0;
    let yy = i.1 * i.1;
    let zz = i.2 * i.2;
    let mag = (xx + yy + zz).sqrt();

    (i.0 / mag, i.1 / mag, i.2 / mag)
}
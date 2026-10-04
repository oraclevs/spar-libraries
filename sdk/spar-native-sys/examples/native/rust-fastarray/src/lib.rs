//! Example Spar native module in Rust. Build: `cargo build --release`, load the resulting
//! `target/release/librust_fastarray.so` with `SPAR_NATIVE_MODULES` or `Engine`.

#[spar_native::module(name = "fastArray", version = "0.1.0")]
mod fast_array {
    use spar_native::{Buf, Error, Owned, Resource};
    use std::sync::atomic::{AtomicI64, Ordering};

    /// A native counter exposed to Spar as the opaque `Tally` type.
    pub struct Tally(AtomicI64);
    impl Resource for Tally {
        const SPAR_TYPE: &'static str = "Tally";
    }

    #[spar_native::function]
    fn tally_new(start: i64) -> Owned<Tally> {
        Owned(Tally(AtomicI64::new(start)))
    }

    #[spar_native::function]
    fn tally_add(tally: &Tally, n: i64) -> i64 {
        tally.0.fetch_add(n, Ordering::SeqCst) + n
    }


    /// Sum of a float list or Buffer. Zero-copy for a Buffer, one copy for a plain list.
    #[spar_native::function]
    fn sum(values: &[f64]) -> f64 {
        values.iter().sum()
    }

    #[spar_native::function]
    fn dot(a: &[f64], b: &[f64]) -> Result<f64, Error> {
        if a.len() != b.len() {
            return Err(Error::range(format!("dot: length mismatch ({} vs {})", a.len(), b.len())));
        }
        Ok(a.iter().zip(b).map(|(x, y)| x * y).sum())
    }

    /// `n` evenly spaced values in [0, 1]. The Vec moves into the runtime without copying.
    #[spar_native::function]
    fn linspace(n: i64) -> Result<Buf<f64>, Error> {
        if n < 0 {
            return Err(Error::range("linspace: n must be non-negative"));
        }
        let n = n as usize;
        let step = if n > 1 { 1.0 / (n - 1) as f64 } else { 0.0 };
        Ok(Buf((0..n).map(|i| i as f64 * step).collect()))
    }

    /// In-place scale through a writable borrow.
    #[spar_native::function]
    fn scale(buf: &mut [f64], k: f64) {
        for x in buf {
            *x *= k;
        }
    }

    /// Parallel sum with std threads: the borrow outlives the scope, the runtime holds no lock.
    #[spar_native::function]
    fn par_sum(values: &[f64], threads: i64) -> f64 {
        let t = threads.clamp(1, 64) as usize;
        let chunk = values.len().div_ceil(t).max(1);
        std::thread::scope(|s| {
            let handles: Vec<_> = values.chunks(chunk).map(|c| s.spawn(move || c.iter().sum::<f64>())).collect();
            handles.into_iter().map(|h| h.join().unwrap()).sum()
        })
    }

    #[spar_native::function]
    fn word_count(text: &str) -> i64 {
        text.split_whitespace().count() as i64
    }

    #[spar_native::function]
    fn reverse(text: &str) -> String {
        text.chars().rev().collect()
    }

    #[spar_native::function]
    fn bytes_sum(data: &[u8]) -> i64 {
        data.iter().map(|b| *b as i64).sum()
    }

    #[spar_native::function]
    fn explode() -> i64 {
        panic!("boom from rust");
    }
}

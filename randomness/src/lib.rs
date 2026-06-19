pub use rand::prelude::SliceRandom;
pub use rand::rand_core::Infallible;
pub use rand::{CryptoRng, Rng, RngExt, SeedableRng, TryCryptoRng, TryRng, rand_core, seq};

pub mod distributions {
    pub use rand::distr::{Alphanumeric, Distribution, StandardUniform, weighted::WeightedIndex};
    pub mod uniform {
        pub use rand::distr::uniform::SampleRange;
    }
}

pub mod rngs {
    pub use rand::rngs::SysRng;
}

#[must_use]
pub fn make_os_rng() -> impl CryptoRng {
    // - SysRng reads OS entropy and is a fallible CryptoRng (its error is an OS read failure).
    // - UnwrapErr turns that fallible source into an infallible CryptoRng.
    // - An OS entropy read failure is an unrecoverable invariant, so panicking is correct.
    // - Use for consumers that demand a concrete infallible CryptoRng, like ssh key generation.
    rand_core::UnwrapErr(rand::rngs::SysRng)
}

#[must_use]
pub fn make_true_rng() -> impl CryptoRng {
    // - rand 0.10 builds StdRng by seeding it from another RNG.
    // - SysRng is the OS entropy source feeding the seed.
    // - try_from_rng only fails if the OS read fails, which is an unrecoverable invariant.
    rand::rngs::StdRng::try_from_rng(&mut rand::rngs::SysRng)
        .expect("OS entropy source must be available for the true RNG")
}

#[must_use]
pub fn make_pseudo_rng() -> impl Rng {
    rand::rngs::ThreadRng::default()
}

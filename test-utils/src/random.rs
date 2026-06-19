use rand_chacha::ChaChaRng;
pub use randomness::{
    self, CryptoRng, Infallible, Rng, RngExt, SeedableRng, TryCryptoRng, TryRng,
    seq::IteratorRandom,
};
use rstest::fixture;
use std::{num::ParseIntError, ops::RangeBounds, str::FromStr};

#[derive(Debug, Copy, Clone)]
pub struct Seed(pub u64);

impl Seed {
    #[must_use]
    pub fn from_entropy() -> Self {
        Seed(randomness::make_true_rng().next_u64())
    }

    #[must_use]
    pub fn from_entropy_and_print(test_name: &str) -> Self {
        let result = Seed(randomness::make_true_rng().next_u64());
        result.print_with_decoration(test_name);
        result
    }

    #[must_use]
    pub fn from_u64(v: u64) -> Self {
        Seed(v)
    }

    #[must_use]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn print_with_decoration(&self, test_name: &str) {
        println!("{test_name} seed: {}", self.0);
    }

    #[must_use]
    pub fn derive_seed(&self) -> Seed {
        let mut rng = make_seedable_rng(*self);
        rng.random()
    }
}

impl FromStr for Seed {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.parse::<u64>()?;
        Ok(Seed::from_u64(v))
    }
}

impl From<u64> for Seed {
    fn from(v: u64) -> Self {
        Seed::from_u64(v)
    }
}

impl randomness::distributions::Distribution<Seed> for randomness::distributions::StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Seed {
        let new_seed = rng.next_u64();
        Seed::from_u64(new_seed)
    }
}

pub fn make_random_alphanumeric_string(rng: &mut impl Rng, size: usize) -> String {
    rng.sample_iter(&randomness::distributions::Alphanumeric)
        .take(size)
        .map(char::from)
        .collect()
}

#[derive(Debug, Clone)]
pub struct TestRng(rand_chacha::ChaChaRng);

impl TestRng {
    #[must_use]
    pub fn new(seed: Seed) -> Self {
        Self(ChaChaRng::seed_from_u64(seed.as_u64()))
    }

    #[must_use]
    pub fn random(rng: &mut impl CryptoRng) -> Self {
        Self::new(Seed(rng.next_u64()))
    }
    #[must_use]
    pub fn from_entropy() -> Self {
        Self::new(Seed::from_entropy())
    }
}

// - rand 0.10 builds Rng/CryptoRng from TryRng/TryCryptoRng via blanket impls.
// - The inner ChaChaRng is infallible, so the deterministic TestRng is infallible too.
// - Implementing the fallible base traits gives TestRng the full Rng + CryptoRng surface.
impl TryRng for TestRng {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        self.0.try_next_u32()
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        self.0.try_next_u64()
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
        self.0.try_fill_bytes(dest)
    }
}

impl TryCryptoRng for TestRng {}

#[must_use]
pub fn make_seedable_rng(seed: Seed) -> impl CryptoRng {
    TestRng::new(seed)
}

fn range_to_random_size(rng: &mut impl Rng, size: impl RangeBounds<usize>) -> usize {
    let start = match size.start_bound() {
        std::ops::Bound::Included(&n) => n,
        std::ops::Bound::Excluded(&n) => n + 1,
        std::ops::Bound::Unbounded => 0,
    };
    let end = match size.end_bound() {
        std::ops::Bound::Included(&n) => n + 1,
        std::ops::Bound::Excluded(&n) => n,
        std::ops::Bound::Unbounded => usize::MAX,
    };
    rng.random_range(start..end)
}

pub fn gen_random_bytes(rng: &mut impl Rng, size: impl RangeBounds<usize>) -> Vec<u8> {
    let data_length = range_to_random_size(rng, size);
    let mut bytes = vec![0; data_length];
    rng.fill_bytes(&mut bytes);
    bytes
}

pub fn gen_random_string<R: Rng>(rng: &mut R, size: impl RangeBounds<usize>) -> String {
    let size = range_to_random_size(rng, size);
    rng.sample_iter(&randomness::distributions::Alphanumeric)
        .take(size)
        .map(char::from)
        .collect()
}

#[fixture]
pub fn random_seed() -> Seed {
    Seed::from_entropy()
}

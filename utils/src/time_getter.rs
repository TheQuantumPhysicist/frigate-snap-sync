use std::sync::Arc;

use crate::time::{self, Time};

pub trait TimeGetterFn: Send + Sync {
    fn get_time(&self) -> Time;
}

/// A function wrapper that contains the function that will be used to get the current time in chainstate
#[derive(Clone)]
pub struct TimeGetter {
    f: Arc<dyn TimeGetterFn>,
}

impl TimeGetter {
    #[must_use]
    pub fn new(f: Arc<dyn TimeGetterFn>) -> Self {
        Self { f }
    }

    #[must_use]
    pub fn get_time(&self) -> Time {
        self.f.get_time()
    }

    #[must_use]
    pub fn getter(&self) -> &dyn TimeGetterFn {
        &*self.f
    }
}

impl Default for TimeGetter {
    fn default() -> Self {
        Self::new(Arc::new(DefaultTimeGetterFn::new()))
    }
}

struct DefaultTimeGetterFn;

impl DefaultTimeGetterFn {
    fn new() -> Self {
        Self
    }
}

impl TimeGetterFn for DefaultTimeGetterFn {
    fn get_time(&self) -> Time {
        time::get_time()
    }
}

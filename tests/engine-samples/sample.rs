//! Engine sample for Rust: a header comment, then declarations of most kinds.

use std::fmt;

/// Documented.
#[derive(Debug)]
pub struct Point<T> {
    x: T,
}

pub struct Pair<T>(T, T)
where
    T: Copy;

pub enum Role {
    Admin,
}

pub trait Engine: Sync {
    fn run(&self);
}

impl<T: Clone> Point<T>
where
    T: Copy,
{
    fn get(&self) -> T {
        self.x
    }
}

// A plain comment.
pub fn generic<T>(value: T) -> T
where
    T: Clone,
{
    value
}

pub const MAX: usize = 3;
static NAME: &str = "]";
pub type Id = u64;
macro_rules! make_id {
    ($x:expr) => {
        $x
    };
}
pub use fmt::Write;
thread_local! {
    static COUNT: u8 = 0;
}
#[cfg(test)]
mod tests {}
unsafe extern "C" {
    fn c_log(code: i32);
}

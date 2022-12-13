use std::{
  cell::UnsafeCell,
  mem,
  ops,
  sync::atomic::{AtomicUsize, Ordering},
};

const WRITE_BIT: usize = 1 << (mem::size_of::<usize>() * 8 - 1);

/// A shared reference to `SynCell` data.
pub struct SynRef<'a, T> {
  state: &'a AtomicUsize,
  value: &'a T,
}

impl<T> Drop for SynRef<'_, T> {
  fn drop(&mut self) {
    self.state.fetch_sub(1, Ordering::Release);
  }
}

impl<T> ops::Deref for SynRef<'_, T> {
  type Target = T;

  fn deref(&self) -> &T {
    self.value
  }
}

/// A mutable reference to `SynCell` data.
pub struct SynRefMut<'a, T> {
  state: &'a AtomicUsize,
  value: &'a mut T,
}

impl<T> Drop for SynRefMut<'_, T> {
  fn drop(&mut self) {
    self.state.fetch_and(!WRITE_BIT, Ordering::Release);
  }
}

impl<T> ops::Deref for SynRefMut<'_, T> {
  type Target = T;

  fn deref(&self) -> &T {
    self.value
  }
}

impl<T> ops::DerefMut for SynRefMut<'_, T> {
  fn deref_mut(&mut self) -> &mut T {
    self.value
  }
}

/// A Sync cell. Stores a value of type `T` and allows
/// to access it behind a reference. `SynCell` follows Rust borrowing
/// rules but checks them at run time as opposed to compile time.
pub struct SynCell<T> {
  state: AtomicUsize,
  value: UnsafeCell<T>,
}

unsafe impl<T> Sync for SynCell<T> {}

impl<T> SynCell<T> {
  /// Create a new cell.
  pub fn new(value: T) -> Self {
    Self {
      state: AtomicUsize::new(0),
      value: UnsafeCell::new(value),
    }
  }

  /// Borrow mutably (exclusive).
  ///
  /// Panics if the value is already borrowed in any way.
  pub fn borrow_mut(&self) -> SynRefMut<T> {
    let old = self.state.fetch_or(WRITE_BIT, Ordering::AcqRel);
    if old & WRITE_BIT != 0 {
      panic!("SynCell is mutably borrowed elsewhere!");
    } else if old != 0 {
      self.state.fetch_and(!WRITE_BIT, Ordering::Release);
      panic!("SynCell is immutably borrowed elsewhere!");
    }
    SynRefMut {
      state: &self.state,
      value: unsafe { &mut *self.value.get() },
    }
  }
}

impl<T> std::fmt::Debug for SynCell<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("SynCell")
      .field("value", &self.value)
      .finish()
  }
}

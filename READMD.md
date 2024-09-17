# GMTX

GMTX is a Rust crate that provides `Gutex` type for protecting shared data similar to
`std::sync::Mutex`.

The `std::sync::Mutex` and the related types are prone to deadlock when using on a multiple struct
fields like this:

```rust
use std::sync::Mutex;

pub struct Foo {
    field1: Mutex<()>,
    field2: Mutex<()>,
}
```

The order to acquire the lock must be the same everywhere otherwise the deadlock is possible.
Maintaining the lock order manually are cumbersome task so we invent this crate to handle this
instead.

How this crate are working is simple. Any locks on any `Gutex` will lock the same mutex in the
group, which mean there are only one mutex in the group. It have the same effect as the following
code:

```rust
use std::sync::Mutex;

pub struct Foo {
    data: Mutex<Data>,
}

struct Data {
    field1: (),
    field2: (),
}
```

The bonus point of `Gutex` is it will allow recursive lock for read-only access so you will never
end up deadlock yourself. This read-only access is per `Gutex`. It will panic if you try to acquire
write access while the readers are still active the same as `std::cell::RefCell`.

## Example

```rust
use gmtx::{GutexGroup, Gutex};

pub struct MyType {
    field1: Gutex<String>,
    field2: Gutex<usize>,
}

impl MyType {
    pub fn new() -> Self {
        // Create a single group for both field1 and field2. Any lock on the same group will lock
        // the same mutex.
        let gg = GutexGroup::new();

        Self {
            field1: gg.spawn("value1"),
            field2: gg.spawn(0),
        }
    }

    pub fn method1(&self) {
        // This will acquire a group lock for both field1 and field2. This will block if the other
        // thread being hold a lock on this group.
        let v1 = self.field1.read();

        // Group lock already acquired by field1 lock so this will return immediately. The lock
        // order on the same group does not matter so you can swap this line with the above line.
        let mut v2 = self.field2.write();

        if v1 == "value1" {
            *v2 = 0;
        } else {
            *v2 = 1;
        }

        // You can have multiple read locks on the same Gutex.
        self.method2();
    }

    fn method2(&self) {
        println!("{}", self.field1.read());
    }
}
```

## License

This project is licensed under either of

- Apache License, Version 2.0
- MIT License

at your option.

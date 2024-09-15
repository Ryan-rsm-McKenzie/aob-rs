# Overview

`aob` (array of bytes) is a library for string searching with wildcards. It supports IDA-style patterns, `const` compiling patterns, and accelerating searches using simd whenever possible/practical.

The latest development docs are available at: https://ryan-rsm-mckenzie.github.io/aob-rs/aob/index.html

The stable release docs are available at: https://docs.rs/aob/latest/aob/

Changelogs are available at: https://github.com/Ryan-rsm-McKenzie/aob-rs/releases

# Example

```rust
use aob::Needle as _;
aob::aob! { const NEEDLE = ida("67 ? AB"); }
let haystack = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];
let found = NEEDLE.find(&haystack).unwrap();
assert_eq!(found.range(), 3..6);
```

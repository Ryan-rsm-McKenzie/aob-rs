use aob_common::{
    DynamicNeedle,
    Needle as _,
};
use criterion::{
    criterion_group,
    criterion_main,
    BenchmarkId,
    Criterion,
    Throughput,
};
use std::hint;

mod lightning {
    use lightningscanner::Scanner;
    use std::{
        marker::PhantomData,
        ops::Range,
    };

    pub(crate) struct Iter<'haystack, 'scanner> {
        scanner: &'scanner Scanner,
        haystack: Range<*const u8>,
        cursor: *const u8,
        _phantom: PhantomData<&'haystack u8>,
    }

    impl<'haystack, 'scanner> Iter<'haystack, 'scanner> {
        pub(crate) fn new(scanner: &'scanner Scanner, haystack: &'haystack [u8]) -> Self {
            Self {
                scanner,
                haystack: haystack.as_ptr_range(),
                cursor: haystack.as_ptr(),
                _phantom: PhantomData,
            }
        }
    }

    impl Iterator for Iter<'_, '_> {
        type Item = usize;

        fn next(&mut self) -> Option<Self::Item> {
            let len = unsafe { self.haystack.end.offset_from(self.cursor) as usize };
            let result = unsafe { self.scanner.find(None, self.cursor, len) };
            if result.is_valid() {
                let cursor = result.get_addr();
                let pos = unsafe { cursor.offset_from(self.haystack.start) as usize };
                self.cursor = unsafe { cursor.add(1) };
                Some(pos)
            } else {
                self.haystack.start = self.haystack.end;
                None
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Parameter {
    _4,
    _4Wild,
    _8,
    _8Wild,
    _16,
    _16Wild,
    _32,
    _32Wild,
}

impl Parameter {
    fn into_id(self, method: &str) -> BenchmarkId {
        let parameter = match self {
            Self::_4 => "4 bytes",
            Self::_4Wild => "4 bytes with wildcards",
            Self::_8 => "8 bytes",
            Self::_8Wild => "8 bytes with wildcards",
            Self::_16 => "16 bytes",
            Self::_16Wild => "16 bytes with wildcards",
            Self::_32 => "32 bytes",
            Self::_32Wild => "32 bytes with wildcards",
        };
        BenchmarkId::new(method, parameter)
    }
}

fn bench_simple(c: &mut Criterion) {
    let haystack = include_bytes!("../../../data/moby_dick.txt");
    let needles = [
        (Parameter::_4, "80 94 54 68"),
        (Parameter::_4Wild, "80 94 ? 68"),
        (Parameter::_8, "6E 64 20 74 68 65 20 41"),
        (Parameter::_8Wild, "? 64 20 74 ? ? 20 41"),
        (Parameter::_16, "72 65 2C 20 61 6E 64 20 73 77 6F 72 65 20 6E 6F"),
        (Parameter::_16Wild, "? 65 ? 20 61 ? 64 20 ? 77 6F ? ? 20 6E ?"),
        (Parameter::_32, "66 75 73 69 6E 67 20 77 69 64 65 20 76 65 69 6C 20 6F 66 20 6D 69 73 74 3B 20 6E 65 69 74 68 65"),
        (Parameter::_32Wild, "66 75 73 ? 6E 67 20 77 69 64 65 20 76 65 ? 6C 20 6F 66 ? ? 69 73 ? 3B ? 6E 65 69 74 68 ?"),
    ];

    let mut group = c.benchmark_group("iter-scanning");
    group.throughput(Throughput::Bytes(haystack.len() as u64));

    for (parameter, pattern) in needles {
        let id = parameter.into_id("aob");
        let needle = DynamicNeedle::from_ida(pattern).unwrap();
        group.bench_with_input(id, &needle, |b, needle| {
            b.iter(|| {
                let count = needle.find_iter(haystack).count();
                hint::black_box(count);
            });
        });

        let id = parameter.into_id("lightningscanner");
        let scanner = lightningscanner::Scanner::new(pattern);
        group.bench_with_input(id, &scanner, |b, scanner| {
            b.iter(|| {
                let iter = lightning::Iter::new(scanner, haystack);
                let count = iter.count();
                hint::black_box(count);
            });
        });
    }
}

criterion_group!(benches, bench_simple);
criterion_main!(benches);

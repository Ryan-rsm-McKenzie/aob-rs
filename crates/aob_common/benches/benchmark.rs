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

fn bench_simple(c: &mut Criterion) {
    let haystack = include_bytes!("../../../data/moby_dick.txt");
    let needles = [
        ("4 bytes", "80 94 54 68"),
        ("4 bytes w/ wildcards", "80 94 ? 68"),
        ("8 bytes", "6E 64 20 74 68 65 20 41"),
        ("8 bytes w/ wildcards", "? 64 20 74 ? ? 20 41"),
        ("16 bytes", "72 65 2C 20 61 6E 64 20 73 77 6F 72 65 20 6E 6F"),
        ("16 bytes w/ wildcards", "? 65 ? 20 61 ? 64 20 ? 77 6F ? ? 20 6E ?"),
        ("32 bytes", "66 75 73 69 6E 67 20 77 69 64 65 20 76 65 69 6C 20 6F 66 20 6D 69 73 74 3B 20 6E 65 69 74 68 65"),
        ("32 bytes w/ wildcards", "66 75 73 ? 6E 67 20 77 69 64 65 20 76 65 ? 6C 20 6F 66 ? ? 69 73 ? 3B ? 6E 65 69 74 68 ?"),
    ];

    let mut group = c.benchmark_group("Needle::find_iter");
    group.throughput(Throughput::Bytes(haystack.len() as u64));
    for (id, pattern) in needles {
        let id = BenchmarkId::from_parameter(id);
        let needle = DynamicNeedle::from_ida(pattern).unwrap();
        group.bench_with_input(id, &needle, |b, needle| {
            b.iter(|| {
                let count = needle.find_iter(haystack).count();
                hint::black_box(count);
            });
        });
    }
}

criterion_group!(benches, bench_simple);
criterion_main!(benches);

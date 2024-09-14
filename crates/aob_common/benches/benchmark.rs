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
        "69 6E ? 68 65 73",
        "74 68 61 74",
        "6F ?",
        "61 73",
        "70 72 ? ? ? 73 ? 3B E2 80 ? 74 6F",
        "61",
        "69 ?",
        "73 75 63 63 65 73 73 ? 75 ?",
        "74 68 69 ? 67 ?",
        "46 72 ? 6E 63 68",
        "? 68 ?",
        "76 6F 72 61 63 69 6F 75 73",
        "61 6C 6D 6F 73 74",
        "? 72 6F 73 73",
        "45 6D ? ?",
        "68 61 70 70 65 6E 73",
        "? 6C 6C",
        "69 ? 6E ? 74 65",
        "6F 66",
        "65 72 65 63 ?",
    ];

    let mut group = c.benchmark_group("Needle::find_iter");
    group.throughput(Throughput::Bytes(haystack.len() as u64));
    for pattern in needles {
        let needle = DynamicNeedle::from_ida(pattern).unwrap();
        group.bench_with_input(
            BenchmarkId::from_parameter(pattern),
            &needle,
            |b, needle| {
                b.iter(|| {
                    let count = needle.find_iter(haystack).count();
                    hint::black_box(count);
                });
            },
        );
    }
}

criterion_group!(benches, bench_simple);
criterion_main!(benches);

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

use listpack::Listpack;

fn bench_push_back(c: &mut Criterion) {
    c.bench_function("push_back 1000 small elements", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for _ in 0..1000 {
                lp.push_back(black_box(b"abc"));
            }
        });
    });
}

fn bench_extend_back(c: &mut Criterion) {
    let data = vec![b"abc".as_ref(); 1000];
    c.bench_function("extend_back 1000 elements", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            lp.extend_back(black_box(data.clone())).unwrap();
        });
    });
}

fn bench_extend_front(c: &mut Criterion) {
    let data = vec![b"abc".as_ref(); 1000];
    c.bench_function("extend_front 1000 elements", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            lp.extend_front(black_box(data.clone())).unwrap();
        });
    });
}

fn bench_push_back_with_reserve(c: &mut Criterion) {
    c.bench_function("push_back with reserve (1000)", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            lp.reserve(1000 * 4); // запас с учётом среднего размера
            for _ in 0..1000 {
                lp.push_back(black_box(b"abc"));
            }
        });
    });
}

fn bench_replace_grow(c: &mut Criterion) {
    c.bench_function("replace 100 elements (grow)", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for _ in 0..1000 {
                lp.push_back(b"abc");
            }
            for i in (0..100).map(|x| x * 10) {
                black_box(lp.replace(i, b"abcdef")); // больше
            }
        });
    });
}

fn bench_replace_shrink(c: &mut Criterion) {
    c.bench_function("replace 100 elements (shrink)", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for _ in 0..1000 {
                lp.push_back(b"abcdef");
            }
            for i in (0..100).map(|x| x * 10) {
                black_box(lp.replace(i, b"abc")); // меньше
            }
        });
    });
}

fn bench_push_front(c: &mut Criterion) {
    c.bench_function("push_front 1000 small elements", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for _ in 0..1000 {
                lp.push_front(black_box(b"abc"));
            }
        });
    });
}

fn bench_pop_back(c: &mut Criterion) {
    c.bench_function("pop_back 1000 elements", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for _ in 0..1000 {
                lp.push_back(b"abc");
            }
            for _ in 0..1000 {
                black_box(lp.pop_back());
            }
        });
    });
}

fn bench_pop_front(c: &mut Criterion) {
    c.bench_function("pop_front 1000 elements", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for _ in 0..1000 {
                lp.push_back(b"abc");
            }
            for _ in 0..1000 {
                black_box(lp.pop_front());
            }
        });
    });
}

fn bench_iterate(c: &mut Criterion) {
    let mut lp = Listpack::new();
    for _ in 0..1000 {
        lp.push_back(b"abc");
    }

    c.bench_function("iterate over 1000 elements", |b| {
        b.iter(|| {
            for item in lp.iter() {
                black_box(item);
            }
        });
    });
}

fn bench_get_random(c: &mut Criterion) {
    let mut lp = Listpack::new();
    for _ in 0..1000 {
        lp.push_back(b"abc");
    }

    c.bench_function("get 100 random elements", |b| {
        b.iter(|| {
            for i in (0..100).map(|x| x * 10) {
                black_box(lp.get(i));
            }
        });
    });
}

fn bench_remove(c: &mut Criterion) {
    c.bench_function("remove 100 elements from middle", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for _ in 0..1000 {
                lp.push_back(b"abc");
            }
            for _ in 0..100 {
                lp.remove(black_box(500));
            }
        });
    });
}

fn bench_replace_same_size(c: &mut Criterion) {
    c.bench_function("replace 100 elements (same size)", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for _ in 0..1000 {
                lp.push_back(b"abc");
            }
            for i in (0..100).map(|x| x * 10) {
                black_box(lp.replace(i, b"xyz")); // тот же размер
            }
        });
    });
}

criterion_group!(
    benches,
    bench_push_back,
    bench_push_front,
    bench_pop_back,
    bench_pop_front,
    bench_iterate,
    bench_get_random,
    bench_remove,
    bench_replace_same_size,
    bench_replace_grow,
    bench_replace_shrink,
    bench_extend_back,
    bench_extend_front,
    bench_push_back_with_reserve,
);
criterion_main!(benches);

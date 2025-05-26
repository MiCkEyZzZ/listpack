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

fn bench_push_integer(c: &mut Criterion) {
    let mut group = c.benchmark_group("integer_push");
    
    // Бенчмарк для 8-битных чисел
    group.bench_function("push_int8", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for i in -128i64..128 {
                lp.push_integer(black_box(i));
            }
        })
    });

    // Бенчмарк для 16-битных чисел
    group.bench_function("push_int16", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for i in -32768i64..32768 {
                lp.push_integer(black_box(i));
            }
        })
    });

    // Бенчмарк для 24-битных чисел (уменьшенный диапазон)
    group.bench_function("push_int24", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            // Тестируем только граничные значения и несколько промежуточных
            let values = [
                -(1 << 23),
                -(1 << 22),
                -(1 << 21),
                0,
                1 << 21,
                1 << 22,
                (1 << 23) - 1
            ];
            for &v in &values {
                lp.push_integer(black_box(v));
            }
        })
    });

    // Бенчмарк для 32-битных чисел (уменьшенный диапазон)
    group.bench_function("push_int32", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            // Тестируем только граничные значения и несколько промежуточных
            let values = [
                i32::MIN as i64,
                i32::MIN as i64 / 2,
                0,
                i32::MAX as i64 / 2,
                i32::MAX as i64
            ];
            for &v in &values {
                lp.push_integer(black_box(v));
            }
        })
    });

    group.finish();
}

fn bench_decode_integer(c: &mut Criterion) {
    let mut group = c.benchmark_group("integer_decode");
    
    // Подготовка данных для декодирования (уменьшенный набор)
    let mut lp = Listpack::new();
    let test_values = [
        i8::MIN as i64,
        i8::MAX as i64,
        i16::MIN as i64,
        i16::MAX as i64,
        -(1 << 23),
        (1 << 23) - 1,
        i32::MIN as i64,
        i32::MAX as i64,
        i64::MIN,
        i64::MAX,
    ];
    
    for &v in &test_values {
        lp.push_integer(v);
    }

    // Бенчмарк декодирования
    group.bench_function("decode_mixed", |b| {
        b.iter(|| {
            for i in 0..lp.len() {
                black_box(lp.decode_integer(lp.get(i).unwrap()).unwrap());
            }
        })
    });

    group.finish();
}

fn bench_mixed_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_operations");
    
    // Бенчмарк смешанных операций (уменьшенный набор)
    group.bench_function("push_pop_mixed", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            // Добавляем числа и строки (меньше итераций)
            for i in -100i64..100 {
                lp.push_integer(black_box(i));
                lp.push_back(black_box(b"test"));
            }
            // Удаляем всё
            while !lp.is_empty() {
                black_box(lp.pop_back());
            }
        })
    });

    group.finish();
}

fn bench_integer_encoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("integer_encoding");
    
    // Бенчмарк кодирования разных типов чисел
    let test_values = [
        i8::MIN as i64,
        i8::MAX as i64,
        i16::MIN as i64,
        i16::MAX as i64,
        -(1 << 23),
        (1 << 23) - 1,
        i32::MIN as i64,
        i32::MAX as i64,
        i64::MIN,
        i64::MAX,
    ];

    group.bench_function("encode_decode_edge_cases", |b| {
        b.iter(|| {
            let mut lp = Listpack::new();
            for &v in &test_values {
                lp.push_integer(black_box(v));
            }
            for i in 0..lp.len() {
                black_box(lp.decode_integer(lp.get(i).unwrap()).unwrap());
            }
        })
    });

    group.finish();
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
    bench_push_integer,
    bench_decode_integer,
    bench_mixed_operations,
    bench_integer_encoding
);
criterion_main!(benches);

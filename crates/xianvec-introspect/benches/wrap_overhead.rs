//! Criterion benchmark: `IntrospectionHook` (all flags off) vs `IdentityHook`
//! wrapping overhead on a random Tensor.
//!
//! Acceptance criterion: wrapped overhead ≤ 5% of unwrapped (Phase 4.4.1).

use std::hint::black_box;

use candle_core::{Device, Tensor};
use criterion::{criterion_group, criterion_main, Criterion};
use xianvec_inference::hooks::{HookContext, IdentityHook, LayerHook};
use xianvec_introspect::{CaptureFlags, IntrospectionHook};

fn bench_hooks(c: &mut Criterion) {
    let device = Device::Cpu;
    let dim = 5120usize;
    let data: Vec<f32> = (0..dim).map(|i| i as f32 / dim as f32).collect();
    let tensor = Tensor::from_vec(data, (dim,), &device).unwrap();
    let ctx = HookContext::new(0);

    let identity = IdentityHook;
    let wrapped = IntrospectionHook::new(IdentityHook, CaptureFlags::default());

    let mut group = c.benchmark_group("hook_overhead");

    group.bench_function("identity_hook", |b| {
        b.iter(|| {
            black_box(identity.apply(black_box(0), black_box(&tensor), black_box(&ctx)).unwrap())
        })
    });

    group.bench_function("introspection_hook_all_flags_off", |b| {
        b.iter(|| {
            black_box(wrapped.apply(black_box(0), black_box(&tensor), black_box(&ctx)).unwrap())
        })
    });

    group.finish();
}

criterion_group!(benches, bench_hooks);
criterion_main!(benches);

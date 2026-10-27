use criterion::{black_box, criterion_group, criterion_main, Criterion};
use envdoctor::scan::prefilter::quick_prefilter;
use envdoctor::scan::scan_source;

fn bench_prefilter(c: &mut Criterion) {
    let clean_code = r#"
        function fib(n: number): number {
            if (n <= 1) return n;
            return fib(n - 1) + fib(n - 2);
        }
        export const x = fib(10);
    "#;

    let env_code = r#"
        import os
        port = os.getenv("PORT", "8080")
        api_key = os.environ.get("API_KEY")
    "#;

    c.bench_function("prefilter_clean_ts", |b| {
        b.iter(|| quick_prefilter(black_box(clean_code.as_bytes()), "ts", "fib.ts"))
    });

    c.bench_function("prefilter_env_py", |b| {
        b.iter(|| quick_prefilter(black_box(env_code.as_bytes()), "py", "app.py"))
    });
}

fn bench_ast_scan(c: &mut Criterion) {
    let ts_source = r#"
        const port = process.env.PORT || '3000';
        const apiKey = process.env.API_KEY;
        const dbUrl = process.env.DATABASE_URL;
        console.log(`Listening on ${port}`);
    "#;

    c.bench_function("ast_scan_ts", |b| {
        b.iter(|| scan_source(black_box(ts_source), "src/index.ts"))
    });
}

criterion_group!(benches, bench_prefilter, bench_ast_scan);
criterion_main!(benches);

#![allow(clippy::disallowed_methods, clippy::field_reassign_with_default)]

use envdoctor::cache::{ScanCache, DEFAULT_CACHE_FILE};
use envdoctor::engine::{run_engine, EngineConfig};
use envdoctor::scan::prefilter::quick_prefilter;
use envdoctor::DiagnosticCode;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_prefilter_rules_out_clean_files_instantly() {
    let clean_rust = r#"
        pub fn fib(n: u32) -> u32 {
            match n {
                0 => 0,
                1 => 1,
                _ => fib(n - 1) + fib(n - 2),
            }
        }
    "#;
    assert!(!quick_prefilter(clean_rust.as_bytes(), "rs", "fib.rs"));

    let env_rust = r#"
        pub fn get_port() -> String {
            std::env::var("APP_PORT").unwrap_or_else(|_| "8080".into())
        }
    "#;
    assert!(quick_prefilter(env_rust.as_bytes(), "rs", "port.rs"));

    let clean_ts = r#"
        export const add = (a: number, b: number): number => a + b;
    "#;
    assert!(!quick_prefilter(clean_ts.as_bytes(), "ts", "math.ts"));

    let env_ts = r#"
        export const dbUrl = process.env.DATABASE_URL;
    "#;
    assert!(quick_prefilter(env_ts.as_bytes(), "ts", "db.ts"));
}

#[test]
fn test_content_hash_cache_workflow() {
    let dir = tempdir().unwrap();
    let contract_path = dir.path().join(".env.example");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(&contract_path, "API_KEY=\nDATABASE_URL=\nPORT=\n").unwrap();

    let file_a = src_dir.join("a.ts");
    let file_b = src_dir.join("b.ts");
    let file_c = src_dir.join("clean.ts");

    fs::write(&file_a, "const key = process.env.API_KEY;").unwrap();
    fs::write(&file_b, "const db = process.env.DATABASE_URL;").unwrap();
    fs::write(&file_c, "export const helper = 42;").unwrap();

    let cache_file = dir.path().join(DEFAULT_CACHE_FILE);

    let mut config = EngineConfig::default();
    config.contract_path = contract_path.clone();
    config.scan_paths = vec![src_dir.clone()];
    config.use_cache = true;
    config.cache_path = Some(cache_file.clone());

    // Run 1: Initial run populates cache
    let report1 = run_engine(&config);
    let env001_count1 = report1
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV001)
        .count();
    assert_eq!(env001_count1, 0);
    assert!(cache_file.exists());

    let cache1 = ScanCache::load(&cache_file).expect("Cache must exist after run 1");
    assert!(cache1.entries.len() >= 2);

    // Run 2: Re-run with no modifications should hit cache
    let report2 = run_engine(&config);
    assert_eq!(report1.summary, report2.summary);

    // Run 3: Modify only file_b to introduce an undocumented env variable
    fs::write(&file_b, "const redis = process.env.REDIS_HOST;").unwrap();
    let report3 = run_engine(&config);
    let env001_count3 = report3
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV001)
        .count();
    assert_eq!(env001_count3, 1);

    // Run 4: Test with caching disabled
    let mut no_cache_config = config.clone();
    no_cache_config.use_cache = false;
    let report4 = run_engine(&no_cache_config);
    assert_eq!(report3.summary, report4.summary);
}

#[test]
fn test_deterministic_parallel_scan_ordering() {
    let dir = tempdir().unwrap();
    let contract_path = dir.path().join(".env.example");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(&contract_path, "VAR_A=\nVAR_B=\nVAR_C=\n").unwrap();

    // Generate 30 files with varying accesses to test Rayon parallel ordering
    for i in 0..30 {
        let fpath = src_dir.join(format!("file_{:02}.ts", i));
        let var_name = match i % 3 {
            0 => "VAR_A",
            1 => "VAR_B",
            _ => "VAR_C",
        };
        fs::write(
            fpath,
            format!("// file {}\nconst val = process.env.{};\n", i, var_name),
        )
        .unwrap();
    }

    let mut config = EngineConfig::default();
    config.contract_path = contract_path;
    config.scan_paths = vec![src_dir];
    config.use_cache = false; // Fresh scans to test concurrency ordering

    let report_a = run_engine(&config);
    let report_b = run_engine(&config);

    // Verify reports are 100% deterministic and identical across runs
    assert_eq!(report_a.summary, report_b.summary);
    assert_eq!(report_a.diagnostics.len(), report_b.diagnostics.len());
    for (d_a, d_b) in report_a.diagnostics.iter().zip(report_b.diagnostics.iter()) {
        assert_eq!(d_a.code, d_b.code);
        assert_eq!(d_a.locations, d_b.locations);
    }
}

#[test]
fn test_bounded_batch_memory_handling() {
    let dir = tempdir().unwrap();
    let contract_path = dir.path().join(".env.example");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(&contract_path, "PORT=\n").unwrap();

    // Create 70 files (exceeding BATCH_SIZE=64)
    for i in 0..70 {
        let fpath = src_dir.join(format!("batch_{:03}.js", i));
        fs::write(fpath, "const p = process.env.PORT;\n").unwrap();
    }

    let mut config = EngineConfig::default();
    config.contract_path = contract_path;
    config.scan_paths = vec![src_dir];
    config.use_cache = false;

    let report = run_engine(&config);
    assert_eq!(report.summary.scanned_files, 70);
    assert!(report.summary.complete);
}

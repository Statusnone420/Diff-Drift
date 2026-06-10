use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use criterion::{criterion_group, criterion_main, Criterion};
use diff_drift_lib::diff::diff_nodes;
use diff_drift_lib::parse::{parse_file, Lang};
use diff_drift_lib::session::{analyze_all, Baseline};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn representative_ts() -> String {
    let mut src = String::from("import { verify } from \"./crypto\";\n\n");
    for i in 0..20 {
        src.push_str(&format!(
            "export function validateToken{i}(token: string): boolean {{\n  const pattern = /^[A-Z0-9]{{12,64}}$/;\n  if (!pattern.test(token)) {{\n    throw new Error(\"bad token\");\n  }}\n  return verify(token);\n}}\n\n"
        ));
    }
    src
}

struct BenchRepo {
    root: PathBuf,
}

impl BenchRepo {
    fn new(files: usize) -> Self {
        let root = std::env::temp_dir().join(format!(
            "diff-drift-bench-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        force_remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create bench repo");

        for i in 0..files {
            std::fs::write(root.join(format!("route{i}.ts")), baseline_file(i))
                .expect("write baseline file");
        }

        let repo = git2::Repository::init(&root).expect("git init");
        let mut index = repo.index().expect("index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("git add");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = git2::Signature::now("Bench", "bench@diff-drift.local").expect("signature");
        repo.commit(Some("HEAD"), &sig, &sig, "baseline", &tree, &[])
            .expect("commit baseline");

        for i in 0..files {
            std::fs::write(root.join(format!("route{i}.ts")), drift_file(i))
                .expect("write drift file");
        }

        Self { root }
    }
}

impl Drop for BenchRepo {
    fn drop(&mut self) {
        force_remove_dir_all(&self.root);
    }
}

fn baseline_file(i: usize) -> String {
    format!(
        "export function route{i}(token: string): boolean {{\n  const pattern = /^[A-Z0-9]{{12,64}}$/;\n  if (!pattern.test(token)) {{\n    throw new Error(\"bad token\");\n  }}\n  return verify(token, PUBLIC_KEY);\n}}\n"
    )
}

fn drift_file(i: usize) -> String {
    format!(
        "export function route{i}(token: string): boolean {{\n  const pattern = /.*/;\n  if (false) {{\n    throw new Error(\"bad token\");\n  }}\n  return decode(token);\n}}\n"
    )
}

fn force_remove_dir_all(p: &Path) {
    fn clear_readonly(p: &Path) {
        if let Ok(meta) = std::fs::symlink_metadata(p) {
            let mut perm = meta.permissions();
            if perm.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                perm.set_readonly(false);
                let _ = std::fs::set_permissions(p, perm);
            }
            if meta.is_dir() {
                if let Ok(rd) = std::fs::read_dir(p) {
                    for e in rd.flatten() {
                        clear_readonly(&e.path());
                    }
                }
            }
        }
    }

    if p.exists() {
        clear_readonly(p);
        let _ = std::fs::remove_dir_all(p);
    }
}

fn bench_parse_file(c: &mut Criterion) {
    let src = representative_ts();
    c.bench_function("parse_file/100_line_ts", |b| {
        b.iter(|| parse_file(black_box(&src), black_box(Lang::Ts)))
    });
}

fn bench_diff_nodes(c: &mut Criterion) {
    let before = parse_file(&representative_ts(), Lang::Ts);
    let after = parse_file(
        &representative_ts()
            .replace("return verify(token);", "return decode(token);")
            .replace(
                "const pattern = /^[A-Z0-9]{12,64}$/;",
                "const pattern = /.*/;",
            ),
        Lang::Ts,
    );
    c.bench_function("diff_nodes/representative_ts", |b| {
        b.iter(|| diff_nodes(black_box(&before), black_box(&after)))
    });
}

fn bench_analyze_all(c: &mut Criterion) {
    let repo = BenchRepo::new(25);
    c.bench_function("analyze_all/25_drifted_files", |b| {
        b.iter(|| analyze_all(black_box(&repo.root), black_box(&Baseline::default())))
    });
}

fn criterion_config() -> Criterion {
    Criterion::default().sample_size(20)
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets = bench_parse_file, bench_diff_nodes, bench_analyze_all
}
criterion_main!(benches);

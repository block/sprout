use std::path::{Path, PathBuf};

const MAX_OUTPUT_BYTES: usize = 50 * 1024;
const MAX_OUTPUT_LINES: usize = 2000;
const MAX_WALK_DEPTH: usize = 50;
const SKIP_DIRS: &[&str] = &["target", "node_modules", "dist", "build"];

pub fn run(args: Vec<String>) -> i32 {
    let (root, max_depth) = match parse(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("tree: {e}");
            return 2;
        }
    };
    if !root.is_dir() {
        eprintln!("tree: not a directory: {}", root.display());
        return 2;
    }
    let mut out = Vec::new();
    let total = collect(&root, "", max_depth, 0, &mut out, MAX_OUTPUT_LINES);
    let name = root
        .file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy();
    println!("{name}/  [{total}]");
    let mut bytes = 0usize;
    let limit = MAX_OUTPUT_LINES.saturating_sub(1);
    let truncated = out.len() > limit;
    for line in out.into_iter().take(limit) {
        if bytes + line.len() + 1 > MAX_OUTPUT_BYTES {
            println!("[truncated]");
            return 0;
        }
        println!("{line}");
        bytes += line.len() + 1;
    }
    if truncated {
        println!("[truncated]");
    }
    0
}

fn parse(args: Vec<String>) -> Result<(PathBuf, usize), String> {
    let mut depth = MAX_WALK_DEPTH;
    let mut path = PathBuf::from(".");
    let mut path_set = false;
    let mut iter = args.into_iter();
    while let Some(a) = iter.next() {
        match a.as_str() {
            "-d" | "--depth" => {
                let n = iter.next().ok_or("missing value for --depth")?;
                depth = n.parse::<usize>().map_err(|_| format!("bad depth: {n}"))?;
            }
            s if s.starts_with('-') => return Err(format!("unknown flag: {s}")),
            _ => {
                if path_set {
                    return Err("multiple paths not supported".to_string());
                }
                path = PathBuf::from(a);
                path_set = true;
            }
        }
    }
    Ok((path, depth.min(MAX_WALK_DEPTH)))
}

fn collect(
    dir: &Path,
    prefix: &str,
    max_depth: usize,
    depth: usize,
    out: &mut Vec<String>,
    line_budget: usize,
) -> usize {
    if out.len() >= line_budget {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if !SKIP_DIRS.contains(&name) {
                dirs.push(path)
            }
        } else if ft.is_file() {
            files.push(path);
        }
    }
    dirs.sort();
    files.sort();
    let mut total = 0usize;
    let child_prefix = format!("{prefix}  ");
    for d in &dirs {
        let Some(name) = d.file_name().map(|n| n.to_string_lossy()) else { continue };
        if out.len() >= line_budget { break }
        if depth < max_depth {
            let idx = out.len();
            out.push(String::new());
            let sub = collect(d, &child_prefix, max_depth, depth + 1, out, line_budget);
            out[idx] = format!("{prefix}{name}/  [{sub}]");
            total += sub;
        } else {
            out.push(format!("{prefix}{name}/"));
        }
    }
    for f in &files {
        if out.len() >= line_budget { break }
        let Some(name) = f.file_name().map(|n| n.to_string_lossy()) else { continue };
        let lc = std::fs::read(f)
            .map(|b| {
                if b.is_empty() {
                    0
                } else {
                    b.iter().filter(|&&c| c == b'\n').count()
                        + if b.last() != Some(&b'\n') { 1 } else { 0 }
                }
            })
            .unwrap_or(0);
        total += lc;
        out.push(format!("{prefix}{name}  [{lc}]"));
    }
    total
}

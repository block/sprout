use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(args: Vec<String>) -> i32 {
    if let Some(code) = try_system_rg(&args) {
        return code;
    }
    fallback(args)
}

fn try_system_rg(args: &[String]) -> Option<i32> {
    let self_exe = std::env::current_exe().ok()?;
    let self_canon = std::fs::canonicalize(&self_exe).ok()?;
    let cleaned_path = clean_path(&self_canon);
    let candidate = which_rg(&cleaned_path)?;

    let status = Command::new(&candidate)
        .args(args)
        .env("PATH", &cleaned_path)
        .status()
        .ok()?;
    Some(status.code().unwrap_or(2))
}

fn clean_path(self_canon: &Path) -> String {
    let original = std::env::var("PATH").unwrap_or_default();
    original
        .split(':')
        .filter(|dir| {
            if dir.is_empty() {
                return false;
            }
            let candidate = Path::new(dir).join("rg");
            match std::fs::canonicalize(&candidate) {
                Ok(c) => c != *self_canon,
                Err(_) => true,
            }
        })
        .collect::<Vec<_>>()
        .join(":")
}

fn which_rg(path: &str) -> Option<PathBuf> {
    for dir in path.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join("rg");
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_p: &Path) -> bool {
    true
}

struct RgArgs {
    pattern: Option<String>,
    paths: Vec<PathBuf>,
    line_numbers: bool,
    ignore_case: bool,
    files_only: bool,
    list_files_with_matches: bool,
    context: usize,
    glob: Option<String>,
}

fn parse(args: Vec<String>) -> Result<RgArgs, String> {
    let mut out = RgArgs {
        pattern: None,
        paths: Vec::new(),
        line_numbers: false,
        ignore_case: false,
        files_only: false,
        list_files_with_matches: false,
        context: 0,
        glob: None,
    };
    let mut iter = args.into_iter();
    let mut positional: Vec<String> = Vec::new();
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--files" => out.files_only = true,
            "-n" | "--line-number" => out.line_numbers = true,
            "-i" | "--ignore-case" => out.ignore_case = true,
            "-l" | "--files-with-matches" => out.list_files_with_matches = true,
            "-C" | "--context" => {
                let n = iter.next().ok_or("missing value for -C")?;
                out.context = n.parse().map_err(|_| format!("bad -C value: {n}"))?;
            }
            "-g" | "--glob" => {
                out.glob = Some(iter.next().ok_or("missing value for -g")?);
            }
            "--" => positional.extend(iter.by_ref()),
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unsupported flag (fallback rg): {s}"));
            }
            _ => positional.push(a),
        }
    }
    if out.files_only {
        out.paths = positional.into_iter().map(PathBuf::from).collect();
        if out.paths.is_empty() {
            out.paths.push(PathBuf::from("."));
        }
    } else {
        let mut it = positional.into_iter();
        out.pattern = it.next();
        out.paths = it.map(PathBuf::from).collect();
        if out.paths.is_empty() {
            out.paths.push(PathBuf::from("."));
        }
    }
    Ok(out)
}

fn fallback(args: Vec<String>) -> i32 {
    let opts = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("rg (fallback): {e}");
            return 2;
        }
    };

    let mut found_any = false;
    let mut printed_files: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    if opts.files_only {
        for root in &opts.paths {
            walk(root, &opts, &mut |p| {
                println!("{}", p.display());
                found_any = true;
            });
        }
        return if found_any { 0 } else { 1 };
    }

    let pattern = match &opts.pattern {
        Some(p) => p.clone(),
        None => {
            eprintln!("rg (fallback): missing PATTERN");
            return 2;
        }
    };
    let needle = if opts.ignore_case {
        pattern.to_lowercase()
    } else {
        pattern.clone()
    };

    for root in &opts.paths {
        walk(root, &opts, &mut |path| {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(_) => return,
            };
            let lines: Vec<&str> = text.lines().collect();
            let mut matches: Vec<usize> = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                let hay = if opts.ignore_case {
                    line.to_lowercase()
                } else {
                    (*line).to_string()
                };
                if hay.contains(&needle) {
                    matches.push(i);
                }
            }
            if matches.is_empty() {
                return;
            }
            found_any = true;
            if opts.list_files_with_matches {
                if printed_files.insert(path.to_path_buf()) {
                    println!("{}", path.display());
                }
                return;
            }
            let mut shown: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
            for m in &matches {
                let lo = m.saturating_sub(opts.context);
                let hi = (m + opts.context).min(lines.len().saturating_sub(1));
                for i in lo..=hi {
                    shown.insert(i);
                }
            }
            for i in shown {
                let prefix = if opts.line_numbers {
                    format!("{}:{}:", path.display(), i + 1)
                } else {
                    format!("{}:", path.display())
                };
                println!("{prefix}{}", lines[i]);
            }
        });
    }
    if found_any {
        0
    } else {
        1
    }
}

fn walk(root: &Path, opts: &RgArgs, on_file: &mut dyn FnMut(&Path)) {
    if root.is_file() {
        if accept(root, opts) {
            on_file(root);
        }
        return;
    }
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if matches!(name, "target" | "node_modules" | "dist" | "build") {
                    continue;
                }
                stack.push(path);
            } else if accept(&path, opts) {
                on_file(&path);
            }
        }
    }
}

fn accept(path: &Path, opts: &RgArgs) -> bool {
    match &opts.glob {
        None => true,
        Some(g) => glob_match(g, path),
    }
}

fn glob_match(pattern: &str, path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let full = path.to_string_lossy();
    simple_glob(pattern, name) || simple_glob(pattern, &full)
}

fn simple_glob(pattern: &str, text: &str) -> bool {
    let mut p: Vec<char> = pattern.chars().collect();
    let mut t: Vec<char> = text.chars().collect();
    glob_recurse(&mut p, 0, &mut t, 0)
}

fn glob_recurse(p: &mut Vec<char>, pi: usize, t: &mut Vec<char>, ti: usize) -> bool {
    if pi == p.len() {
        return ti == t.len();
    }
    match p[pi] {
        '*' => {
            for end in ti..=t.len() {
                if glob_recurse(p, pi + 1, t, end) {
                    return true;
                }
            }
            false
        }
        '?' => ti < t.len() && glob_recurse(p, pi + 1, t, ti + 1),
        c => ti < t.len() && t[ti] == c && glob_recurse(p, pi + 1, t, ti + 1),
    }
}

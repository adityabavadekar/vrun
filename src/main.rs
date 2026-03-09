use clap::{Parser, Subcommand};
use colored::Colorize;
use log::{Level, error, info, warn};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
};
use std::{
    process::{Command, Stdio},
    time::Instant,
};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Listen for Competitive Companion testcases
    Listen {
        /// Base directory (testcases/ will be created inside)
        #[arg(long)]
        source_dir: Option<String>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Compile and run C++ code using testcases
    Run {
        /// Path to C++ source file
        source: String,

        /// Base directory where testcases/ exists
        #[arg(long)]
        source_dir: Option<String>,

        /// Interactive mode
        #[arg(short, long)]
        interactive: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,

        /// Custom input file (skips testcase discovery)
        #[arg(long = "in")]
        input_file: Option<String>,

        /// Custom expected output file (used with --in)
        #[arg(long = "exp")]
        expected_file: Option<String>,

        /// Skip compilation and use the existing binary
        #[arg(long = "nc")]
        no_compile: bool,
    },

    /// Stress test: run solution vs brute force using a generator
    Stress {
        /// Path to the brute force C++ source file
        brute: String,

        /// Path to the generator C++ source file (receives seed as first arg)
        generator: String,

        /// Path to your solution's C++ source file
        solution: String,

        /// Base directory
        #[arg(long)]
        source_dir: Option<String>,

        /// Number of stress test iterations (0 = infinite)
        #[arg(short, long, default_value = "0")]
        count: usize,

        /// Stop on first failure
        #[arg(long, default_value = "true")]
        stop_on_fail: bool,

        /// Starting seed value
        #[arg(long, default_value = "1")]
        seed: usize,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,

        /// Skip compilation and use the existing binaries
        #[arg(long = "nc")]
        no_compile: bool,
    },
}

const TESTCASES_DIR: &str = "testcases";
const CPH_DIR: &str = ".cph";

#[derive(Deserialize, Serialize)]
struct Payload {
    name: String,
    group: Option<String>,
    tests: Vec<Test>,
}

#[derive(Deserialize, Serialize)]
struct Test {
    input: String,
    output: String,
}

#[derive(Deserialize)]
struct CphProb {
    name: Option<String>,
    tests: Vec<CphTest>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct CphTest {
    id: serde_json::Value,
    input: String,
    output: String,
}

fn sanitize(s: &str) -> String {
    let mut out = String::new();
    let mut last_was_underscore = false;

    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_was_underscore = false;
        } else if !last_was_underscore {
            out.push('_');
            last_was_underscore = true;
        }
    }

    out.trim_matches('_').to_string()
}

fn expand_path(path: &str) -> PathBuf {
    if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    if path == "." {
        if let Ok(cwd) = std::env::current_dir() {
            return cwd;
        }
    }
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    PathBuf::from(path)
}

fn resolve_base(explicit_dir: &Option<String>, fallback_file: Option<&str>) -> PathBuf {
    if let Some(dir) = explicit_dir {
        return expand_path(dir);
    }
    if let Some(file) = fallback_file {
        let p = Path::new(file);
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                return parent.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn main() {
    let cli = Cli::parse();
    let verbose = match &cli.command {
        Commands::Run { verbose, .. } => *verbose,
        Commands::Stress { verbose, .. } => *verbose,
        Commands::Listen { verbose, .. } => *verbose,
    };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(if verbose {
        "debug"
    } else {
        "info"
    }))
    .format(|buf, record| {
        let level = match record.level() {
            Level::Error => "[  ERROR ]".red(),
            Level::Warn => "[  WARN  ]".yellow(),
            Level::Info => "[  INFO  ]".green(),
            Level::Debug => "[  DEBUG ]".blue(),
            Level::Trace => "[  TRACE ]".normal(),
        };
        writeln!(buf, "{} {}", level, record.args())
    })
    .init();

    match cli.command {
        Commands::Listen { source_dir, .. } => {
            let base = resolve_base(&source_dir, None);
            listen_mode(base);
        }

        Commands::Run {
            source,
            source_dir,
            interactive,
            verbose,
            input_file,
            expected_file,
            no_compile,
        } => {
            let base = resolve_base(&source_dir, Some(&source));
            run_mode(
                &source,
                base,
                verbose,
                interactive,
                input_file.as_deref(),
                expected_file.as_deref(),
                no_compile,
            );
        }

        Commands::Stress {
            solution,
            brute,
            generator,
            source_dir,
            count,
            stop_on_fail,
            seed,
            verbose,
            no_compile,
        } => {
            let base = resolve_base(&source_dir, Some(&solution));
            stress_mode(
                &solution,
                &brute,
                &generator,
                base,
                count,
                stop_on_fail,
                seed,
                verbose,
                no_compile,
            );
        }
    }
}

fn compile_cpp(source_path: &std::path::Path, out_exe: &std::path::Path, label: &str) -> bool {
    let start = Instant::now();
    info!("Compiling {} → {}", label, out_exe.display());

    let c = Command::new("g++")
        .args(["-std=gnu++17", "-O2", "-pipe", "-Wall", "-Wextra"])
        .arg(source_path)
        .arg("-o")
        .arg(out_exe)
        .output()
        .expect("Failed to invoke g++");

    if !c.status.success() {
        eprintln!(
            "{}",
            format!("Compilation failed for {}:", label).bold().red()
        );
        eprintln!("{}", String::from_utf8_lossy(&c.stderr));
        return false;
    }

    info!(
        "  {} compiled ({:.2}s)",
        label,
        start.elapsed().as_secs_f64()
    );
    true
}

/// Path of the hidden cache file that stores source-file hashes.
const CACHE_FILENAME: &str = ".vrun_cache";

fn cache_file_path(base_dir: &Path) -> PathBuf {
    base_dir.join("temp").join(CACHE_FILENAME)
}

/// FNV-1a 64-bit hash of the entire file contents.
fn file_hash(path: &Path) -> Option<u64> {
    let bytes = fs::read(path).ok()?;
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    Some(h)
}

/// Parse cache file into a map of absolute path to hash.
/// Format per line: <16-hex-digits> <absolute_path>
fn read_cache(cache_file: &Path) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    let Ok(content) = fs::read_to_string(cache_file) else {
        return map;
    };
    for line in content.lines() {
        let mut parts = line.splitn(2, ' ');
        if let (Some(hash_str), Some(src)) = (parts.next(), parts.next()) {
            if let Ok(hash) = u64::from_str_radix(hash_str, 16) {
                map.insert(src.to_string(), hash);
            }
        }
    }
    map
}

fn write_cache(cache_file: &Path, map: &HashMap<String, u64>) {
    let content: String = map
        .iter()
        .map(|(path, hash)| format!("{:016x} {}\n", hash, path))
        .collect();
    let _ = fs::write(cache_file, content);
}

/// Store the hash of source_path in the cache.
fn update_cache(cache_file: &Path, source_path: &Path) {
    if let Some(hash) = file_hash(source_path) {
        let mut map = read_cache(cache_file);
        map.insert(source_path.to_string_lossy().into_owned(), hash);
        write_cache(cache_file, &map);
    }
}

fn is_up_to_date(cache_file: &Path, source_path: &Path, exe_path: &Path) -> bool {
    if !exe_path.exists() {
        return false;
    }
    let Some(current_hash) = file_hash(source_path) else {
        return false;
    };
    let key = source_path.to_string_lossy().into_owned();
    read_cache(cache_file).get(&key) == Some(&current_hash)
}

fn compile_if_needed(
    source_path: &Path,
    exe_path: &Path,
    label: &str,
    cache_file: &Path,
    no_compile: bool,
) -> bool {
    if no_compile {
        if !exe_path.exists() {
            error!(
                "--nc specified but no binary found at {}",
                exe_path.display()
            );
            return false;
        }
        info!("--nc: reusing existing binary {}", exe_path.display());
        return true;
    }

    if is_up_to_date(cache_file, source_path, exe_path) {
        info!("'{}' is up-to-date, skipping recompilation", label);
        return true;
    }

    let ok = compile_cpp(source_path, exe_path, label);
    if ok {
        update_cache(cache_file, source_path);
    }
    ok
}

fn run_exe(exe: &std::path::Path, input: &str) -> Option<(String, String)> {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    child.stdin.as_mut()?.write_all(input.as_bytes()).ok()?;

    let out = child.wait_with_output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Some((stdout, stderr))
}

const DIFF_PATH: &str = "temp/vrun_diff.txt";
/// Maximum lines to print inline before switching to a summary.
const INLINE_LINE_LIMIT: usize = 60;

/// Write a unified diff of `expected` vs `actual` to DIFF_PATH
/// Tries the system `diff` binary first, then
/// falls back to the similar crate.
fn write_diff_to_file(expected: &str, actual: &str) {
    use std::io::Write;

    let system_diff_ok = (|| -> std::io::Result<bool> {
        let mut exp_tmp = tempfile::NamedTempFile::new()?;
        let mut act_tmp = tempfile::NamedTempFile::new()?;
        exp_tmp.write_all(expected.as_bytes())?;
        act_tmp.write_all(actual.as_bytes())?;
        exp_tmp.flush()?;
        act_tmp.flush()?;

        let out = Command::new("diff")
            .arg("-u")
            .arg("--label")
            .arg("expected")
            .arg("--label")
            .arg("actual")
            .arg(exp_tmp.path())
            .arg(act_tmp.path())
            .output()?;

        // diff exits 0 = same, 1 = different, 2 = error
        if out.status.code() == Some(2) {
            return Ok(false);
        }
        fs::write(DIFF_PATH, &out.stdout)?;
        Ok(true)
    })();

    match system_diff_ok {
        Ok(true) => return,
        _ => {}
    }

    let norm = |s: &str| {
        s.lines()
            .map(|l| l.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let exp_n = norm(expected);
    let act_n = norm(actual);
    let diff = TextDiff::from_lines(&exp_n, &act_n);

    let mut out = String::new();
    for change in diff.iter_all_changes() {
        let prefix = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        out.push_str(prefix);
        out.push_str(&change.to_string());
    }
    let _ = fs::write(DIFF_PATH, &out);
}

fn normalize(s: &str) -> String {
    s.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_block(label: &str, text: &str) {
    let lines: Vec<&str> = text.lines().collect();
    eprintln!("{}:", label);
    eprintln!("--------------------");
    if lines.len() <= INLINE_LINE_LIMIT {
        eprintln!("{}", text);
    } else {
        eprintln!("<{} lines — too large to print inline>", lines.len());
    }
    eprintln!("--------------------");
}

fn print_test_result(
    label: &str,
    input: &str,
    expected: &str,
    actual: &str,
    stderr: &str,
    time_str: &str,
    verbose: bool,
) -> bool {
    let passed = normalize(actual) == normalize(expected);

    if passed {
        println!(
            "{} ({})",
            format!("[ AC ] {} PASSED", label).bold().green(),
            time_str
        );
    } else {
        println!(
            "{} ({})",
            format!(">>> [ WA ] {} FAILED", label).bold().red(),
            time_str
        );
    }

    if !passed || verbose {
        print_block("Input", input);
        print_block("Expected Output", expected);
        print_block("Your Output", actual);

        if !passed {
            write_diff_to_file(expected, actual);
            eprintln!(
                "Diff written to {} (open with: less -R {})",
                DIFF_PATH, DIFF_PATH
            );
        }

        if !stderr.is_empty() {
            print_block("Debug Output (stderr)", stderr);
        }
        eprintln!();
    }

    passed
}

fn format_time(d: std::time::Duration) -> String {
    format!("{} s", d.as_secs_f32())
}

fn listen_mode(source_dir: PathBuf) {
    let listener = TcpListener::bind("127.0.0.1:10045").expect("Failed to bind port 10045");
    info!("Listening for Competitive Companion on 127.0.0.1:10045");

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => {
                warn!("Connection error: {}", e);
                continue;
            }
        };

        let mut request = String::new();
        if stream.read_to_string(&mut request).is_err() {
            warn!("Failed to read request");
            continue;
        }

        let body = match request.split("\r\n\r\n").nth(1) {
            Some(b) => b,
            None => {
                error!("Malformed HTTP request");
                continue;
            }
        };

        let payload: Payload = match serde_json::from_str(body) {
            Ok(p) => p,
            Err(e) => {
                error!("Invalid JSON payload: {}", e);
                continue;
            }
        };

        if payload.tests.is_empty() {
            warn!("No testcases received");
            continue;
        }

        let dir = source_dir.join(TESTCASES_DIR);
        if let Err(e) = fs::create_dir_all(&dir) {
            error!("Failed to create directory {}: {}", dir.display(), e);
            continue;
        }

        let problem = sanitize(&payload.name);
        for (i, t) in payload.tests.iter().enumerate() {
            let input_path = dir.join(format!("{}_input{}.txt", problem, i));
            let output_path = dir.join(format!("{}_output{}.txt", problem, i));

            if let Err(e) = fs::write(&input_path, t.input.trim_end().to_string() + "\n") {
                error!("Failed to write {}: {}", input_path.display(), e);
                continue;
            }

            if let Err(e) = fs::write(&output_path, t.output.trim_end().to_string() + "\n") {
                error!("Failed to write {}: {}", output_path.display(), e);
                continue;
            }
        }

        info!(
            "Saved {} testcases → {}_input{{N}}.txt",
            payload.tests.len(),
            problem
        );

        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK");
    }
}

use std::path::Path;

fn run_interactive_loop(exe: &std::path::Path) {
    println!(
        "{}",
        "[ INTERACTIVE MODE ] Ctrl+D → restart | Ctrl+C → exit"
            .bold()
            .yellow()
    );

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    while running.load(Ordering::SeqCst) {
        println!(
            "{}",
            "\n--- Program started (Ctrl+D to end run) ---"
                .cyan()
                .bold()
        );

        let status = Command::new(exe)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();

        match status {
            Ok(s) => println!(
                "{} {:?}",
                "[ PROGRAM EXITED ]".blue(),
                s.code().unwrap_or(-1)
            ),
            Err(e) => {
                eprintln!("{}", format!("Failed to run program: {}", e).red());
                break;
            }
        }

        if !running.load(Ordering::SeqCst) {
            break;
        }

        println!(
            "{}",
            "--- Press Enter to run again (Ctrl+C to exit) ---".dimmed()
        );
        let mut _buf = String::new();
        let _ = std::io::stdin().read_line(&mut _buf);
    }

    println!("{}", "\n[ INTERACTIVE MODE EXITED ]".bold().green());
}

enum TestSource {
    /// `--in` flag: redirect the file directly as stdin; optional expected output file.
    DirectFile {
        input_path: PathBuf,
        expected_path: Option<PathBuf>,
    },
    /// Auto-detected CPH `.prob` file: tests are already in memory as strings.
    CphProb { tests: Vec<(usize, String, String)> },
    /// Regular on-disk testcase pairs: redirect each input file directly as stdin.
    FilePairs(Vec<(usize, PathBuf, PathBuf)>),
}

impl TestSource {
    fn len(&self) -> usize {
        match self {
            TestSource::DirectFile { .. } => 1,
            TestSource::CphProb { tests } => tests.len(),
            TestSource::FilePairs(pairs) => pairs.len(),
        }
    }
}

fn run_one_test(
    exe: &Path,
    stdin_file: Option<std::fs::File>,
    stdin_bytes: Option<Vec<u8>>,
) -> Option<(String, String)> {
    use std::io::Write;

    let stdin_cfg = if stdin_file.is_some() {
        Stdio::from(stdin_file.unwrap())
    } else {
        Stdio::piped()
    };

    let mut child = Command::new(exe)
        .stdin(stdin_cfg)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    // When bytes are provided (CphProb / string), write them in a separate thread
    if let Some(bytes) = stdin_bytes {
        let mut stdin_handle = child.stdin.take()?;
        std::thread::spawn(move || {
            let _ = stdin_handle.write_all(&bytes);
        });
    }

    let out = child.wait_with_output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Some((stdout, stderr))
}

fn run_mode(
    source: &str,
    base_dir: std::path::PathBuf,
    verbose: bool,
    interactive: bool,
    input_file: Option<&str>,
    expected_file: Option<&str>,
    no_compile: bool,
) {
    let source_path = {
        let p = Path::new(source);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            // Resolve relative to cwd, not base_dir — base_dir was derived
            // from source's parent so joining them would double the path.
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(p)
        }
    };

    log::debug!("Source path: {}", source_path.display());

    if !source_path.exists() {
        error!("Source file not found: {}", source_path.display());
        std::process::exit(1);
    }
    if !source_path.is_file() {
        error!("Source path is not a file: {}", source_path.display());
        std::process::exit(1);
    }

    let problem = match source_path.file_stem().and_then(|s| s.to_str()) {
        Some(name) => sanitize(name),
        None => {
            error!("Invalid C++ source filename: {}", source_path.display());
            std::process::exit(1);
        }
    };

    let test_source: TestSource = if let Some(in_path) = input_file {
        let input_path = expand_path(in_path);
        if !input_path.exists() {
            error!("Cannot read --in file: file not found");
            std::process::exit(1);
        }
        let expected_path = expected_file.map(|p| {
            let ep = expand_path(p);
            if !ep.exists() {
                error!("Cannot read --exp file: file not found");
                std::process::exit(1);
            }
            ep
        });
        TestSource::DirectFile {
            input_path,
            expected_path,
        }
    } else if interactive {
        TestSource::FilePairs(vec![])
    } else {
        let prefix = format!("{}_input", problem);
        let single_input = format!("{}_input.txt", problem);
        let tc_dir = base_dir.join(TESTCASES_DIR);
        log::debug!("Looking for testcases in {}", tc_dir.display());

        let mut pairs: Vec<(usize, PathBuf, PathBuf)> = Vec::new();

        let scan_dir = |dir: &PathBuf, pairs: &mut Vec<(usize, PathBuf, PathBuf)>| {
            let Ok(rd) = fs::read_dir(dir) else { return };
            for entry in rd.flatten() {
                let path = entry.path();
                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_owned(),
                    None => continue,
                };

                if name == single_input {
                    let out = dir.join(format!("{}_output.txt", problem));
                    log::debug!("Found single testcase: {}", name);
                    if out.exists() {
                        pairs.push((0, path, out));
                    }
                    continue;
                }

                if name.starts_with(&prefix) && name.ends_with(".txt") {
                    let idx_part = &name[prefix.len()..name.len() - 4];
                    if idx_part.is_empty() {
                        continue;
                    }
                    if let Ok(idx) = idx_part.parse::<usize>() {
                        let out = dir.join(format!("{}_output{}.txt", problem, idx));
                        log::debug!("Found testcase #{}: {}", idx, name);
                        if out.exists() {
                            pairs.push((idx, path, out));
                        }
                    }
                }
            }
        };

        // primary: testcases/ directory
        if tc_dir.exists() {
            scan_dir(&tc_dir, &mut pairs);
        }

        // fallback: scan the base directory
        if pairs.is_empty() {
            log::debug!(
                "No testcases in {}, falling back to {}",
                tc_dir.display(),
                base_dir.display()
            );
            scan_dir(&base_dir.clone(), &mut pairs);
        }

        if pairs.is_empty() {
            match find_prob_for_source(&source_path, &base_dir) {
                Some(prob_path) => {
                    log::debug!("Auto-detected CPH prob: {}", prob_path.display());
                    let tests = load_tests_from_prob(&prob_path);
                    TestSource::CphProb { tests }
                }
                None => {
                    error!(
                        "No testcases found for \\'{}\\' — tried {}/{}_input*.txt, {}_input*.txt, and {}/*.prob",
                        problem, TESTCASES_DIR, problem, problem, CPH_DIR
                    );
                    std::process::exit(1);
                }
            }
        } else {
            pairs.sort_by_key(|(idx, _, _)| *idx);
            TestSource::FilePairs(pairs)
        }
    };

    // Ensure temp/ dir exists (cache file also lives here)
    if let Err(e) = fs::create_dir_all(base_dir.join("temp")) {
        error!("Failed to create temp directory: {}", e);
        std::process::exit(1);
    }

    let exe = base_dir.join("temp/main");
    let cache_file = cache_file_path(&base_dir);

    if !compile_if_needed(&source_path, &exe, source, &cache_file, no_compile) {
        std::process::exit(1);
    }
    println!();

    if interactive {
        run_interactive_loop(&exe);
        return;
    }

    let total = test_source.len();
    info!("Running {} testcases", total);

    let mut passed = 0usize;

    match test_source {
        TestSource::DirectFile {
            input_path,
            expected_path,
        } => {
            let expected = match &expected_path {
                Some(p) => fs::read_to_string(p).unwrap_or_default(),
                None => String::new(),
            };
            // Read input for display; open a second handle to redirect as stdin.
            let input_display = fs::read_to_string(&input_path).unwrap_or_default();
            let stdin_file = std::fs::File::open(&input_path).unwrap_or_else(|e| {
                error!("Cannot open --in file: {}", e);
                std::process::exit(1);
            });

            let t0 = Instant::now();
            let (actual, stderr) =
                run_one_test(&exe, Some(stdin_file), None).unwrap_or_else(|| {
                    error!("Failed to run executable");
                    std::process::exit(1);
                });
            let time_str = format_time(t0.elapsed());

            if print_test_result(
                "TEST #1",
                input_display.trim(),
                expected.trim(),
                &actual,
                &stderr,
                &time_str,
                verbose,
            ) {
                passed += 1;
            }
        }

        TestSource::CphProb { tests } => {
            for (idx, input, expected) in tests {
                let t0 = Instant::now();
                let (actual, stderr) = run_one_test(&exe, None, Some(input.as_bytes().to_vec()))
                    .unwrap_or_else(|| {
                        error!("Failed to run executable");
                        std::process::exit(1);
                    });
                let time_str = format_time(t0.elapsed());

                if print_test_result(
                    &format!("TEST #{}", idx),
                    input.trim(),
                    expected.trim(),
                    &actual,
                    &stderr,
                    &time_str,
                    verbose,
                ) {
                    passed += 1;
                }
            }
        }

        TestSource::FilePairs(pairs) => {
            for (idx, input_path, output_path) in pairs {
                let input_display = fs::read_to_string(&input_path).unwrap_or_default();
                let expected = fs::read_to_string(&output_path).unwrap_or_default();
                let stdin_file = std::fs::File::open(&input_path).unwrap_or_else(|e| {
                    error!("Cannot open input file: {}", e);
                    std::process::exit(1);
                });

                let t0 = Instant::now();
                let (actual, stderr) =
                    run_one_test(&exe, Some(stdin_file), None).unwrap_or_else(|| {
                        error!("Failed to run executable");
                        std::process::exit(1);
                    });
                let time_str = format_time(t0.elapsed());

                if print_test_result(
                    &format!("TEST #{}", idx),
                    input_display.trim(),
                    expected.trim(),
                    &actual,
                    &stderr,
                    &time_str,
                    verbose,
                ) {
                    passed += 1;
                }
            }
        }
    }

    println!();
    if passed != total {
        error!("Some tests failed: {}/{} passed", passed, total);
        std::process::exit(1);
    }
    info!("All tests passed");
}

fn load_tests_from_prob(prob_path: &Path) -> Vec<(usize, String, String)> {
    let raw = fs::read_to_string(prob_path).unwrap_or_else(|e| {
        error!("Failed to read .prob file {}: {}", prob_path.display(), e);
        std::process::exit(1);
    });
    let prob: CphProb = serde_json::from_str(&raw).unwrap_or_else(|e| {
        error!("Failed to parse .prob file {}: {}", prob_path.display(), e);
        std::process::exit(1);
    });
    info!(
        "Loaded {} testcases from CPH: {}",
        prob.tests.len(),
        prob.name.as_deref().unwrap_or("?")
    );
    prob.tests
        .into_iter()
        .enumerate()
        .map(|(i, t)| (i + 1, t.input, t.output))
        .collect()
}

/// Find the .prob file for a given source path inside base_dir/.cph/
/// CPH names them like: .A_Ambitious_Kid.cpp_<hash>.prob
fn find_prob_for_source(source_path: &Path, base_dir: &Path) -> Option<PathBuf> {
    let filename = source_path.file_name()?.to_str()?; // e.g. "A_Ambitious_Kid.cpp"
    let cph_dir = base_dir.join(CPH_DIR);
    if !cph_dir.exists() {
        return None;
    }
    // CPH prefixes with a dot and appends _<hash>.prob
    let prefix = format!(".{}_", filename);
    fs::read_dir(&cph_dir).ok()?.flatten().find_map(|e| {
        let name = e.file_name();
        let name = name.to_str()?;
        if name.starts_with(&prefix) && name.ends_with(".prob") {
            Some(e.path())
        } else {
            None
        }
    })
}

fn stress_mode(
    solution_src: &str,
    brute_src: &str,
    gen_src: &str,
    base_dir: PathBuf,
    count: usize,
    stop_on_fail: bool,
    start_seed: usize,
    verbose: bool,
    no_compile: bool,
) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let resolve = |src: &str| -> PathBuf {
        let p = Path::new(src);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            cwd.join(p)
        }
    };

    let solution_path = resolve(solution_src);
    let brute_path = resolve(brute_src);
    let gen_path = resolve(gen_src);

    for p in [&solution_path, &brute_path, &gen_path] {
        if !p.exists() {
            error!("File not found: {}", p.display());
            std::process::exit(1);
        }
    }

    let tmp = base_dir.join("temp");
    fs::create_dir_all(&tmp).expect("Failed to create temp dir");

    let exe_sol = tmp.join("stress_sol");
    let exe_brute = tmp.join("stress_brute");
    let exe_gen = tmp.join("stress_gen");

    let cache_file = cache_file_path(&tmp);
    if !compile_if_needed(
        &solution_path,
        &exe_sol,
        "solution",
        &cache_file,
        no_compile,
    ) || !compile_if_needed(&brute_path, &exe_brute, "brute", &cache_file, no_compile)
        || !compile_if_needed(&gen_path, &exe_gen, "generator", &cache_file, no_compile)
    {
        std::process::exit(1);
    }

    println!();

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    let infinite = count == 0;
    let mut passed = 0usize;
    let mut seed = start_seed;

    while running.load(Ordering::SeqCst) && (infinite || seed < start_seed + count) {
        // generate input
        let gen_out = Command::new(&exe_gen)
            .arg(seed.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("Failed to run generator");

        if !gen_out.status.success() {
            error!(
                "Generator crashed on seed {}: {}",
                seed,
                String::from_utf8_lossy(&gen_out.stderr)
            );
            std::process::exit(1);
        }

        let input = String::from_utf8_lossy(&gen_out.stdout).to_string();

        // run both
        let t0 = Instant::now();
        let (sol_out, sol_stderr) = run_exe(&exe_sol, &input).unwrap_or_else(|| {
            error!("Solution crashed on seed {}", seed);
            std::process::exit(1);
        });
        let time_str = format_time(t0.elapsed());

        let (brute_out, _) = run_exe(&exe_brute, &input).unwrap_or_else(|| {
            error!("Brute crashed on seed {}", seed);
            std::process::exit(1);
        });

        let ok = print_test_result(
            &format!("STRESS seed={}", seed),
            input.trim(),
            &brute_out, // expected
            &sol_out,   // actual
            &sol_stderr,
            &time_str,
            verbose,
        );

        if ok {
            passed += 1;
        } else if stop_on_fail {
            // store in parent of sol.cpp
            let tc_dir = solution_path
                .parent()
                .unwrap_or(&base_dir)
                .join(TESTCASES_DIR);
            fs::create_dir_all(&tc_dir).ok();
            let fail_input = tc_dir.join("stress_fail_input.txt");
            let fail_expected = tc_dir.join("stress_fail_output.txt");
            fs::write(&fail_input, &input).ok();
            fs::write(&fail_expected, &brute_out).ok();
            info!("Failing input saved to {}", fail_input.display());
            info!("Correct output saved to {}", fail_expected.display());
            std::process::exit(1);
        }

        seed += 1;
    }

    println!();
    if running.load(Ordering::SeqCst) {
        info!(
            "Stress test complete: {} / {} passed",
            passed,
            seed - start_seed
        );
    } else {
        info!("Interrupted after {} test(s) passed", passed);
    }
}

use clap::{Parser, Subcommand};
use colored::Colorize;
use log::{Level, error, info, warn};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
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
#[command(name = "case-compiler")]
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
        } => {
            let base = resolve_base(&source_dir, Some(&source));
            run_mode(
                &source,
                base,
                verbose,
                interactive,
                input_file.as_deref(),
                expected_file.as_deref(),
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

fn diff_lines(expected: &str, actual: &str) {
    let normalize = |s: &str| s.lines().map(|l| l.trim()).collect::<Vec<_>>().join("\n");

    let expected = normalize(expected);
    let actual = normalize(actual);

    let diff = TextDiff::from_lines(&expected, &actual);

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => {
                eprint!("{}", "- ".red().bold());
                eprint!("{}", change.to_string().red());
            }
            ChangeTag::Insert => {
                eprint!("{}", "+ ".green().bold());
                eprint!("{}", change.to_string().green());
            }
            ChangeTag::Equal => {
                eprint!("  {}", change);
            }
        }
    }
}

fn normalize(s: &str) -> String {
    s.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
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
        eprintln!("Input:");
        eprintln!("--------------------");
        eprintln!("{}", input);
        eprintln!("--------------------");
        eprintln!("Expected Output:");
        eprintln!("--------------------");
        eprintln!("{}", expected);
        eprintln!("--------------------");
        eprintln!("Your Output:");
        eprintln!("--------------------");
        eprintln!("{}", actual);
        eprintln!("--------------------");
        if !passed {
            eprintln!("Diff:");
            eprintln!("--------------------");
            diff_lines(expected, actual);
            eprintln!("--------------------");
        }
        if !stderr.is_empty() {
            eprintln!("Debug Output (stderr):");
            eprintln!("--------------------");
            eprintln!("{}", stderr);
            eprintln!("--------------------");
        }
        eprintln!();
    }

    passed
}

fn format_time(d: std::time::Duration) -> String {
    format!("{} s", d.as_secs_f32())
}

fn listen_mode(source_dir: PathBuf) {
    let listener = TcpListener::bind("127.0.0.1:10045").expect("Failed to bind port 27121");
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

fn run_mode(
    source: &str,
    base_dir: std::path::PathBuf,
    verbose: bool,
    interactive: bool,
    input_file: Option<&str>,
    expected_file: Option<&str>,
) {
    let source_path = {
        let p = Path::new(source);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            base_dir.join(p)
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

    let mut tests = Vec::new();
    let mut use_direct_cases = false;
    let mut cph_prob_tests: Vec<(usize, String, String)> = Vec::new();

    if let Some(in_path) = input_file {
        let input = fs::read_to_string(expand_path(in_path)).unwrap_or_else(|e| {
            error!("Cannot read --in file: {}", e);
            std::process::exit(1);
        });
        let expected = match expected_file {
            Some(exp_path) => fs::read_to_string(expand_path(exp_path)).unwrap_or_else(|e| {
                error!("Cannot read --exp file: {}", e);
                std::process::exit(1);
            }),
            None => String::new(),
        };
        use_direct_cases = true;
        cph_prob_tests.push((1, input, expected));
    } else if !interactive {
        let prefix = format!("{}_input", problem);
        let single_input = format!("{}_input.txt", problem);
        let tc_dir = base_dir.join(TESTCASES_DIR);
        log::debug!("Looking for testcases in {}", tc_dir.display());
        if tc_dir.exists() {
            for entry in fs::read_dir(&tc_dir).expect("No testcases directory") {
                let path = entry.unwrap().path();
                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n,
                    None => continue,
                };

                if name == single_input {
                    let out = tc_dir.join(format!("{}_output.txt", problem));
                    log::debug!("Found single testcase: {}", name);
                    if out.exists() {
                        tests.push((0, path, out));
                    }
                    continue;
                }

                if name.starts_with(&prefix) && name.ends_with(".txt") {
                    let idx_part = &name[prefix.len()..name.len() - 4];
                    if idx_part.is_empty() {
                        continue;
                    }
                    if let Ok(idx) = idx_part.parse::<usize>() {
                        let out = tc_dir.join(format!("{}_output{}.txt", problem, idx));
                        log::debug!("Found testcase #{}: {}", idx, name);
                        if out.exists() {
                            tests.push((idx, path, out));
                        }
                    }
                }
            }
        }

        if tests.is_empty() {
            match find_prob_for_source(&source_path, &base_dir) {
                Some(prob_path) => {
                    log::debug!("Auto-detected CPH prob: {}", prob_path.display());
                    cph_prob_tests = load_tests_from_prob(&prob_path);
                    use_direct_cases = true;
                }
                None => {
                    error!(
                        "No testcases found for \'{}\' — tried {}/{}_input*.txt and {}/*.prob",
                        problem, TESTCASES_DIR, problem, CPH_DIR
                    );
                    std::process::exit(1);
                }
            }
        }

        tests.sort_by_key(|(idx, _, _)| *idx);

        if tests.is_empty() && cph_prob_tests.is_empty() {
            error!("No testcases found for problem '{}'", source_path.display());
            std::process::exit(1);
        }
    }

    info!("Compiling {}", source);
    let start = Instant::now();
    let exe = base_dir.join("temp/main");

    let c = Command::new("g++")
        .args(["-std=gnu++17", "-O2", "-pipe", "-Wall", "-Wextra"])
        .arg(source_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();

    if !c.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&c.stderr));
        std::process::exit(1);
    }

    info!("Compiled ({:.2}s)", start.elapsed().as_secs_f64());
    println!("");

    if interactive {
        run_interactive_loop(&exe);
        return;
    }

    let mut passed = 0;
    let total = if use_direct_cases {
        cph_prob_tests.len()
    } else {
        tests.len()
    };

    info!("Running {} testcases", total);
    if use_direct_cases {
        info!(
            "Running {} testcases from CPH problem",
            cph_prob_tests.len()
        );
        for (idx, input, expected) in cph_prob_tests {
            let t0 = Instant::now();
            let mut child = Command::new(&exe)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();

            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();

            let out = child.wait_with_output().unwrap();
            let time_str = format_time(t0.elapsed());
            let actual = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let expected = expected.trim().to_string();
            let input = input.trim().to_string();

            if print_test_result(
                &format!("TEST #{}", idx),
                &input,
                &expected,
                &actual,
                &stderr,
                &time_str,
                verbose,
            ) {
                passed += 1;
            }
        }
    } else {
        for (idx, input_path, output_path) in tests {
            let input = fs::read_to_string(&input_path).unwrap();
            let expected = fs::read_to_string(&output_path).unwrap();

            let t0 = Instant::now();
            let mut child = Command::new(&exe)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();

            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();

            let out = child.wait_with_output().unwrap();
            let time_str = format_time(t0.elapsed());
            let actual = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let expected = expected.trim().to_string();
            let input = input.trim().to_string();

            if print_test_result(
                &format!("TEST #{}", idx),
                &input,
                &expected,
                &actual,
                &stderr,
                &time_str,
                verbose,
            ) {
                passed += 1;
            }
        }
    }

    println!("");
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
) {
    let resolve = |src: &str| -> PathBuf {
        let p = Path::new(src);
        if p.is_absolute() || p.exists() {
            p.to_path_buf()
        } else {
            base_dir.join(p)
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

    if !compile_cpp(&solution_path, &exe_sol, "solution")
        || !compile_cpp(&brute_path, &exe_brute, "brute")
        || !compile_cpp(&gen_path, &exe_gen, "generator")
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

# vrun

A CLI tool for competitive programmers. It:

- Receives test cases directly from your browser via Competitive Companion
- Compiles your C++ solution and runs it against those test cases
- Reports each test as AC or WA, with a diff of expected vs actual output on failure
- Stress tests your solution against a brute-checker using a generator to find failing cases
- Supports interactive mode for manual debugging


## Installation

### Prerequisites

1.  **Rust Toolchain:** Install Rust via [rustup](https://rustup.rs/):
    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```
2.  **C++ Compiler:** Ensure `g++` is installed and available in your `PATH`.

### Build and Install

```bash
# Build the project
cargo build --release

# Install globally to your ~/.cargo/bin
cargo install --path .
```

## Usage Examples

### 1. Ingesting Testcases
Start the listener to receive testcases directly from your browser:
```bash
vrun listen
```
*Note: Make sure Competitive Companion is configured to send requests to port 27121 (the default for this tool).*

### 2. Running a Solution
Test your solution against testcases from various sources:
- **`testcases/` folder:** Automatically looks for `{source}_input*.txt` and `{source}_output*.txt` files (compatible with **Competitest** Neovim plugin).
- **CPH VS Code Extension:** Automatically detects `.cph/*.prob` files for the given source.
- **Custom Input:** Run against a specific file:
  ```bash
  vrun run A.cpp --in input.txt --exp output.txt
  ```

Basic usage:
```bash
vrun run A_Eating_Game.cpp
```
If your testcases are elsewhere, you can specify it: `vrun run solution.cpp --source-dir ~/custom_path`.

### 3. Stress Testing
To find failing cases for your solution by comparing it to a brute-force approach using a generator:
```bash
vrun stress samples/brute.cpp samples/gen.cpp samples/sol.cpp --count 50
```
This will automatically use the parent directory of your solution as the base for storing temporary files and looking for testcases.

### 4. Interactive Mode
If you need to manually debug or provide input:
```bash
vrun run A_Eating_Game.cpp --interactive
```

## CLI Reference

```text
Usage: vrun <COMMAND>

Commands:
  listen  Listen for Competitive Companion testcases
  run     Compile and run C++ code using testcases
  stress  Stress test: run solution vs brute force using a generator
  help    Print this message or the help of the given subcommand(s)

Options for 'listen':
  --source-dir <DIR>  Base directory (testcases/ will be created inside)
  -v, --verbose       Verbose output

Options for 'run':
  --source-dir <DIR>  Base directory where testcases/ exists (defaults to source file's parent)
  -i, --interactive   Interactive mode
  -v, --verbose       Verbose output
  --in <INPUT_FILE>   Custom input file (skips testcase discovery)
  --exp <EXPECTED_FILE> Custom expected output file (used with --in)

Options for 'stress':
  --source-dir <DIR>  Base directory (defaults to solution file's parent)
  -c, --count <NUM>   Number of stress test iterations (0 = infinite) [default: 0]
  --stop-on-fail      Stop on first failure [default: true]
  --seed <NUM>        Starting seed value [default: 1]
  -v, --verbose       Verbose output
```


## License
Apache License 2.0

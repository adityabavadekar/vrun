# vrun

[![crates.io](https://img.shields.io/crates/v/vrun.svg)](https://crates.io/crates/vrun)

A CLI tool for competitive programmers that:

- Receives test cases directly from your browser via Competitive Companion
- Compiles your C++ solution and runs it against those test cases
- Reports each test as AC or WA, with a diff of expected vs actual output on failure
- Stress tests your solution against a brute-checker using a generator to find failing cases
- Supports interactive mode for manual debugging

<img src="assets/demo.gif" alt="vrun demo" width="700"/>

## Installation

### Prerequisites

1.  **Rust Toolchain:** Install Rust via [rustup](https://rustup.rs/):
    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```
2.  **C++ Compiler:** Ensure `g++` is installed and available in your `PATH`.

### Installation

```
cargo install vrun
```

<details>
<summary>Build from source</summary>

```bash
git clone https://github.com/adityabavadekar/vrun
cd vrun
cargo install --path . # Install globally to your ~/.cargo/bin
```

</details>

## Usage

### 1. Ingesting Testcases
Start the listener to receive testcases directly from your browser:
```bash
vrun listen
```
*Note: Make sure Competitive Companion is configured to send requests to port 10045 (the default for this tool).*

### 2. Running a Solution
Test your solution against testcases from various sources (discovery order):

1. **`testcases/` subfolder** - looks for `{Problem}_input*.txt` / `{Problem}_output*.txt` inside a `testcases/` directory (compatible with the **Competitest** Neovim plugin).
2. **Alongside the source file** - if `testcases/` yields nothing, looks for the same file pattern in the same directory as the source.
3. **CPH VS Code Extension** - auto-detects `.cph/*.prob` files.
4. **Custom input file:**
   ```bash
   vrun run A.cpp --in input.txt --exp output.txt
   ```

Basic usage:
```bash
vrun run A_Eating_Game.cpp
```
If your testcases are in a non-standard location, specify it explicitly: `vrun run solution.cpp --source-dir ~/custom_path`.

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

## Quick Examples

```bash
# run solution
vrun run solution.cpp

# verbose run
vrun run solution.cpp -v

# custom input
vrun run solution.cpp --in input.txt --exp output.txt

# interactive (waits for input)
vrun run solution.cpp --interactive

# stress test
vrun stress brute.cpp gen.cpp solution.cpp --count 50

# listen for Competitive Companion
vrun listen
```

## License
Apache License 2.0

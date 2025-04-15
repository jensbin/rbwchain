# rbwchain ✨

`rbwchain` is a command-line utility designed to securely execute commands by injecting secrets fetched from [`rbw`](https://github.com/doy/rbw), a CLI for Bitwarden. It provides two methods for passing secrets to the child process:

1.  **Environment Variables (Default):** Parses the `rbw` note content for `KEY=VALUE` pairs and sets them as environment variables.
2.  **Temporary File:** Writes the raw `rbw` note content to a temporary file and provides the path to this file via a specified environment variable.

This allows seamless integration of secrets stored in Bitwarden into various command-line workflows without exposing them directly in scripts or shell history.

## Motivation 🤔

Managing secrets (API keys, passwords, certificates) for command-line tools and scripts can be challenging. Hardcoding secrets is insecure, and managing environment variables manually can be cumbersome. `rbwchain` leverages the security of Bitwarden via `rbw` to provide secrets *just-in-time* to the processes that need them.

## Features 🚀

*   **Secure Secret Injection:** Fetches secrets directly from `rbw`.
*   **Environment Variable Mode:** Parses `KEY=VALUE` lines from the secret note (supports `#` comments, skips invalid lines).
*   **Temporary File Mode:** Provides raw secret content (e.g., private keys, config files) via a temporary file path set in an environment variable.
*   **Automatic Cleanup:** Temporary files are automatically deleted when the child process exits.
*   **Correct Exit Status:** Propagates the exit code or termination signal from the child process.
*   **Dependency Check:** Verifies `rbw` is available in the system's PATH before execution.
*   **Informative Output:** Logs actions to stderr (`[rbwchain] ...`).

## Prerequisites 🛠️

*   **Rust Toolchain:** Required for building the project (e.g., `rustup`, `cargo`). See [rust-lang.org](https://www.rust-lang.org/tools/install).
*   **`rbw`:** The `rbw` command-line tool must be installed, configured, and logged into your Bitwarden account. See the [rbw documentation](https://github.com/doy/rbw) for installation instructions.

## Installation 📦

### From Source

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/your-username/rbwchain.git # Replace with actual repo URL
    cd rbwchain
    ```
2.  **Build the release binary:**
    ```bash
    cargo build --release
    ```
3.  **Copy the binary to a location in your `$PATH`:**
    ```bash
    cp target/release/rbwchain ~/.local/bin/ # Or any other preferred directory in your PATH
    ```

## Usage ⌨️

The basic syntax is:

```
rbwchain <SECRET_NOTE> [-f ENV_VAR_NAME | --file ENV_VAR_NAME] [-d] <COMMAND> [ARGS...]
```

*   `<SECRET_NOTE>`: The name of the note in your Bitwarden vault (as accessed by `rbw get <SECRET_NOTE>`).
*   `-f ENV_VAR_NAME` or `--file ENV_VAR_NAME`: (Optional) Use temporary file mode. The path to the temporary file containing the secret content will be stored in the environment variable named `ENV_VAR_NAME`.
*   `-d` enable debug mode`.
*   `<COMMAND>`: The command to execute.
*   `[ARGS...]`: Arguments to pass to the command.

---

### Mode 1: Environment Variables (Default)

If the `-f`/`--file` option is **not** provided, `rbwchain` fetches the content of `<SECRET_NOTE>`, parses lines matching `KEY=VALUE`, and injects them as environment variables for the `<COMMAND>`.

**Example `rbw` Note (`my-app-secrets`):**

```
# API Credentials for My App
API_KEY=abcdef123456
API_SECRET=very_secret_value_789
# DB_URL=... (Commented out)

DEBUG_MODE=false
```

**Command:**

```bash
# Run 'my-app --config prod.json' with secrets from 'my-app-secrets' note
rbwchain my-app-secrets ./my-app --config prod.json

# Verify the environment variables are set (using 'env')
rbwchain my-app-secrets env | grep API_
# Output might show:
# API_KEY=abcdef123456
# API_SECRET=very_secret_value_789
```

`rbwchain` will execute `./my-app --config prod.json` with `API_KEY`, `API_SECRET`, and `DEBUG_MODE` set in its environment, alongside standard `rbwchain` helper variables (`RBWCHAIN_VERSION`, `RBWCHAIN_SECRET_NOTE`).

---

### Mode 2: Temporary File (`-f` / `--file`)

If the `-f`/`--file` option **is** provided, `rbwchain` fetches the *raw* content of `<SECRET_NOTE>`, writes it to a secure temporary file, sets the environment variable `ENV_VAR_NAME` to the *path* of this file, and then executes `<COMMAND>`. The temporary file is deleted automatically after the command finishes.

This is useful for secrets that are files themselves (like SSH keys, TLS certificates, JSON service account keys, or configuration files).

**Example `rbw` Note (`exoscale config`):** (Contains raw exoscale config in TOML)

```toml
defaultaccount = "my-account"

[[accounts]]
  account = "myaccount"
  defaultZone = "ch-dk-2"
  environment = ""
  key = "EXO111111111111111"
  name = "my-account"
  secret = "dsdddddddddddddddddddddddddd"
```

**Command:**

```bash
# Run exo using the config from the 'my-exosclaleconfig' note (exo is honoring EXOSCALE_CONFIG)
rbwchain -f EXOSCALE_CONFIG my-exoscaleconfig exo compute instance list
```

In the first example, `kubectl` will be executed with the `KUBECONFIG_TMP` environment variable set to `/tmp/some_random_name.tmp` (the actual path will vary), and that file will contain the content of the `my-kubeconfig` note.

---

## Environment Variables Set by `rbwchain` 📦

`rbwchain` always sets the following environment variables for the child process:

*   `RBWCHAIN_VERSION`: The version of the `rbwchain` utility being used.
*   `RBWCHAIN_SECRET_NOTE`: The name of the secret note requested from `rbw`.

Additionally:

*   In **Environment Variable Mode**, it sets variables parsed from the secret note.
*   In **Temporary File Mode**, it sets the user-specified environment variable (e.g., `KUBECONFIG_TMP` in the example) to the path of the temporary file.

## Error Handling and Exit Codes ⚠️

*   `rbwchain` will exit with a non-zero status code if:
    *   The `rbw` command is not found in the `PATH`.
    *   `rbw get <SECRET_NOTE>` fails (e.g., note not found, `rbw` error).
    *   It fails to create or write to the temporary file (in file mode).
    *   The specified environment variable name for file mode (`-f NAME`) is empty.
*   If the child command executes successfully or fails, `rbwchain` will exit with the **same exit code** as the child command.
*   If the child command is terminated by a signal (on Unix-like systems), `rbwchain` will exit with `128 + signal_number`, mimicking standard shell behavior.

## Inspiration

*   `envchain` - https://github.com/sorah/envchain

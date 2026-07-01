#!/usr/bin/env python3
import os
import sys
import subprocess
import shutil
import time

RESET = "\033[0m"
BOLD = "\033[1m"
RED = "\033[31m"
GREEN = "\033[32m"
YELLOW = "\033[33m"
CYAN = "\033[36m"
WHITE = "\033[37m"

def print_header(title):
    width = 60
    print(f"\n{CYAN}{BOLD}┌" + "─" * (width - 2) + f"┐{RESET}")
    print(f"{CYAN}{BOLD}│{WHITE}{BOLD}{title.center(width - 2)}{CYAN}{BOLD}│{RESET}")
    print(f"{CYAN}{BOLD}└" + "─" * (width - 2) + f"┘{RESET}")

def run_cmd(args, desc):
    print(f"\n{YELLOW}➔ Running: {desc}...{RESET}")
    try:
        subprocess.run(args, check=True)
        print(f"{GREEN}✔ Done!{RESET}")
        return True
    except subprocess.CalledProcessError as e:
        print(f"{RED}✘ Failed to run command: {e}{RESET}")
        return False

def minify():
    print_header("⚡ HTML MINIFICATION ENGINE ⚡")
    if not os.path.exists("index-dev.html"):
        print(f"{RED}Error: index-dev.html not found!{RESET}")
        return False
    
    if not shutil.which("npx"):
        print(f"{RED}Error: 'npx' is required for minification. Please install Node.js/npm.{RESET}")
        return False

    cmd = [
        "npx", "-y", "html-minifier-terser",
        "index-dev.html",
        "--collapse-whitespace",
        "--remove-comments",
        "--minify-css", "true",
        "--minify-js", "true",
        "--collapse-boolean-attributes",
        "--remove-attribute-quotes",
        "--remove-redundant-attributes",
        "--remove-script-type-attributes",
        "--remove-style-link-type-attributes",
        "--use-short-doctype",
        "-o", "index.html"
    ]
    return run_cmd(cmd, "html-minifier-terser")

def build_local():
    print_header("⚙️ BUILDING LOCAL SYSTEM BINARY ⚙️")
    return run_cmd(["cargo", "build", "--release"], "cargo build --release")

def build_cross():
    print_header("📱 CROSS-COMPILING FOR ANDROID/TERMUX (ARM64) 📱")
    
    # Check container engine
    engine = "podman" if shutil.which("podman") else "docker" if shutil.which("docker") else None
    if not engine:
        print(f"{RED}Error: 'podman' or 'docker' is required to run the 'cross' compiler wrapper.{RESET}")
        return False

    if not shutil.which("cross"):
        print(f"{YELLOW}Warning: 'cross' command not found in PATH.{RESET}")
        print(f"Installing 'cross' tool first...")
        if not run_cmd(["cargo", "install", "cross"], "cargo install cross"):
            return False

    env = os.environ.copy()
    env["CROSS_CONTAINER_ENGINE"] = engine
    
    cmd = ["cross", "build", "--release", "--target", "aarch64-unknown-linux-musl"]
    print(f"{YELLOW}➔ Running cross-build using target: aarch64-unknown-linux-musl (Linker engine: {engine})...{RESET}")
    try:
        subprocess.run(cmd, check=True, env=env)
        print(f"{GREEN}✔ Cross-compilation completed successfully!{RESET}")
        binary_path = "target/aarch64-unknown-linux-musl/release/retro-homepage"
        if os.path.exists(binary_path):
            size = os.path.getsize(binary_path) / (1024 * 1024)
            print(f"{GREEN}Output binary size: {size:.2f} MB{RESET}")
            print(f"{GREEN}Binary location: {binary_path}{RESET}")
        return True
    except subprocess.CalledProcessError as e:
        print(f"{RED}✘ Cross-compilation failed: {e}{RESET}")
        return False

def run_local(extra_args=[]):
    print_header("🚀 RUNNING LOCAL SERVER 🚀")
    binary_path = "./target/release/retro-homepage"
    if not os.path.exists(binary_path):
        print(f"{YELLOW}Warning: Binary not found at {binary_path}. Building it first...{RESET}")
        if not build_local():
            return False
    cmd = [binary_path] + extra_args
    return run_cmd(cmd, " ".join(cmd))

def print_help():
    print(f"\n{BOLD}Retro Homepage build manager script{RESET}")
    print("Usage: python3 script.py <command>")
    print("\nCommands:")
    print("  minify   - Compress and minify index-dev.html to index.html")
    print("  build    - Compile the Rust release binary for the local PC")
    print("  cross    - Cross-compile for Android/Termux (ARM64)")
    print("  run      - Run the local backend server (served at localhost:3000)")
    print("  all      - Execute minify, build, and cross compilation")
    print("  help     - Show this help summary")

def main():
    if len(sys.argv) < 2:
        print_help()
        sys.exit(1)
        
    cmd = sys.argv[1].strip().lower()
    
    if cmd == "minify":
        success = minify()
    elif cmd == "build":
        success = minify() and build_local()
    elif cmd == "cross":
        success = minify() and build_cross()
    elif cmd == "run":
        success = run_local(sys.argv[2:])
    elif cmd == "all":
        success = minify() and build_local() and build_cross()
    elif cmd in ("help", "-h", "--help"):
        print_help()
        sys.exit(0)
    else:
        print(f"{RED}Unknown command: {cmd}{RESET}")
        print_help()
        sys.exit(1)
        
    if not success:
        sys.exit(1)

if __name__ == "__main__":
    main()

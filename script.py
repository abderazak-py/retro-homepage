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

def get_version():
    try:
        if os.path.exists("Cargo.toml"):
            with open("Cargo.toml", "r") as f:
                for line in f:
                    if line.strip().startswith("version"):
                        return line.split("=")[1].strip().replace('"', '')
    except Exception as e:
        print(f"{RED}Error reading version from Cargo.toml: {e}{RESET}")
    return "1.0.0"

def package():
    print_header("📦 PACKAGING RELEASE BINARIES 📦")
    version = get_version()
    dist_dir = "dist"
    
    if os.path.exists(dist_dir):
        shutil.rmtree(dist_dir)
    os.makedirs(dist_dir)
    
    binaries = {
        "x86_64": "target/release/retro-homepage",
        "aarch64": "target/aarch64-unknown-linux-musl/release/retro-homepage"
    }
    
    success = True
    packaged_count = 0
    for arch, path in binaries.items():
        if not os.path.exists(path):
            print(f"{YELLOW}Warning: Binary for {arch} not found at {path}. Skipping packaging for this target.{RESET}")
            continue
            
        archive_name = f"retro-homepage-v{version}-linux-{arch}"
        archive_path = os.path.join(dist_dir, archive_name)
        
        # Create a temp directory to hold the archive structure
        temp_dir = os.path.join(dist_dir, f"temp_{arch}")
        os.makedirs(temp_dir, exist_ok=True)
        
        try:
            # Copy binary
            shutil.copy2(path, os.path.join(temp_dir, "retro-homepage"))
            
            # Copy LICENSE and README.md if they exist
            for doc in ["LICENSE", "README.md"]:
                if os.path.exists(doc):
                    shutil.copy2(doc, temp_dir)
                    
            # Create tarball
            import tarfile
            with tarfile.open(f"{archive_path}.tar.gz", "w:gz") as tar:
                # Add contents of temp_dir but with archive_name as the root folder
                tar.add(temp_dir, arcname=archive_name)
                
            print(f"{GREEN}✔ Packaged {arch} binary successfully to {archive_path}.tar.gz{RESET}")
            packaged_count += 1
        except Exception as e:
            print(f"{RED}✘ Failed to package {arch}: {e}{RESET}")
            success = False
        finally:
            if os.path.exists(temp_dir):
                shutil.rmtree(temp_dir)
                
    if packaged_count == 0:
        print(f"{RED}✘ No binaries were found to package. Run 'python3 script.py all' or 'build'/'cross' first.{RESET}")
        return False
        
    return success

def print_help():
    print(f"\n{BOLD}Retro Homepage build manager script{RESET}")
    print("Usage: python3 script.py <command>")
    print("\nCommands:")
    print("  minify   - Compress and minify index-dev.html to index.html")
    print("  build    - Compile the Rust release binary for the local PC")
    print("  cross    - Cross-compile for Android/Termux (ARM64)")
    print("  run      - Run the local backend server (served at localhost:3000)")
    print("  all      - Execute minify, build, and cross compilation")
    print("  package  - Package release binaries into dist/ as .tar.gz archives")
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
    elif cmd == "package":
        success = package()
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

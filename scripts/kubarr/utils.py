"""Utility functions for Kubarr deployment."""

import subprocess
import sys
from typing import Optional


class Colors:
    """ANSI color codes for terminal output."""
    GREEN = "\033[92m"
    YELLOW = "\033[93m"
    RED = "\033[91m"
    BLUE = "\033[94m"
    CYAN = "\033[96m"
    BOLD = "\033[1m"
    DIM = "\033[2m"
    END = "\033[0m"


def log(msg: str, level: str = "info") -> None:
    """Print colored log message."""
    colors = {
        "info": Colors.BLUE,
        "success": Colors.GREEN,
        "warning": Colors.YELLOW,
        "error": Colors.RED,
        "step": Colors.CYAN,
    }
    prefix = {
        "info": "i",
        "success": "+",
        "warning": "!",
        "error": "x",
        "step": ">",
    }
    color = colors.get(level, Colors.BLUE)
    symbol = prefix.get(level, "*")
    print(f"{color}[{symbol}]{Colors.END} {msg}")


def log_header(msg: str) -> None:
    """Print a header message."""
    print()
    print(f"{Colors.BOLD}{Colors.CYAN}=== {msg} ==={Colors.END}")
    print()


def log_subheader(msg: str) -> None:
    """Print a subheader message."""
    print(f"{Colors.BOLD}{msg}{Colors.END}")


def run(
    cmd: str,
    check: bool = True,
    capture: bool = False,
    timeout: Optional[int] = None,
    shell: bool = True,
) -> subprocess.CompletedProcess:
    """Run a shell command.

    Args:
        cmd: Command to run
        check: Raise exception on non-zero exit
        capture: Capture stdout/stderr
        timeout: Timeout in seconds
        shell: Run through shell

    Returns:
        CompletedProcess instance
    """
    result = subprocess.run(
        cmd,
        shell=shell,
        check=check,
        capture_output=capture,
        text=True,
        timeout=timeout,
    )
    return result


def run_quiet(
    cmd: str,
    check: bool = True,
    timeout: Optional[int] = None,
) -> tuple[bool, str]:
    """Run command and return success status and output.

    Args:
        cmd: Command to run
        check: Whether to check return code
        timeout: Timeout in seconds

    Returns:
        Tuple of (success, output)
    """
    try:
        result = subprocess.run(
            cmd,
            shell=True,
            check=check,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return True, result.stdout
    except subprocess.CalledProcessError as e:
        return False, e.stderr or e.stdout or str(e)
    except subprocess.TimeoutExpired:
        return False, "Command timed out"


def confirm(msg: str, default: bool = False) -> bool:
    """Ask for user confirmation.

    Args:
        msg: Message to display
        default: Default value if user just presses Enter

    Returns:
        True if confirmed, False otherwise
    """
    suffix = "[Y/n]" if default else "[y/N]"
    try:
        response = input(f"{msg} {suffix} ").strip().lower()
        if not response:
            return default
        return response in ("y", "yes")
    except (KeyboardInterrupt, EOFError):
        print()
        return False


def die(msg: str, code: int = 1) -> None:
    """Print error message and exit."""
    log(msg, "error")
    sys.exit(code)


def check_tool(name: str, cmd: str) -> bool:
    """Check if a tool is available.

    Args:
        name: Tool name for display
        cmd: Command to check

    Returns:
        True if tool is available
    """
    success, _ = run_quiet(cmd, check=False)
    if success:
        log(f"{name} found", "success")
    else:
        log(f"{name} not found", "error")
    return success


def check_prerequisites() -> bool:
    """Check that required tools are installed.

    Returns:
        True if all prerequisites are met
    """
    log_subheader("Checking prerequisites...")

    tools = {
        "docker": "docker --version",
        "kubectl": "kubectl version --client",
        "helm": "helm version --short",
        "kind": "kind --version",
    }

    all_found = True
    for tool, cmd in tools.items():
        if not check_tool(tool, cmd):
            all_found = False

    return all_found

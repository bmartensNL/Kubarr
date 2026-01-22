"""Tests for CLI module."""

from click.testing import CliRunner

from kubarr.cli import main


def test_cli_help() -> None:
    """Test that CLI help works."""
    runner = CliRunner()
    result = runner.invoke(main, ["--help"])
    assert result.exit_code == 0
    assert "Kubarr" in result.output


def test_cli_version() -> None:
    """Test that version flag works."""
    runner = CliRunner()
    result = runner.invoke(main, ["--version"])
    assert result.exit_code == 0
    assert "version" in result.output.lower()

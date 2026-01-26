#!/usr/bin/env python3
"""Kubarr Deployment Tool - CLI for managing Kubarr deployments.

Usage:
    python kubarr.py setup           # Full setup with new cluster
    python kubarr.py reset           # Delete cluster
    python kubarr.py reset --full-rebuild  # Delete and rebuild everything
    python kubarr.py build [component]     # Build Docker images
    python kubarr.py deploy [chart]        # Deploy Helm charts
    python kubarr.py redeploy <component>  # Rebuild and redeploy component
    python kubarr.py status          # Show deployment status
    python kubarr.py logs <component>      # Stream component logs

Run 'python kubarr.py --help' for more information.
"""

import sys
from pathlib import Path

# Add scripts directory to path for imports
scripts_dir = Path(__file__).resolve().parent
sys.path.insert(0, str(scripts_dir))

from kubarr.cli import main

if __name__ == "__main__":
    sys.exit(main())

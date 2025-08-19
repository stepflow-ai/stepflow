#!/usr/bin/env python3
# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

# Copyright 2025 StepFlow Contributors
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""
Check if the current git user has signed the Individual Contributor License Agreement.

This script is used as a pre-commit hook to ensure all contributors have signed
the ICLA before their commits are accepted.
"""

import json
import subprocess
import sys
from pathlib import Path


class Colors:
    """Terminal color codes for better output visibility."""

    RED = "\033[91m"
    GREEN = "\033[92m"
    YELLOW = "\033[93m"
    BLUE = "\033[94m"
    BOLD = "\033[1m"
    RESET = "\033[0m"


def get_git_config(key):
    """
    Get a value from git config.

    Args:
        key: The git config key to retrieve

    Returns:
        The config value or None if not found
    """
    try:
        result = subprocess.run(
            ["git", "config", key], capture_output=True, text=True, check=False
        )
        if result.returncode == 0:
            return result.stdout.strip()
    except Exception:
        pass
    return None


def get_project_root():
    """
    Find the project root directory (where .git is located).

    Returns:
        Path to the project root directory
    """
    current_dir = Path.cwd()
    while current_dir != current_dir.parent:
        if (current_dir / ".git").exists():
            return current_dir
        current_dir = current_dir.parent

    # If we can't find .git, assume we're in the project root
    return Path.cwd()


def load_signatures():
    """
    Load ICLA signatures from the JSON file.

    Returns:
        List of signature dictionaries
    """
    project_root = get_project_root()
    signatures_file = project_root / ".github" / "cla-signatures.json"

    if not signatures_file.exists():
        print(
            f"{Colors.YELLOW}Warning: CLA signatures file not found at "
            f"{signatures_file}{Colors.RESET}"
        )
        return []

    try:
        with open(signatures_file, "r") as f:
            data = json.load(f)
            return data.get("signatures", [])
    except Exception as e:
        print(f"{Colors.RED}Error reading signatures file: {e}{Colors.RESET}")
        return []


def check_signature(github_username):
    """
    Check if a user has signed the ICLA.

    Args:
        github_username: The user's GitHub username

    Returns:
        Tuple of (has_signed, signature_info)
    """
    if not github_username:
        return False, None
        
    signatures = load_signatures()

    for sig in signatures:
        if sig.get("github_username", "").lower() == github_username.lower():
            return True, sig

    return False, None


def main():
    """Main entry point for the ICLA check."""
    # Get user information
    user_email = get_git_config("user.email")
    user_name = get_git_config("user.name")
    github_username = get_git_config("github.user")

    # Check if we have GitHub username configured
    if not github_username:
        print(
            f"{Colors.YELLOW}Warning: GitHub username not configured{Colors.RESET}"
        )
        print(
            "Please configure git with: git config github.user 'your-github-username'"
        )
        print("")
        print("Alternatively, run: python scripts/sign_icla.py")
        return 1

    # Check if user has signed
    has_signed, signature_info = check_signature(github_username)

    if has_signed:
        print(f"{Colors.GREEN}✓ ICLA Check Passed{Colors.RESET}")
        print(f"  User: {signature_info.get('name', user_name or 'Unknown')}")
        print(f"  GitHub: {signature_info.get('github_username', github_username)}")
        print(f"  Signed on: {signature_info.get('date', 'Unknown date')}")
        return 0
    else:
        print(f"{Colors.RED}{Colors.BOLD}✗ ICLA Check Failed{Colors.RESET}")
        print(
            f"{Colors.RED}You have not signed the Individual Contributor "
            f"License Agreement (ICLA).{Colors.RESET}"
        )
        print()
        print(f"{Colors.YELLOW}To sign the ICLA, please run:{Colors.RESET}")
        print(f"  {Colors.BLUE}python scripts/sign_icla.py{Colors.RESET}")
        print()
        print("Your current git configuration:")
        print(f"  GitHub: {github_username}")
        if user_name:
            print(f"  Name: {user_name}")
        if user_email:
            print(f"  Email: {user_email} (not used for ICLA check)")
        print()
        print("The ICLA is required for all contributors to ensure clear " "licensing")
        print("and intellectual property rights. This is a one-time process.")
        print()
        print("For more information, see ICLA.md in the project root.")
        return 1


if __name__ == "__main__":
    sys.exit(main())

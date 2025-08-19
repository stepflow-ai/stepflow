#!/bin/bash
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

"""
Check ICLA signatures for GitHub Actions.

This script is used by the ICLA GitHub Action to verify if a user has signed
the Individual Contributor License Agreement.
"""

import json
import os
import sys
from pathlib import Path


def find_signatures_file():
    """Find the CLA signatures file in the repository."""
    # Start from the action directory and go up to find the repo root
    current_dir = Path(__file__).parent
    while current_dir != current_dir.parent:
        signatures_file = current_dir / ".github" / "cla-signatures.json"
        if signatures_file.exists():
            return signatures_file
        current_dir = current_dir.parent
    return None


def load_signatures():
    """Load ICLA signatures from the JSON file."""
    signatures_file = find_signatures_file()
    if not signatures_file:
        return []

    try:
        with open(signatures_file, "r") as f:
            data = json.load(f)
            return data.get("signatures", [])
    except Exception as e:
        print(f"Error reading signatures file: {e}")
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
        sig_github = sig.get("github_username", "").lower()
        
        # Check GitHub username match
        if sig_github == github_username.lower():
            return True, sig
    
    return False, None


def set_github_output(name, value):
    """Set GitHub Actions output."""
    github_output = os.environ.get("GITHUB_OUTPUT")
    if github_output:
        with open(github_output, "a") as f:
            f.write(f"{name}={value}\n")
    else:
        print(f"{name}={value}")


def main():
    """Main entry point for GitHub Actions ICLA check."""
    if len(sys.argv) != 3:
        # Keep email parameter for backward compatibility but don't use it
        print("Usage: check_signature.py <github_username> <email>")
        sys.exit(1)
    
    github_username = sys.argv[1]
    # email = sys.argv[2]  # No longer used
    
    has_signed, signature_info = check_signature(github_username)
    
    if has_signed:
        print("✅ ICLA signature found")
        print(f"Name: {signature_info.get('name', 'Unknown')}")
        # Email no longer stored in public signatures
        print(f"GitHub: {signature_info.get('github_username', 'Unknown')}")
        print(f"Date: {signature_info.get('date', 'Unknown')}")
        
        set_github_output("signed", "true")
        set_github_output("signature-info", json.dumps(signature_info))
    else:
        print("❌ ICLA signature not found")
        set_github_output("signed", "false")


if __name__ == "__main__":
    main()
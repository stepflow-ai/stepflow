#!/usr/bin/env python3
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
Interactive script to sign the Individual Contributor License Agreement (ICLA).

This script guides contributors through the process of signing the ICLA,
collecting necessary information and updating the signatures file.
"""

import hashlib
import json
import subprocess
import sys
from datetime import datetime
from pathlib import Path


class Colors:
    """Terminal color codes for better output visibility."""

    RED = "\033[91m"
    GREEN = "\033[92m"
    YELLOW = "\033[93m"
    BLUE = "\033[94m"
    CYAN = "\033[96m"
    BOLD = "\033[1m"
    UNDERLINE = "\033[4m"
    RESET = "\033[0m"


CLA_VERSION = "1.0"


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


def load_icla_content():
    """
    Load the ICLA document content.

    Returns:
        The ICLA text content
    """
    project_root = get_project_root()
    icla_file = project_root / "ICLA.md"

    if not icla_file.exists():
        return """
Individual Contributor License Agreement (ICLA)

By signing this agreement, you grant the project a perpetual, worldwide,
non-exclusive, no-charge, royalty-free, irrevocable copyright and patent
license for your contributions.

You confirm that your contributions are your original work and that you
have the right to grant these licenses.

For full terms, please see ICLA.md in the project root.
"""

    try:
        with open(icla_file, "r") as f:
            return f.read()
    except Exception as e:
        print(f"{Colors.YELLOW}Warning: Could not read ICLA.md: {e}{Colors.RESET}")
        return "Please see ICLA.md for the full agreement terms."


def load_signatures():
    """
    Load existing ICLA signatures from the JSON file.

    Returns:
        The complete signatures data structure
    """
    project_root = get_project_root()
    signatures_file = project_root / ".github" / "cla-signatures.json"

    if not signatures_file.exists():
        return {"signatures": []}

    try:
        with open(signatures_file, "r") as f:
            return json.load(f)
    except Exception as e:
        print(
            f"{Colors.YELLOW}Warning: Could not read signatures file: {e}{Colors.RESET}"
        )
        return {"signatures": []}


def save_signatures(data):
    """
    Save ICLA signatures to the JSON file.

    Args:
        data: The complete signatures data structure

    Returns:
        True if successful, False otherwise
    """
    project_root = get_project_root()
    signatures_file = project_root / ".github" / "cla-signatures.json"

    # Ensure .github directory exists
    signatures_file.parent.mkdir(parents=True, exist_ok=True)

    try:
        with open(signatures_file, "w") as f:
            json.dump(data, f, indent=2, sort_keys=True)
        return True
    except Exception as e:
        print(f"{Colors.RED}Error saving signatures: {e}{Colors.RESET}")
        return False


def generate_signature_id(email, timestamp):
    """
    Generate a unique signature ID.

    Args:
        email: The signer's email
        timestamp: The signature timestamp

    Returns:
        A unique signature ID
    """
    content = f"{email}:{timestamp}"
    return hashlib.sha256(content.encode()).hexdigest()[:16]


def get_user_input(prompt, default=None, required=True, validator=None):
    """
    Get input from the user with optional default and validation.

    Args:
        prompt: The prompt to display
        default: Default value if user presses Enter
        required: Whether the field is required
        validator: Optional validation function

    Returns:
        The user's input
    """
    if default:
        prompt = f"{prompt} [{default}]: "
    else:
        prompt = f"{prompt}: "

    while True:
        value = input(prompt).strip()

        if not value and default:
            value = default

        if not value and required:
            print(
                f"{Colors.RED}This field is required. Please provide a "
                f"value.{Colors.RESET}"
            )
            continue

        if validator and value:
            error = validator(value)
            if error:
                print(f"{Colors.RED}{error}{Colors.RESET}")
                continue

        return value


def validate_email(email):
    """Validate email format."""
    if "@" not in email or "." not in email.split("@")[1]:
        return "Please enter a valid email address"
    return None


def validate_name(name):
    """Validate name format."""
    if len(name) < 2:
        return "Please enter your full legal name"
    return None


def display_icla():
    """Display the ICLA content with formatting."""
    print(f"\n{Colors.CYAN}{Colors.BOLD}{'=' * 70}{Colors.RESET}")
    print(
        f"{Colors.CYAN}{Colors.BOLD}Individual Contributor License Agreement "
        f"(ICLA){Colors.RESET}"
    )
    print(f"{Colors.CYAN}{'=' * 70}{Colors.RESET}\n")

    content = load_icla_content()

    # Display content with some basic formatting
    for line in content.split("\n"):
        if line.startswith("#"):
            print(f"{Colors.BOLD}{line}{Colors.RESET}")
        elif line.startswith("**"):
            print(f"{Colors.YELLOW}{line}{Colors.RESET}")
        else:
            print(line)

    print(f"\n{Colors.CYAN}{'=' * 70}{Colors.RESET}\n")


def main():
    """Main entry point for the ICLA signing process."""
    print(
        f"{Colors.GREEN}{Colors.BOLD}StepFlow Individual Contributor "
        f"License Agreement (ICLA) Signing{Colors.RESET}"
    )
    print(f"{Colors.GREEN}{'=' * 65}{Colors.RESET}\n")

    # Get default values from git config
    default_email = get_git_config("user.email")
    default_name = get_git_config("user.name")
    default_github = get_git_config("github.user")

    # Check if already signed
    data = load_signatures()
    existing_signature = None

    if default_email:
        for sig in data["signatures"]:
            if sig.get("email", "").lower() == default_email.lower():
                existing_signature = sig
                break

    if existing_signature:
        print(f"{Colors.YELLOW}You have already signed the ICLA:{Colors.RESET}")
        print(f"  Name: {existing_signature.get('name')}")
        print(f"  Email: {existing_signature.get('email')}")
        print(f"  Date: {existing_signature.get('date')}")
        print()

        response = (
            input("Do you want to update your signature? (y/N): ").strip().lower()
        )
        if response != "y":
            print(
                f"\n{Colors.GREEN}Your existing ICLA signature is valid. "
                f"No changes made.{Colors.RESET}"
            )
            return 0
        print()

    # Collect user information
    print(f"{Colors.BOLD}Please provide your information:{Colors.RESET}")
    print(
        f"{Colors.YELLOW}Note: Please use your full legal name as it "
        f"would appear on legal documents.{Colors.RESET}\n"
    )

    full_name = get_user_input(
        "Full Legal Name", default=default_name, required=True, validator=validate_name
    )

    email = get_user_input(
        "Email Address", default=default_email, required=True, validator=validate_email
    )

    github_username = get_user_input(
        "GitHub Username", default=default_github, required=True
    )

    company = get_user_input("Company/Organization (optional)", required=False)

    country = get_user_input("Country", required=True)

    # Display the ICLA
    print()
    display_icla()

    # Get agreement
    print(f"{Colors.BOLD}By typing 'I AGREE' below, you confirm that:{Colors.RESET}")
    print("1. You have read and understood the ICLA")
    print("2. You agree to be bound by its terms and conditions")
    print("3. The information you provided is accurate")
    print("4. You are legally able to enter into this agreement")
    print()

    agreement = input(
        f"{Colors.BOLD}Type 'I AGREE' to sign the ICLA: {Colors.RESET}"
    ).strip()

    if agreement != "I AGREE":
        print(
            f"\n{Colors.RED}ICLA signing cancelled. You must type 'I AGREE' "
            f"exactly to sign.{Colors.RESET}"
        )
        return 1

    # Create signature record
    timestamp = datetime.utcnow().isoformat() + "Z"
    signature_id = generate_signature_id(email, timestamp)

    signature = {
        "id": signature_id,
        "name": full_name,
        "email": email,
        "github_username": github_username,
        "company": company if company else None,
        "country": country,
        "date": timestamp,
        "cla_version": CLA_VERSION,
    }

    # Update or add signature
    if existing_signature:
        # Update existing signature
        for i, sig in enumerate(data["signatures"]):
            if sig.get("email", "").lower() == email.lower():
                data["signatures"][i] = signature
                break
    else:
        # Add new signature
        data["signatures"].append(signature)

    # Sort signatures by date
    data["signatures"].sort(key=lambda x: x.get("date", ""))

    # Save signatures
    if save_signatures(data):
        print(f"\n{Colors.GREEN}{Colors.BOLD}✓ ICLA Successfully Signed!{Colors.RESET}")
        print(f"{Colors.GREEN}{'=' * 30}{Colors.RESET}")
        print(f"  Signature ID: {signature_id}")
        print(f"  Name: {full_name}")
        print(f"  Email: {email}")
        print(f"  GitHub: {github_username}")
        print(f"  Date: {timestamp}")
        print()
        print(f"{Colors.CYAN}Next steps:{Colors.RESET}")
        print("1. Your ICLA signature has been saved to " ".github/cla-signatures.json")
        print("2. Include this file when you submit your first pull " "request")
        print("3. The pre-commit hook will now allow your commits")
        print("4. Thank you for contributing to StepFlow!")
        print()
        print(
            f"{Colors.YELLOW}Note: You only need to sign the ICLA "
            f"once.{Colors.RESET}"
        )
        return 0
    else:
        print(
            f"\n{Colors.RED}Failed to save signature. Please try again or "
            f"contact the maintainers.{Colors.RESET}"
        )
        return 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print(f"\n\n{Colors.YELLOW}ICLA signing cancelled by user.{Colors.RESET}")
        sys.exit(1)
    except Exception as e:
        print(f"\n{Colors.RED}An error occurred: {e}{Colors.RESET}")
        sys.exit(1)

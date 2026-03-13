#!/usr/bin/env python3
# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License. You may obtain a copy of
# the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations under
# the License.

"""End-to-end parity test for the stepflow-docling-grpc facade.

Exercises the deployed facade using the Docling paper from arXiv to verify
that the API is compatible with docling-serve. Identical to the HTTP variant's
test-parity.py — the facade API is the same regardless of worker transport.

Usage:
    python test-parity.py [--base-url URL] [--timeout SECONDS]
    python test-parity.py --help
"""

from __future__ import annotations

import argparse
import io
import json
import sys
import time

try:
    import httpx

    _HAS_HTTPX = True
except ImportError:
    _HAS_HTTPX = False
    import urllib.error
    import urllib.request

DOCLING_PAPER_URL = "https://arxiv.org/pdf/2501.17887"


# ---------------------------------------------------------------------------
# HTTP helpers (httpx preferred, stdlib fallback)
# ---------------------------------------------------------------------------


def _post_json(url: str, body: dict, timeout: float) -> tuple[int, dict]:
    """POST JSON and return (status_code, response_json)."""
    if _HAS_HTTPX:
        with httpx.Client(timeout=timeout) as c:
            r = c.post(url, json=body)
            return r.status_code, r.json()
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        url,
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status, json.loads(resp.read())
    except urllib.error.HTTPError as exc:
        return exc.code, json.loads(exc.read())


def _post_multipart(
    url: str, file_bytes: bytes, filename: str, fields: dict, timeout: float
) -> tuple[int, dict]:
    """POST multipart form data and return (status_code, response_json)."""
    if _HAS_HTTPX:
        with httpx.Client(timeout=timeout) as c:
            files = {"files": (filename, io.BytesIO(file_bytes), "application/pdf")}
            r = c.post(url, files=files, data=fields)
            return r.status_code, r.json()
    # stdlib multipart — minimal implementation
    boundary = "----StepflowParity"
    body = bytearray()
    for key, val in fields.items():
        body += f"--{boundary}\r\n".encode()
        body += f'Content-Disposition: form-data; name="{key}"\r\n\r\n'.encode()
        body += f"{val}\r\n".encode()
    body += f"--{boundary}\r\n".encode()
    body += f'Content-Disposition: form-data; name="files"; filename="{filename}"\r\n'.encode()
    body += b"Content-Type: application/pdf\r\n\r\n"
    body += file_bytes
    body += f"\r\n--{boundary}--\r\n".encode()
    req = urllib.request.Request(
        url,
        data=bytes(body),
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status, json.loads(resp.read())
    except urllib.error.HTTPError as exc:
        return exc.code, json.loads(exc.read())


def _get(url: str, timeout: float) -> tuple[int, dict | None]:
    """GET and return (status_code, response_json_or_None)."""
    if _HAS_HTTPX:
        with httpx.Client(timeout=timeout) as c:
            r = c.get(url)
            try:
                return r.status_code, r.json()
            except Exception:
                return r.status_code, None
    req = urllib.request.Request(url, method="GET")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status, json.loads(resp.read())
    except urllib.error.HTTPError as exc:
        return exc.code, None


def _download(url: str, timeout: float) -> bytes:
    """Download a URL and return bytes."""
    if _HAS_HTTPX:
        with httpx.Client(timeout=timeout, follow_redirects=True) as c:
            r = c.get(url)
            r.raise_for_status()
            return r.content
    with urllib.request.urlopen(url, timeout=timeout) as resp:
        return resp.read()


# ---------------------------------------------------------------------------
# Test cases
# ---------------------------------------------------------------------------


class TestResult:
    def __init__(self, name: str, passed: bool, detail: str = ""):
        self.name = name
        self.passed = passed
        self.detail = detail


def test_health(base_url: str, timeout: float) -> TestResult:
    """[1/6] Health check with retry."""
    deadline = time.time() + 60
    last_err = ""
    while time.time() < deadline:
        try:
            status, _ = _get(f"{base_url}/health", timeout=5)
            if status == 200:
                return TestResult("Health check", True)
            last_err = f"status={status}"
        except Exception as exc:
            last_err = str(exc)
        time.sleep(2)
    return TestResult("Health check", False, f"Not healthy after 60s: {last_err}")


def test_convert_url_v1(base_url: str, timeout: float) -> TestResult:
    """[2/6] Convert URL source (v1 API)."""
    t0 = time.time()
    try:
        status, body = _post_json(
            f"{base_url}/v1/convert/source",
            {
                "sources": [{"kind": "http", "url": DOCLING_PAPER_URL}],
                "options": {
                    "to_formats": ["md", "json", "text"],
                    "do_ocr": False,
                    "table_mode": "fast",
                },
            },
            timeout=timeout,
        )
    except Exception as exc:
        return TestResult("Convert URL source (v1)", False, str(exc))

    elapsed = time.time() - t0

    if status != 200:
        return TestResult(
            "Convert URL source (v1)", False, f"HTTP {status}: {json.dumps(body)[:200]}"
        )

    doc = body.get("document", {})
    md = doc.get("md_content", "")
    if not md:
        return TestResult("Convert URL source (v1)", False, "md_content is empty")
    if "Docling" not in md:
        return TestResult(
            "Convert URL source (v1)", False, "'Docling' not found in md_content"
        )
    if body.get("status") != "success":
        return TestResult(
            "Convert URL source (v1)",
            False,
            f"status={body.get('status')}",
        )
    pt = body.get("processing_time", 0)
    if not isinstance(pt, (int, float)) or pt <= 0:
        return TestResult(
            "Convert URL source (v1)",
            False,
            f"processing_time invalid: {pt}",
        )

    return TestResult(
        "Convert URL source (v1)",
        True,
        f"{elapsed:.1f}s, md: {len(md)} chars",
    )


def test_convert_file_v1(
    base_url: str, timeout: float, pdf_bytes: bytes
) -> TestResult:
    """[3/6] Convert file upload (v1 API)."""
    t0 = time.time()
    try:
        status, body = _post_multipart(
            f"{base_url}/v1/convert/file",
            pdf_bytes,
            "2501.17887.pdf",
            {"options": json.dumps({"to_formats": ["md"]})},
            timeout=timeout,
        )
    except Exception as exc:
        return TestResult("Convert file upload (v1)", False, str(exc))

    elapsed = time.time() - t0

    if status != 200:
        return TestResult(
            "Convert file upload (v1)",
            False,
            f"HTTP {status}: {json.dumps(body)[:200]}",
        )

    md = body.get("document", {}).get("md_content", "")
    if not md or "Docling" not in md:
        return TestResult(
            "Convert file upload (v1)",
            False,
            "md_content missing or doesn't contain 'Docling'",
        )

    return TestResult(
        "Convert file upload (v1)",
        True,
        f"{elapsed:.1f}s, md: {len(md)} chars",
    )


def test_v1alpha_compat(base_url: str, timeout: float) -> TestResult:
    """[4/6] v1alpha backward compatibility."""
    try:
        status, body = _post_json(
            f"{base_url}/v1alpha/convert/source",
            {
                "http_sources": [{"url": DOCLING_PAPER_URL}],
                "options": {"to_formats": ["md"]},
            },
            timeout=timeout,
        )
    except Exception as exc:
        return TestResult("v1alpha backward compat", False, str(exc))

    if status != 200:
        return TestResult(
            "v1alpha backward compat",
            False,
            f"HTTP {status}: {json.dumps(body)[:200]}",
        )

    md = body.get("document", {}).get("md_content", "")
    if not md:
        return TestResult("v1alpha backward compat", False, "md_content is empty")

    return TestResult("v1alpha backward compat", True)


def test_options_passthrough(base_url: str, timeout: float) -> TestResult:
    """[5/6] Options passthrough (do_ocr, table_mode)."""
    try:
        status, body = _post_json(
            f"{base_url}/v1/convert/source",
            {
                "sources": [{"kind": "http", "url": DOCLING_PAPER_URL}],
                "options": {
                    "to_formats": ["md"],
                    "do_ocr": True,
                    "table_mode": "accurate",
                },
            },
            timeout=timeout,
        )
    except Exception as exc:
        return TestResult("Options passthrough", False, str(exc))

    if status != 200:
        return TestResult(
            "Options passthrough",
            False,
            f"HTTP {status}: {json.dumps(body)[:200]}",
        )

    return TestResult("Options passthrough", True)


def test_error_handling(base_url: str, timeout: float) -> TestResult:
    """[6/6] Error handling — invalid source URL."""
    try:
        status, body = _post_json(
            f"{base_url}/v1/convert/source",
            {
                "sources": [
                    {"kind": "http", "url": "https://invalid.example.test/nofile.pdf"}
                ],
                "options": {"to_formats": ["md"]},
            },
            timeout=min(timeout, 60),
        )
    except Exception as exc:
        # Connection error from the facade itself is a failure
        return TestResult("Error handling", False, f"Unexpected exception: {exc}")

    # We accept any structured error response (4xx or 5xx with JSON body)
    if isinstance(body, dict) and (
        body.get("detail") or body.get("errors") or body.get("error")
    ):
        return TestResult("Error handling", True)

    return TestResult(
        "Error handling",
        False,
        f"Expected structured error, got HTTP {status}: {json.dumps(body)[:200]}",
    )


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(
        description="End-to-end parity test for the stepflow-docling-grpc facade."
    )
    parser.add_argument(
        "--base-url",
        default="http://localhost:5001",
        help="Base URL of the facade (default: http://localhost:5001)",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=300,
        help="Request timeout in seconds (default: 300)",
    )
    args = parser.parse_args()

    base = args.base_url.rstrip("/")
    to = args.timeout

    print(f"Stepflow-Docling gRPC Parity Test — {base}")
    print(f"Using: {'httpx' if _HAS_HTTPX else 'urllib (stdlib)'}")
    print()

    results: list[TestResult] = []

    # 1. Health
    r = test_health(base, to)
    results.append(r)
    _print_result(1, 6, r)
    if not r.passed:
        print("\nFacade not healthy — aborting remaining tests.")
        sys.exit(1)

    # 2. Convert URL (v1) — also downloads PDF for test 3
    r = test_convert_url_v1(base, to)
    results.append(r)
    _print_result(2, 6, r)

    # Download PDF for file upload test
    pdf_bytes = b""
    try:
        print("    (downloading paper for file upload test...)")
        pdf_bytes = _download(DOCLING_PAPER_URL, timeout=60)
    except Exception as exc:
        print(f"    (download failed: {exc}, skipping file upload test)")

    # 3. Convert file upload (v1)
    if pdf_bytes:
        r = test_convert_file_v1(base, to, pdf_bytes)
    else:
        r = TestResult("Convert file upload (v1)", False, "PDF download failed")
    results.append(r)
    _print_result(3, 6, r)

    # 4. v1alpha compat
    r = test_v1alpha_compat(base, to)
    results.append(r)
    _print_result(4, 6, r)

    # 5. Options passthrough
    r = test_options_passthrough(base, to)
    results.append(r)
    _print_result(5, 6, r)

    # 6. Error handling
    r = test_error_handling(base, to)
    results.append(r)
    _print_result(6, 6, r)

    # Summary
    passed = sum(1 for r in results if r.passed)
    total = len(results)
    print()
    print(f"{'='*50}")
    print(f"Results: {passed}/{total} passed")
    if passed == total:
        print("All tests passed!")
    else:
        failed = [r.name for r in results if not r.passed]
        print(f"Failed: {', '.join(failed)}")
    print(f"{'='*50}")

    sys.exit(0 if passed == total else 1)


def _print_result(idx: int, total: int, r: TestResult) -> None:
    status = "PASS" if r.passed else "FAIL"
    name = r.name.ljust(30, ".")
    detail = f" ({r.detail})" if r.detail else ""
    print(f"[{idx}/{total}] {name} {status}{detail}")


if __name__ == "__main__":
    main()

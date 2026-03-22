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

"""PDF conversion throughput and latency benchmark for stepflow-docling.

Downloads arXiv papers (cached locally), submits them through the facade in
configurable rounds and parallelism levels, and reports per-paper and aggregate
latency statistics (p50/p95/p99, throughput, server vs client overhead).

Complements test-parity.py (functional correctness) and compare-batch.py
(quality parity) with a throughput/stress perspective.

Usage:
    python test-pdf-throughput.py [options]
    python test-pdf-throughput.py --help

Examples:
    # Quick 3-paper, single-round benchmark
    python test-pdf-throughput.py --papers 3

    # Full corpus, 3 rounds, 3 concurrent
    python test-pdf-throughput.py --papers 11 --rounds 3 --parallelism 3

    # Warm-up + JSON output
    python test-pdf-throughput.py --papers 5 --rounds 2 --warm-up --output results.json

    # URL mode (worker downloads PDFs)
    python test-pdf-throughput.py --papers 3 --mode url
"""

from __future__ import annotations

import argparse
import io
import json
import math
import statistics
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import asdict, dataclass, field
from pathlib import Path

try:
    import httpx

    _HAS_HTTPX = True
except ImportError:
    _HAS_HTTPX = False
    import urllib.error
    import urllib.request

# ---------------------------------------------------------------------------
# Paper corpus — Docling first, then the 10 from run-pdf-workflow.sh
# ---------------------------------------------------------------------------

PAPERS: list[tuple[str, str, str]] = [
    ("2501.17887", "docling.pdf", "Docling Technical Report"),
    ("1706.03762", "attention-is-all-you-need.pdf", "Attention Is All You Need"),
    ("1810.04805", "bert.pdf", "BERT"),
    ("1512.03385", "resnet.pdf", "Deep Residual Learning"),
    ("1412.6980", "adam.pdf", "Adam Optimizer"),
    ("1207.0580", "dropout.pdf", "Dropout"),
    ("1502.03167", "batch-norm.pdf", "Batch Normalization"),
    ("1301.3781", "word2vec.pdf", "Word2Vec"),
    ("1406.2661", "gan.pdf", "Generative Adversarial Networks"),
    ("1409.1556", "vgg.pdf", "VGG"),
    ("1608.06993", "densenet.pdf", "DenseNet"),
]

CONVERT_OPTIONS = json.dumps(
    {"to_formats": ["md", "text"], "do_ocr": False, "table_mode": "fast"}
)


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass
class ConversionResult:
    paper_id: str
    filename: str
    round_num: int
    success: bool = False
    status_code: int = 0
    wall_time: float = 0.0
    processing_time: float = 0.0
    md_chars: int = 0
    error: str = ""


@dataclass
class RoundSummary:
    round_num: int
    wall_clock: float = 0.0
    results: list[ConversionResult] = field(default_factory=list)


@dataclass
class BenchmarkReport:
    config: dict = field(default_factory=dict)
    rounds: list[RoundSummary] = field(default_factory=list)


# ---------------------------------------------------------------------------
# HTTP helpers (httpx preferred, stdlib fallback)
# ---------------------------------------------------------------------------


def _download_url(url: str, timeout: float = 60) -> bytes:
    """Download a URL and return raw bytes."""
    if _HAS_HTTPX:
        with httpx.Client(timeout=timeout, follow_redirects=True) as c:
            r = c.get(url)
            r.raise_for_status()
            return r.content
    with urllib.request.urlopen(url, timeout=timeout) as resp:
        return resp.read()


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


def _post_file(
    url: str,
    pdf_bytes: bytes,
    filename: str,
    options_json: str,
    timeout: float,
) -> tuple[int, dict]:
    """POST multipart file upload to /v1/convert/file."""
    if _HAS_HTTPX:
        with httpx.Client(timeout=timeout) as c:
            files = {"files": (filename, io.BytesIO(pdf_bytes), "application/pdf")}
            r = c.post(url, files=files, data={"options": options_json})
            try:
                return r.status_code, r.json()
            except Exception:
                return r.status_code, {"error": r.text[:500]}

    # stdlib fallback
    boundary = "----StepflowThroughput"
    body = bytearray()
    body += f"--{boundary}\r\n".encode()
    body += b'Content-Disposition: form-data; name="options"\r\n\r\n'
    body += f"{options_json}\r\n".encode()
    body += f"--{boundary}\r\n".encode()
    body += (
        f'Content-Disposition: form-data; name="files"; '
        f'filename="{filename}"\r\n'
    ).encode()
    body += b"Content-Type: application/pdf\r\n\r\n"
    body += pdf_bytes
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
        try:
            return exc.code, json.loads(exc.read())
        except Exception:
            return exc.code, {"error": str(exc)}


def _post_source(
    url: str,
    arxiv_url: str,
    options_json: str,
    timeout: float,
) -> tuple[int, dict]:
    """POST JSON to /v1/convert/source."""
    payload = {
        "sources": [{"kind": "http", "url": arxiv_url}],
        "options": json.loads(options_json),
    }
    if _HAS_HTTPX:
        with httpx.Client(timeout=timeout) as c:
            r = c.post(url, json=payload)
            try:
                return r.status_code, r.json()
            except Exception:
                return r.status_code, {"error": r.text[:500]}

    data = json.dumps(payload).encode()
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
        try:
            return exc.code, json.loads(exc.read())
        except Exception:
            return exc.code, {"error": str(exc)}


# ---------------------------------------------------------------------------
# Download cache
# ---------------------------------------------------------------------------


def download_papers(
    pdf_dir: Path, count: int
) -> list[tuple[str, str, bytes]]:
    """Download papers to disk cache, return [(arxiv_id, filename, bytes)]."""
    pdf_dir.mkdir(parents=True, exist_ok=True)
    corpus: list[tuple[str, str, bytes]] = []

    for arxiv_id, filename, title in PAPERS[:count]:
        pdf_path = pdf_dir / filename
        url = f"https://arxiv.org/pdf/{arxiv_id}.pdf"

        if pdf_path.exists() and pdf_path.stat().st_size > 1000:
            print(f"  [CACHED] {filename}")
            corpus.append((arxiv_id, filename, pdf_path.read_bytes()))
            continue

        print(f"  [GET]   {filename} — {title}")
        try:
            data = _download_url(url)
            if len(data) > 10 * 1024 * 1024:
                print(f"          Skipping — too large ({len(data) // 1024}K)")
                continue
            pdf_path.write_bytes(data)
            corpus.append((arxiv_id, filename, data))
            print(f"          Downloaded ({len(data) // 1024}K)")
        except Exception as exc:
            print(f"          Failed: {exc}")

        time.sleep(1)

    return corpus


def load_cached_papers(
    pdf_dir: Path, count: int
) -> list[tuple[str, str, bytes]]:
    """Load already-downloaded papers from cache."""
    corpus: list[tuple[str, str, bytes]] = []
    for arxiv_id, filename, _ in PAPERS[:count]:
        pdf_path = pdf_dir / filename
        if pdf_path.exists() and pdf_path.stat().st_size > 1000:
            corpus.append((arxiv_id, filename, pdf_path.read_bytes()))
    return corpus


# ---------------------------------------------------------------------------
# Health check
# ---------------------------------------------------------------------------


def wait_for_health(base_url: str, deadline_s: float = 60) -> bool:
    """Retry health check until healthy or deadline expires."""
    deadline = time.time() + deadline_s
    last_err = ""
    while time.time() < deadline:
        try:
            status, _ = _get(f"{base_url}/health", timeout=5)
            if status == 200:
                return True
            last_err = f"status={status}"
        except Exception as exc:
            last_err = str(exc)
        time.sleep(2)
    print(f"  ERROR: Not healthy after {deadline_s:.0f}s: {last_err}")
    return False


# ---------------------------------------------------------------------------
# Conversion functions
# ---------------------------------------------------------------------------


def convert_file(
    base_url: str,
    arxiv_id: str,
    filename: str,
    pdf_bytes: bytes,
    round_num: int,
    timeout: float,
) -> ConversionResult:
    """Convert a PDF via file upload and return timing metrics."""
    result = ConversionResult(paper_id=arxiv_id, filename=filename, round_num=round_num)
    t0 = time.monotonic()
    try:
        status, body = _post_file(
            f"{base_url}/v1/convert/file",
            pdf_bytes,
            filename,
            CONVERT_OPTIONS,
            timeout,
        )
    except Exception as exc:
        result.wall_time = time.monotonic() - t0
        result.error = str(exc)
        return result

    result.wall_time = time.monotonic() - t0
    result.status_code = status
    result.success = status == 200
    result.processing_time = body.get("processing_time", 0.0) or 0.0

    doc = body.get("document") or {}
    md = doc.get("md_content", "") or ""
    result.md_chars = len(md)

    if not result.success:
        result.error = json.dumps(body)[:300]

    return result


def convert_url(
    base_url: str,
    arxiv_id: str,
    filename: str,
    round_num: int,
    timeout: float,
) -> ConversionResult:
    """Convert a PDF via URL source and return timing metrics."""
    result = ConversionResult(paper_id=arxiv_id, filename=filename, round_num=round_num)
    arxiv_url = f"https://arxiv.org/pdf/{arxiv_id}.pdf"
    t0 = time.monotonic()
    try:
        status, body = _post_source(
            f"{base_url}/v1/convert/source",
            arxiv_url,
            CONVERT_OPTIONS,
            timeout,
        )
    except Exception as exc:
        result.wall_time = time.monotonic() - t0
        result.error = str(exc)
        return result

    result.wall_time = time.monotonic() - t0
    result.status_code = status
    result.success = status == 200
    result.processing_time = body.get("processing_time", 0.0) or 0.0

    doc = body.get("document") or {}
    md = doc.get("md_content", "") or ""
    result.md_chars = len(md)

    if not result.success:
        result.error = json.dumps(body)[:300]

    return result


# ---------------------------------------------------------------------------
# Round execution
# ---------------------------------------------------------------------------


def run_round(
    round_num: int,
    base_url: str,
    mode: str,
    corpus: list[tuple[str, str, bytes]],
    parallelism: int,
    timeout: float,
) -> RoundSummary:
    """Run all papers through the facade for a single round."""
    summary = RoundSummary(round_num=round_num)
    total = len(corpus)
    t0 = time.monotonic()

    if parallelism <= 1:
        for i, (arxiv_id, filename, pdf_bytes) in enumerate(corpus):
            if mode == "url":
                r = convert_url(base_url, arxiv_id, filename, round_num, timeout)
            else:
                r = convert_file(base_url, arxiv_id, filename, pdf_bytes, round_num, timeout)
            summary.results.append(r)
            _print_progress(round_num, i + 1, total, r)
    else:
        with ThreadPoolExecutor(max_workers=parallelism) as pool:
            if mode == "url":
                futures = {
                    pool.submit(
                        convert_url, base_url, arxiv_id, filename, round_num, timeout
                    ): filename
                    for arxiv_id, filename, _ in corpus
                }
            else:
                futures = {
                    pool.submit(
                        convert_file, base_url, arxiv_id, filename, pdf_bytes, round_num, timeout
                    ): filename
                    for arxiv_id, filename, pdf_bytes in corpus
                }
            done_count = 0
            for future in as_completed(futures):
                done_count += 1
                try:
                    r = future.result()
                except Exception as exc:
                    fn = futures[future]
                    r = ConversionResult(
                        paper_id="", filename=fn, round_num=round_num, error=str(exc)
                    )
                summary.results.append(r)
                _print_progress(round_num, done_count, total, r)

    summary.wall_clock = time.monotonic() - t0
    succeeded = sum(1 for r in summary.results if r.success)
    print(
        f"  [Round {round_num}] Complete: {succeeded}/{total} succeeded, "
        f"wall clock: {summary.wall_clock:.1f}s"
    )
    return summary


def _print_progress(round_num: int, idx: int, total: int, r: ConversionResult) -> None:
    status = "OK" if r.success else "FAIL"
    name = r.filename[:28].ljust(28, ".")
    detail = f"{r.wall_time:.1f}s" if r.success else r.error[:40]
    print(f"  [R{round_num}] [{idx}/{total}] {name} {status}  ({detail})")


# ---------------------------------------------------------------------------
# Statistics
# ---------------------------------------------------------------------------


def _percentile(sorted_vals: list[float], p: float) -> float:
    """Compute percentile from sorted values."""
    if not sorted_vals:
        return 0.0
    k = (len(sorted_vals) - 1) * (p / 100.0)
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return sorted_vals[int(k)]
    return sorted_vals[f] * (c - k) + sorted_vals[c] * (k - f)


def compute_stats(report: BenchmarkReport) -> dict:
    """Compute aggregate statistics from all rounds."""
    all_results = [r for s in report.rounds for r in s.results]
    successful = [r for r in all_results if r.success]
    wall_times = sorted(r.wall_time for r in successful)
    proc_times = [r.processing_time for r in successful if r.processing_time > 0]

    total_wall_clock = sum(s.wall_clock for s in report.rounds)

    stats: dict = {
        "total_conversions": len(all_results),
        "successful": len(successful),
        "failed": len(all_results) - len(successful),
        "total_wall_clock": round(total_wall_clock, 2),
        "throughput_papers_per_min": (
            round(len(successful) / (total_wall_clock / 60), 2)
            if total_wall_clock > 0
            else 0
        ),
    }

    if wall_times:
        stats["latency"] = {
            "mean": round(statistics.mean(wall_times), 2),
            "median": round(statistics.median(wall_times), 2),
            "p95": round(_percentile(wall_times, 95), 2),
            "p99": round(_percentile(wall_times, 99), 2),
            "min": round(min(wall_times), 2),
            "max": round(max(wall_times), 2),
            "std_dev": (
                round(statistics.stdev(wall_times), 2) if len(wall_times) > 1 else 0.0
            ),
        }

    if proc_times:
        mean_proc = statistics.mean(proc_times)
        mean_wall = statistics.mean(wall_times) if wall_times else 0
        stats["server"] = {
            "mean_processing_time": round(mean_proc, 2),
            "mean_overhead": round(mean_wall - mean_proc, 2),
        }

    return stats


# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------


def print_summary(report: BenchmarkReport, stats: dict) -> None:
    """Print human-readable summary."""
    cfg = report.config
    W = 80
    print()
    print("=" * W)
    print("  THROUGHPUT SUMMARY")
    print("=" * W)
    print(f"  Target:          {cfg.get('base_url', '?')}")
    print(
        f"  Mode:            {cfg.get('mode', '?')}  |  "
        f"Parallelism: {cfg.get('parallelism', '?')}  |  "
        f"Papers: {cfg.get('papers', '?')}  |  "
        f"Rounds: {cfg.get('rounds', '?')}"
    )
    print()
    print(
        f"  Conversions:     {stats['successful']}/{stats['total_conversions']} successful"
    )
    print(f"  Total wall clock: {stats['total_wall_clock']}s")
    print(f"  Throughput:       {stats['throughput_papers_per_min']} papers/min")

    if "latency" in stats:
        lat = stats["latency"]
        print()
        print("  Per-paper latency (seconds):")
        print(
            f"    Mean:   {lat['mean']:<8}  Std Dev:  {lat['std_dev']}"
        )
        print(
            f"    p50:    {lat['median']:<8}  "
            f"p95:     {lat['p95']:<8}  p99:  {lat['p99']}"
        )
        print(f"    Min:    {lat['min']:<8}  Max:     {lat['max']}")

    if "server" in stats:
        srv = stats["server"]
        print()
        print("  Server processing time:")
        print(
            f"    Mean:   {srv['mean_processing_time']}s    "
            f"Overhead: {srv['mean_overhead']}s avg (queue + network + serialization)"
        )

    print("=" * W)


def build_json_output(report: BenchmarkReport, stats: dict) -> dict:
    """Build JSON-serializable output."""
    return {
        "config": report.config,
        "rounds": [asdict(s) for s in report.rounds],
        "statistics": stats,
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(
        description=(
            "PDF conversion throughput benchmark for stepflow-docling. "
            "Downloads arXiv papers, submits them in configurable rounds and "
            "parallelism levels, and reports latency statistics."
        ),
    )
    parser.add_argument(
        "--base-url",
        default="http://localhost:5001",
        help="Base URL of the facade (default: http://localhost:5001)",
    )
    parser.add_argument(
        "--mode",
        choices=["file", "url"],
        default="file",
        help="Submission mode: file (upload) or url (pass arXiv URL) (default: file)",
    )
    parser.add_argument(
        "--parallelism",
        type=int,
        default=3,
        help="Concurrent HTTP submissions (default: 3)",
    )
    parser.add_argument(
        "--papers",
        type=int,
        default=3,
        help="Number of papers to use (default: 3, max: 11)",
    )
    parser.add_argument(
        "--rounds",
        type=int,
        default=1,
        help="Repeat the full set N times (default: 1)",
    )
    parser.add_argument(
        "--warm-up",
        action="store_true",
        help="Run 1 conversion before timed rounds (excluded from stats)",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=300,
        help="Per-request timeout in seconds (default: 300)",
    )
    parser.add_argument(
        "--pdf-dir",
        default=None,
        help="Cache directory for PDFs (default: ./arxiv-pdfs relative to script)",
    )
    parser.add_argument(
        "--skip-download",
        action="store_true",
        help="Use only cached PDFs (fail if cache is empty)",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print JSON instead of table to stdout",
    )
    parser.add_argument(
        "--output",
        default=None,
        help="Write JSON results to file (table still printed to stdout)",
    )
    args = parser.parse_args()

    base_url = args.base_url.rstrip("/")
    paper_count = min(args.papers, len(PAPERS))
    script_dir = Path(__file__).resolve().parent
    pdf_dir = Path(args.pdf_dir) if args.pdf_dir else script_dir / "arxiv-pdfs"

    config = {
        "base_url": base_url,
        "mode": args.mode,
        "parallelism": args.parallelism,
        "papers": paper_count,
        "rounds": args.rounds,
        "warm_up": args.warm_up,
        "timeout": args.timeout,
    }

    if not args.json:
        print("=" * 60)
        print("  Stepflow-Docling Throughput Benchmark")
        print(f"  HTTP client: {'httpx' if _HAS_HTTPX else 'urllib (stdlib)'}")
        print("=" * 60)
        print()

    # -- Download / load corpus ------------------------------------------
    if not args.json:
        print("Loading papers...")
    if args.skip_download:
        corpus = load_cached_papers(pdf_dir, paper_count)
    else:
        corpus = download_papers(pdf_dir, paper_count)

    if not corpus:
        print("ERROR: No papers available. Run without --skip-download first.")
        sys.exit(1)

    if not args.json:
        print(f"Corpus: {len(corpus)} papers ready\n")

    # -- Health check ----------------------------------------------------
    if not args.json:
        print(f"Checking health at {base_url}...")
    if not wait_for_health(base_url):
        print("ERROR: Facade not healthy — aborting.")
        sys.exit(1)
    if not args.json:
        print("  Healthy!\n")

    # -- Warm-up ---------------------------------------------------------
    if args.warm_up:
        if not args.json:
            print("Warm-up: converting first paper (excluded from stats)...")
        arxiv_id, filename, pdf_bytes = corpus[0]
        if args.mode == "url":
            r = convert_url(base_url, arxiv_id, filename, 0, args.timeout)
        else:
            r = convert_file(base_url, arxiv_id, filename, pdf_bytes, 0, args.timeout)
        status = "OK" if r.success else "FAIL"
        if not args.json:
            print(
                f"  {filename}: {status} ({r.wall_time:.1f}s) "
                f"(warm-up, excluded from stats)\n"
            )

    # -- Timed rounds ----------------------------------------------------
    report = BenchmarkReport(config=config)

    for round_num in range(1, args.rounds + 1):
        if not args.json:
            print(f"--- Round {round_num}/{args.rounds} ---")
        summary = run_round(
            round_num, base_url, args.mode, corpus, args.parallelism, args.timeout
        )
        report.rounds.append(summary)
        if not args.json:
            print()

    # -- Statistics and output -------------------------------------------
    stats = compute_stats(report)

    if args.json:
        json.dump(build_json_output(report, stats), sys.stdout, indent=2)
        print()
    else:
        print_summary(report, stats)

    if args.output:
        with open(args.output, "w") as f:
            json.dump(build_json_output(report, stats), f, indent=2)
        if not args.json:
            print(f"\nResults written to: {args.output}")


if __name__ == "__main__":
    main()

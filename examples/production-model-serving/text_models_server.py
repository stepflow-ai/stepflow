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

"""
Production Model Serving Component Server - Text Models

This server demonstrates production-level model serving capabilities:
1. Hugging Face Transformers integration for text generation and classification
2. Model caching and memory management
3. GPU/CPU resource optimization
4. Batch processing capabilities
5. Health checks and monitoring

In production, this would run on dedicated GPU instances with:
- Model warming and persistent memory allocation
- Load balancing across multiple instances
- Resource-specific routing (e.g., GPU vs CPU models)
- Monitoring and observability
"""

import sys
import os

try:
    from stepflow_py import StepflowHttpServer, StepflowServer, StepflowContext
    import msgspec

    SDK_AVAILABLE = True
except ImportError as e:
    print(f"Stepflow SDK not available: {e}", file=sys.stderr)
    print(
        "Please install stepflow-py or run from the project root with proper PYTHONPATH",
        file=sys.stderr
    )
    SDK_AVAILABLE = False
from typing import List, Optional, Dict, Any
import asyncio
import logging
import json
import time

# Get logger - configuration is handled by SDK's setup_observability()
logger = logging.getLogger(__name__)

# Early exit if SDK not available
if not SDK_AVAILABLE:
    logger.error("Cannot start server without Stepflow SDK")
    sys.exit(1)

# Try to import transformers, fallback to mock implementations for demo
try:
    from transformers import pipeline, AutoTokenizer, AutoModelForCausalLM
    import torch

    HF_AVAILABLE = True
    logger.info("Transformers library available")
except ImportError:
    HF_AVAILABLE = False
    logger.warning("Transformers library not available, using mock implementations")

# Create the server - will be used for STDIO mode and component registration
server = StepflowServer()

# Model registry - in production this would be externalized
MODEL_REGISTRY = {
    "gpt2-small": {
        "model_id": "gpt2",
        "task": "text-generation",
        "resource_tier": "cpu",
        "max_tokens": 512,
    },
    "distilbert-sentiment": {
        "model_id": "distilbert-base-uncased-finetuned-sst-2-english",
        "task": "sentiment-analysis",
        "resource_tier": "cpu",
        "max_tokens": 512,
    },
    "flan-t5-small": {
        "model_id": "google/flan-t5-small",
        "task": "text2text-generation",
        "resource_tier": "gpu",
        "max_tokens": 512,
    },
}

# Global model cache - in production this would be managed by a model server
_model_cache = {}


# Input/Output schemas
class TextGenerationInput(msgspec.Struct):
    prompt: str
    model_name: str = "gpt2-small"
    max_length: int = 100
    temperature: float = 0.7
    do_sample: bool = True


class TextGenerationOutput(msgspec.Struct):
    generated_text: str
    model_used: str
    generation_time_ms: float
    resource_tier: str


class SentimentAnalysisInput(msgspec.Struct):
    text: str
    model_name: str = "distilbert-sentiment"


class SentimentAnalysisOutput(msgspec.Struct):
    label: str
    score: float
    model_used: str
    inference_time_ms: float


class BatchTextInput(msgspec.Struct):
    texts: List[str]
    model_name: str
    task: str  # "generation" or "sentiment"
    batch_size: int = 4


class BatchTextOutput(msgspec.Struct):
    results: List[Dict[str, Any]]
    total_time_ms: float
    throughput_texts_per_sec: float


class ModelHealthInput(msgspec.Struct):
    detailed: bool = False


class ModelHealthOutput(msgspec.Struct):
    status: str
    available_models: List[str]
    loaded_models: List[str]
    gpu_available: bool
    memory_usage: Optional[Dict[str, Any]] = None


def get_or_load_model(model_name: str):
    """Load model into cache if not already loaded."""
    if not HF_AVAILABLE:
        logger.info(f"Mock: Loading model {model_name}")
        _model_cache[model_name] = f"mock_model_{model_name}"
        return _model_cache[model_name]

    if model_name in _model_cache:
        return _model_cache[model_name]

    if model_name not in MODEL_REGISTRY:
        raise ValueError(f"Unknown model: {model_name}")

    config = MODEL_REGISTRY[model_name]
    logger.info(f"Loading model {config['model_id']} for task {config['task']}")

    start_time = time.time()
    try:
        # In production, this would include device placement logic
        device = (
            0 if torch.cuda.is_available() and config["resource_tier"] == "gpu" else -1
        )
        model = pipeline(config["task"], model=config["model_id"], device=device)
        _model_cache[model_name] = model

        load_time = (time.time() - start_time) * 1000
        logger.info(f"Model {model_name} loaded in {load_time:.2f}ms")
        return model
    except Exception as e:
        logger.error(f"Failed to load model {model_name}: {e}")
        raise


@server.component
async def generate_text(input: TextGenerationInput) -> TextGenerationOutput:
    """Generate text using specified language model."""
    start_time = time.time()

    try:
        model = get_or_load_model(input.model_name)
        config = MODEL_REGISTRY[input.model_name]

        if not HF_AVAILABLE:
            # Mock response for demo
            generated = f"[MOCK] Generated response to: {input.prompt[:50]}..."
        else:
            # Real generation
            result = model(
                input.prompt,
                max_length=min(input.max_length, config["max_tokens"]),
                temperature=input.temperature,
                do_sample=input.do_sample,
                pad_token_id=model.tokenizer.eos_token_id,
            )
            generated = result[0]["generated_text"]

        generation_time = (time.time() - start_time) * 1000

        return TextGenerationOutput(
            generated_text=generated,
            model_used=input.model_name,
            generation_time_ms=generation_time,
            resource_tier=config["resource_tier"],
        )

    except Exception as e:
        logger.error(f"Text generation failed: {e}")
        raise


@server.component
async def analyze_sentiment(input: SentimentAnalysisInput) -> SentimentAnalysisOutput:
    """Analyze sentiment of input text."""
    start_time = time.time()

    try:
        model = get_or_load_model(input.model_name)
        config = MODEL_REGISTRY[input.model_name]

        print(
            f"Analyzing sentiment for: {input.text} using {input.model_name}",
            file=sys.stderr,
        )
        if not HF_AVAILABLE:
            # Mock response
            result = {"label": "POSITIVE", "score": 0.9}
        else:
            # Real inference
            result = model(input.text)[0]
        print(f"Sentiment result: {result}", file=sys.stderr)

        inference_time = (time.time() - start_time) * 1000

        return SentimentAnalysisOutput(
            label=result["label"],
            score=result["score"],
            model_used=input.model_name,
            inference_time_ms=inference_time,
        )

    except Exception as e:
        logger.error(f"Sentiment analysis failed: {e!r}")
        raise


@server.component
async def batch_process_text(input: BatchTextInput) -> BatchTextOutput:
    """Process multiple texts in batches for improved throughput."""
    start_time = time.time()

    try:
        model = get_or_load_model(input.model_name)
        results = []

        # Process in batches
        for i in range(0, len(input.texts), input.batch_size):
            batch = input.texts[i : i + input.batch_size]

            if not HF_AVAILABLE:
                # Mock batch processing
                batch_results = [
                    {"text": text, "result": f"[MOCK] Processed: {text[:30]}..."}
                    for text in batch
                ]
            else:
                if input.task == "sentiment":
                    batch_results = []
                    for text in batch:
                        result = model(text)[0]
                        batch_results.append(
                            {
                                "text": text,
                                "label": result["label"],
                                "score": result["score"],
                            }
                        )
                elif input.task == "generation":
                    batch_results = []
                    for text in batch:
                        result = model(text, max_length=100)[0]
                        batch_results.append(
                            {"prompt": text, "generated_text": result["generated_text"]}
                        )
                else:
                    raise ValueError(f"Unknown task: {input.task}")

            results.extend(batch_results)

        total_time = (time.time() - start_time) * 1000
        throughput = len(input.texts) / (total_time / 1000) if total_time > 0 else 0

        return BatchTextOutput(
            results=results,
            total_time_ms=total_time,
            throughput_texts_per_sec=throughput,
        )

    except Exception as e:
        logger.error(f"Batch processing failed: {e}")
        raise


@server.component
async def model_health_check(input: ModelHealthInput) -> ModelHealthOutput:
    """Check health and status of model serving infrastructure."""

    available_models = list(MODEL_REGISTRY.keys())
    loaded_models = list(_model_cache.keys())

    memory_info = None
    if input.detailed and HF_AVAILABLE:
        try:
            import psutil

            process = psutil.Process()
            memory_info = {
                "rss_mb": process.memory_info().rss / 1024 / 1024,
                "vms_mb": process.memory_info().vms / 1024 / 1024,
                "cpu_percent": process.cpu_percent(),
            }

            if torch.cuda.is_available():
                memory_info["gpu_memory_allocated"] = (
                    torch.cuda.memory_allocated() / 1024 / 1024
                )
                memory_info["gpu_memory_cached"] = (
                    torch.cuda.memory_reserved() / 1024 / 1024
                )
        except ImportError:
            logger.warning("psutil not available for detailed memory info")

    return ModelHealthOutput(
        status="healthy",
        available_models=available_models,
        loaded_models=loaded_models,
        gpu_available=(
            HF_AVAILABLE and torch.cuda.is_available() if HF_AVAILABLE else False
        ),
        memory_usage=memory_info,
    )


if __name__ == "__main__":
    logger.info("Starting Text Models Server")
    logger.info(f"Available models: {list(MODEL_REGISTRY.keys())}")
    logger.info(
        f"GPU available: {HF_AVAILABLE and torch.cuda.is_available() if HF_AVAILABLE else False}"
    )

    http_server = StepflowHttpServer(server)
    asyncio.run(http_server.run())

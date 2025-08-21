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
Production Model Serving Component Server - Vision Models

This server demonstrates vision model serving capabilities:
1. Image classification and object detection
2. Different resource requirements (GPU-intensive)
3. Image preprocessing and batch processing
4. Model routing based on image properties

In production, this would typically run on GPU-enabled instances
with specialized hardware for computer vision workloads.
"""

import sys
import os

try:
    from stepflow_py import StepflowStdioServer, StepflowContext
    from stepflow_py.server import StepflowServer

    try:
        from stepflow_py.http_server import StepflowHttpServer
    except ImportError:
        StepflowHttpServer = None
    import msgspec

    SDK_AVAILABLE = True
except ImportError as e:
    print(f"Stepflow SDK not available: {e}")
    print(
        "Please install stepflow-py or run from the project root with proper PYTHONPATH"
    )
    SDK_AVAILABLE = False
from typing import List, Optional, Dict, Any, Tuple
import asyncio
import logging
import time
import base64
import io

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Early exit if SDK not available
if not SDK_AVAILABLE:
    logger.error("Cannot start server without Stepflow SDK")
    sys.exit(1)

# Try to import computer vision libraries
try:
    from PIL import Image
    import torch
    from transformers import (
        pipeline,
        AutoImageProcessor,
        AutoModelForImageClassification,
    )

    CV_AVAILABLE = True
    logger.info("Computer vision libraries available")
except ImportError:
    CV_AVAILABLE = False
    logger.warning(
        "Computer vision libraries not available, using mock implementations"
    )

# Create the server
server = StepflowServer()

# Vision model registry
VISION_MODEL_REGISTRY = {
    "resnet-imagenet": {
        "model_id": "microsoft/resnet-50",
        "task": "image-classification",
        "resource_tier": "gpu",
        "input_size": (224, 224),
        "description": "ResNet-50 trained on ImageNet for general image classification",
    },
    "vit-imagenet": {
        "model_id": "google/vit-base-patch16-224",
        "task": "image-classification",
        "resource_tier": "gpu",
        "input_size": (224, 224),
        "description": "Vision Transformer for image classification",
    },
    "deit-efficient": {
        "model_id": "facebook/deit-tiny-patch16-224",
        "task": "image-classification",
        "resource_tier": "cpu",
        "input_size": (224, 224),
        "description": "Efficient vision model for CPU inference",
    },
}

# Global model cache
_vision_model_cache = {}


# Input/Output schemas
class ImageClassificationInput(msgspec.Struct):
    image_data: str  # Base64 encoded image
    model_name: str = "resnet-imagenet"
    top_k: int = 5


class ImageClassificationOutput(msgspec.Struct):
    predictions: List[Dict[str, Any]]  # [{"label": str, "score": float}]
    model_used: str
    inference_time_ms: float
    image_size: Tuple[int, int]
    resource_tier: str


class BatchImageInput(msgspec.Struct):
    images: List[str]  # List of base64 encoded images
    model_name: str = "deit-efficient"
    batch_size: int = 4


class BatchImageOutput(msgspec.Struct):
    results: List[Dict[str, Any]]
    total_time_ms: float
    throughput_images_per_sec: float
    average_inference_time_ms: float


class ImageMetricsInput(msgspec.Struct):
    image_data: str


class ImageMetricsOutput(msgspec.Struct):
    width: int
    height: int
    format: str
    mode: str
    size_bytes: int
    recommended_model: str
    preprocessing_required: bool


class VisionHealthInput(msgspec.Struct):
    include_model_details: bool = False


class VisionHealthOutput(msgspec.Struct):
    status: str
    available_models: List[str]
    loaded_models: List[str]
    gpu_memory_info: Optional[Dict[str, Any]] = None
    model_details: Optional[Dict[str, Dict[str, Any]]] = None


def decode_image(image_data: str) -> "Image.Image":
    """Decode base64 image data to PIL Image."""
    if not CV_AVAILABLE:
        # Return mock image info
        return {"width": 224, "height": 224, "format": "JPEG"}

    try:
        # Remove data URL prefix if present
        if image_data.startswith("data:image"):
            image_data = image_data.split(",")[1]

        image_bytes = base64.b64decode(image_data)
        image = Image.open(io.BytesIO(image_bytes))
        return image
    except Exception as e:
        logger.error(f"Failed to decode image: {e}")
        raise ValueError(f"Invalid image data: {e}")


def get_or_load_vision_model(model_name: str):
    """Load vision model into cache if not already loaded."""
    if not CV_AVAILABLE:
        logger.info(f"Mock: Loading vision model {model_name}")
        _vision_model_cache[model_name] = f"mock_vision_model_{model_name}"
        return _vision_model_cache[model_name]

    if model_name in _vision_model_cache:
        return _vision_model_cache[model_name]

    if model_name not in VISION_MODEL_REGISTRY:
        raise ValueError(f"Unknown vision model: {model_name}")

    config = VISION_MODEL_REGISTRY[model_name]
    logger.info(f"Loading vision model {config['model_id']} for task {config['task']}")

    start_time = time.time()
    try:
        # In production, this would include device placement logic
        device = (
            0 if torch.cuda.is_available() and config["resource_tier"] == "gpu" else -1
        )
        model = pipeline(config["task"], model=config["model_id"], device=device)
        _vision_model_cache[model_name] = model

        load_time = (time.time() - start_time) * 1000
        logger.info(f"Vision model {model_name} loaded in {load_time:.2f}ms")
        return model
    except Exception as e:
        logger.error(f"Failed to load vision model {model_name}: {e}")
        raise


def recommend_model_for_image(image_size: Tuple[int, int]) -> str:
    """Recommend optimal model based on image characteristics."""
    width, height = image_size
    total_pixels = width * height

    # Simple heuristic - in production this would be more sophisticated
    if total_pixels > 1024 * 1024:  # Large images
        return "vit-imagenet"  # Vision transformer handles large images well
    elif total_pixels < 256 * 256:  # Small images
        return "deit-efficient"  # Efficient model for quick processing
    else:
        return "resnet-imagenet"  # General purpose model


@server.component
async def classify_image(input: ImageClassificationInput) -> ImageClassificationOutput:
    """Classify image using specified vision model."""
    start_time = time.time()

    try:
        # Decode and process image
        image = decode_image(input.image_data)

        if not CV_AVAILABLE:
            # Mock response
            image_size = (224, 224)
            predictions = [
                {"label": "mock_class_1", "score": 0.85},
                {"label": "mock_class_2", "score": 0.12},
                {"label": "mock_class_3", "score": 0.03},
            ]
        else:
            image_size = image.size
            model = get_or_load_vision_model(input.model_name)

            # Perform inference
            results = model(image)

            # Format results
            predictions = sorted(results, key=lambda x: x["score"], reverse=True)[
                : input.top_k
            ]

        inference_time = (time.time() - start_time) * 1000
        config = VISION_MODEL_REGISTRY[input.model_name]

        return ImageClassificationOutput(
            predictions=predictions,
            model_used=input.model_name,
            inference_time_ms=inference_time,
            image_size=image_size,
            resource_tier=config["resource_tier"],
        )

    except Exception as e:
        logger.error(f"Image classification failed: {e}")
        raise


@server.component
async def batch_classify_images(input: BatchImageInput) -> BatchImageOutput:
    """Process multiple images in batches for improved throughput."""
    start_time = time.time()

    try:
        model = get_or_load_vision_model(input.model_name)
        results = []
        total_inference_time = 0

        # Process in batches
        for i in range(0, len(input.images), input.batch_size):
            batch = input.images[i : i + input.batch_size]
            batch_start_time = time.time()

            if not CV_AVAILABLE:
                # Mock batch processing
                batch_results = [
                    {
                        "image_index": i + j,
                        "predictions": [{"label": f"mock_class_{j}", "score": 0.8}],
                        "inference_time_ms": 50.0,
                    }
                    for j, img in enumerate(batch)
                ]
            else:
                batch_results = []
                for j, image_data in enumerate(batch):
                    inference_start = time.time()
                    image = decode_image(image_data)
                    predictions = model(image)
                    inference_time = (time.time() - inference_start) * 1000

                    batch_results.append(
                        {
                            "image_index": i + j,
                            "predictions": sorted(
                                predictions, key=lambda x: x["score"], reverse=True
                            )[:5],
                            "inference_time_ms": inference_time,
                        }
                    )
                    total_inference_time += inference_time

            results.extend(batch_results)

        total_time = (time.time() - start_time) * 1000
        throughput = len(input.images) / (total_time / 1000) if total_time > 0 else 0
        avg_inference_time = (
            total_inference_time / len(input.images) if len(input.images) > 0 else 0
        )

        return BatchImageOutput(
            results=results,
            total_time_ms=total_time,
            throughput_images_per_sec=throughput,
            average_inference_time_ms=avg_inference_time,
        )

    except Exception as e:
        logger.error(f"Batch image processing failed: {e}")
        raise


@server.component
async def analyze_image_metrics(input: ImageMetricsInput) -> ImageMetricsOutput:
    """Analyze image properties and recommend optimal processing model."""

    try:
        if not CV_AVAILABLE:
            # Mock response
            return ImageMetricsOutput(
                width=224,
                height=224,
                format="JPEG",
                mode="RGB",
                size_bytes=len(input.image_data)
                * 3
                // 4,  # Approximate base64 decode size
                recommended_model="deit-efficient",
                preprocessing_required=False,
            )

        image = decode_image(input.image_data)
        size_bytes = len(
            base64.b64decode(
                input.image_data.split(",")[1]
                if "," in input.image_data
                else input.image_data
            )
        )

        recommended_model = recommend_model_for_image(image.size)

        # Check if preprocessing is needed
        config = VISION_MODEL_REGISTRY[recommended_model]
        target_size = config["input_size"]
        preprocessing_required = image.size != target_size

        return ImageMetricsOutput(
            width=image.size[0],
            height=image.size[1],
            format=image.format or "Unknown",
            mode=image.mode,
            size_bytes=size_bytes,
            recommended_model=recommended_model,
            preprocessing_required=preprocessing_required,
        )

    except Exception as e:
        logger.error(f"Image metrics analysis failed: {e}")
        raise


@server.component
async def vision_health_check(input: VisionHealthInput) -> VisionHealthOutput:
    """Check health and status of vision model serving infrastructure."""

    available_models = list(VISION_MODEL_REGISTRY.keys())
    loaded_models = list(_vision_model_cache.keys())

    gpu_memory_info = None
    model_details = None

    if CV_AVAILABLE and torch.cuda.is_available():
        try:
            gpu_memory_info = {
                "gpu_memory_allocated_mb": torch.cuda.memory_allocated() / 1024 / 1024,
                "gpu_memory_cached_mb": torch.cuda.memory_reserved() / 1024 / 1024,
                "gpu_memory_total_mb": torch.cuda.get_device_properties(0).total_memory
                / 1024
                / 1024,
                "gpu_count": torch.cuda.device_count(),
                "current_device": torch.cuda.current_device(),
            }
        except Exception as e:
            logger.warning(f"Could not get GPU memory info: {e}")

    if input.include_model_details:
        model_details = {}
        for model_name, config in VISION_MODEL_REGISTRY.items():
            model_details[model_name] = {
                "config": config,
                "loaded": model_name in _vision_model_cache,
                "memory_estimate_mb": (
                    100 if config["resource_tier"] == "cpu" else 500
                ),  # Rough estimates
            }

    result = VisionHealthOutput(
        status="healthy",
        available_models=available_models,
        loaded_models=loaded_models,
        gpu_memory_info=gpu_memory_info,
        model_details=model_details,
    )
    print(f"Health check result: {result}", file=sys.stderr)
    return result


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Vision Models Server")
    parser.add_argument("--http", action="store_true", help="Run in HTTP mode")
    parser.add_argument("--port", type=int, default=8081, help="Port for HTTP mode")
    parser.add_argument(
        "--host", type=str, default="localhost", help="Host for HTTP mode"
    )
    args = parser.parse_args()

    logger.info("Starting Vision Models Server")
    logger.info(f"Available models: {list(VISION_MODEL_REGISTRY.keys())}")
    logger.info(
        f"GPU available: {CV_AVAILABLE and torch.cuda.is_available() if CV_AVAILABLE else False}"
    )

    if args.http:
        if StepflowHttpServer is None:
            logger.error("HTTP mode requested but StepflowHttpServer not available")
            sys.exit(1)
        logger.info(f"Running in HTTP mode on {args.host}:{args.port}")
        http_server = StepflowHttpServer(host=args.host, port=args.port, server=server)

        import asyncio

        asyncio.run(http_server.run())
    else:
        stdio_server = StepflowStdioServer(server)
        logger.info("Running in STDIO mode")
        stdio_server.run()

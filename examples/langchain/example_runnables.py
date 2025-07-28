#!/usr/bin/env python3
"""
Example runnables for demonstrating import path-based LangChain integration.

This module contains sample runnables that can be imported using Python import paths
like "example_runnables.text_processor" or "example_runnables:stats_calculator".
"""

from langchain_core.runnables import RunnableLambda


def process_text(data):
    """Process text by capitalizing words and providing statistics."""
    text = data["text"]
    words = text.split()
    return {
        "processed_text": " ".join(word.capitalize() for word in words),
        "word_count": len(words),
        "original_length": len(text)
    }


def calculate_stats(data):
    """Calculate statistical measures for a list of numbers."""
    numbers = data["numbers"]
    if not numbers:
        return {"error": "No numbers provided"}
    return {
        "sum": sum(numbers),
        "mean": sum(numbers) / len(numbers),
        "median": sorted(numbers)[len(numbers) // 2],
        "range": max(numbers) - min(numbers)
    }


def format_string(data):
    """Format a string using template variables."""
    template = data.get("template", "Hello {name}!")
    values = data.get("values", {})
    try:
        return {"formatted": template.format(**values)}
    except KeyError as e:
        return {"error": f"Missing template variable: {e}"}


# Create runnables that can be imported by path
text_processor = RunnableLambda(process_text)
stats_calculator = RunnableLambda(calculate_stats)  
string_formatter = RunnableLambda(format_string)
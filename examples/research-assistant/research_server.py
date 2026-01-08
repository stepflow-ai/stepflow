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
Research Assistant LangChain Server

This server provides AI-powered research components using LangChain
for the Stepflow research assistant workflow.
"""

import json
import sys
from datetime import datetime
from typing import Any

try:
    import msgspec
    from langchain_core.runnables import RunnableLambda

    from stepflow_py import StepflowServer, except ImportError as e:
    print(f"Error: Missing required dependencies: {e}", file=sys.stderr)
    print("Please install with:", file=sys.stderr)
    print("  cd ../../sdks/python", file=sys.stderr)
    print("  uv add --group dev langchain-core", file=sys.stderr)
    sys.exit(1)


class QuestionGeneratorInput(msgspec.Struct):
    """Input for research question generation."""
    topic: str
    context: str
    execution_mode: str = "invoke"


class QuestionGeneratorOutput(msgspec.Struct):
    """Output from research question generation."""
    questions: list[str]
    formatted_output: str
    metadata: dict[str, Any]


class TextAnalyzerInput(msgspec.Struct):
    """Input for text analysis."""
    text: str
    topic: str
    analysis_type: str = "research_summary"
    execution_mode: str = "invoke"


class TextAnalyzerOutput(msgspec.Struct):
    """Output from text analysis."""
    summary: str
    key_points: list[str]
    formatted_analysis: str
    statistics: dict[str, Any]


class NoteGeneratorInput(msgspec.Struct):
    """Input for research note generation."""
    topic: str
    questions: list[str]
    analysis: str
    execution_mode: str = "invoke"


class NoteGeneratorOutput(msgspec.Struct):
    """Output from research note generation."""
    notes: str
    sections: dict[str, str]
    metadata: dict[str, Any]


class ReportGeneratorInput(msgspec.Struct):
    """Input for report generation."""
    topic: str
    questions: dict[str, Any]
    analysis: dict[str, Any]
    notes: dict[str, Any]
    execution_mode: str = "invoke"


class ReportGeneratorOutput(msgspec.Struct):
    """Output from report generation."""
    report: dict[str, Any]
    report_json: str
    summary: str


# Initialize the Stepflow server
server = StepflowServer()


@server.langchain_component(name="question_generator")
def create_question_generator():
    """Generate research questions based on a topic and context."""

    def generate_questions(data):
        topic = data["topic"]
        context = data["context"]

        # Generate research questions based on the topic and context
        # In a real implementation, this could use an LLM
        questions = [
            f"What are the fundamental principles of {topic}?",
            f"How does {topic} relate to current industry trends?",
            f"What are the main challenges and opportunities in {topic}?",
            f"Who are the key researchers and organizations working on {topic}?",
            f"What are the practical applications of {topic}?",
            f"How has {topic} evolved over time?",
            f"What future developments are expected in {topic}?",
            f"What are the ethical considerations related to {topic}?"
        ]

        # Add context-specific questions
        if "AI" in context or "artificial intelligence" in context.lower():
            questions.append(
                f"How does {topic} intersect with artificial intelligence?"
            )
        if "data" in context.lower():
            questions.append(f"What role does data play in {topic}?")
        if "workflow" in context.lower() or "orchestration" in context.lower():
            questions.append(
                f"How can {topic} be integrated into workflow orchestration?"
            )

        # Format the output as markdown
        formatted_output = f"""# Research Questions for: {topic}

Generated on: {datetime.now().strftime("%Y-%m-%d %H:%M:%S")}

## Core Research Questions:

"""
        for i, question in enumerate(questions, 1):
            formatted_output += f"{i}. {question}\n"

        formatted_output += f"""

## Context Considerations:
Based on the provided context, these questions explore {topic} from multiple perspectives,
including theoretical foundations, practical applications, and future directions.

### Initial Context:
{context[:500]}{"..." if len(context) > 500 else ""}
"""

        return {
            "questions": questions,
            "formatted_output": formatted_output,
            "metadata": {
                "total_questions": len(questions),
                "generation_time": datetime.now().isoformat(),
                "topic": topic,
                "context_length": len(context)
            }
        }

    return RunnableLambda(generate_questions)


@server.langchain_component(name="text_analyzer")
def create_text_analyzer():
    """Analyze text for research insights."""

    def analyze_text(data):
        text = data["text"]
        topic = data["topic"]
        analysis_type = data.get("analysis_type", "research_summary")

        # Perform text analysis
        # In a real implementation, this could use NLP libraries or LLMs
        words = text.split()
        sentences = text.split('.')

        # Extract key points (simplified version)
        key_points = []
        for sentence in sentences[:5]:  # First 5 sentences as key points
            if len(sentence.strip()) > 20:
                key_points.append(sentence.strip() + ".")

        # Generate summary
        summary = f"""The provided text about {topic} contains {len(words)} words and {len(sentences)} sentences.
The content appears to focus on {analysis_type} aspects of the topic.
Key themes identified include technical implementation, practical applications, and system architecture."""

        # Format the analysis
        formatted_analysis = f"""# Analysis Summary: {topic}

## Overview
{summary}

## Key Points Identified:
"""
        for i, point in enumerate(key_points, 1):
            formatted_analysis += f"\n{i}. {point}"

        formatted_analysis += f"""

## Statistical Analysis:
- Total words: {len(words)}
- Total sentences: {len(sentences)}
- Average words per sentence: {len(words) // max(len(sentences), 1)}
- Unique words: {len({word.lower() for word in words})}

## Topic Relevance:
The text provides {"substantial" if len(words) > 100 else "limited"} context about {topic}.
Further research is {"recommended" if len(words) < 200 else "optional"} to fully understand the topic.

Analysis completed on: {datetime.now().strftime("%Y-%m-%d %H:%M:%S")}
"""

        return {
            "summary": summary,
            "key_points": key_points,
            "formatted_analysis": formatted_analysis,
            "statistics": {
                "word_count": len(words),
                "sentence_count": len(sentences),
                "unique_words": len({word.lower() for word in words}),
                "avg_words_per_sentence": len(words) // max(len(sentences), 1)
            }
        }

    return RunnableLambda(analyze_text)


@server.langchain_component(name="note_generator")
def create_note_generator():
    """Generate structured research notes."""

    def generate_notes(data):
        topic = data["topic"]
        questions = data["questions"]
        analysis = data["analysis"]

        # Generate structured notes
        notes = f"""# Research Notes: {topic}

Generated: {datetime.now().strftime("%Y-%m-%d %H:%M:%S")}

## Executive Summary
{analysis}

## Research Framework

### Primary Research Questions
"""

        # Add questions to notes
        for i, question in enumerate(questions[:5], 1):  # First 5 questions
            notes += f"\n**Q{i}:** {question}\n"
            notes += "- Status: Pending investigation\n"
            notes += f"- Priority: {'High' if i <= 3 else 'Medium'}\n"

        notes += """

## Key Insights

### Current Understanding
Based on the initial analysis, the following insights have been identified:
- The topic encompasses multiple dimensions requiring systematic investigation
- Both theoretical and practical aspects need to be explored
- Integration with existing systems is a key consideration

### Research Methodology
1. **Literature Review**: Comprehensive review of existing research and documentation
2. **Practical Analysis**: Hands-on experimentation and testing
3. **Expert Consultation**: Engagement with domain experts and practitioners
4. **Case Studies**: Analysis of real-world implementations and use cases

## Next Steps

### Immediate Actions (Week 1)
- [ ] Complete literature review for fundamental concepts
- [ ] Identify and contact key researchers in the field
- [ ] Set up testing environment for practical experiments
- [ ] Document initial findings and hypotheses

### Short-term Goals (Month 1)
- [ ] Answer top 3 research questions
- [ ] Develop proof-of-concept implementation
- [ ] Publish initial findings and get community feedback
- [ ] Refine research focus based on discoveries

### Long-term Objectives
- [ ] Contribute to the body of knowledge in {topic}
- [ ] Develop best practices and guidelines
- [ ] Create educational resources and tutorials
- [ ] Build community around the research

## Resources and References

### Key Resources
- Official documentation and specifications
- Academic papers and research articles
- Community forums and discussion groups
- Open-source implementations and examples

### Tools and Technologies
- Stepflow for workflow orchestration
- LangChain for AI/ML integration
- MCP for tool connectivity
- Version control and collaboration platforms

## Notes and Observations

*This section will be updated as research progresses...*

---
*Research notes compiled using Stepflow Research Assistant*
"""

        sections = {
            "executive_summary": analysis,
            "research_questions": "\n".join(questions[:5]),
            "methodology": "Literature Review, Practical Analysis, Expert Consultation, Case Studies",
            "next_steps": "Complete literature review, identify experts, set up testing environment"
        }

        return {
            "notes": notes,
            "sections": sections,
            "metadata": {
                "generation_time": datetime.now().isoformat(),
                "topic": topic,
                "question_count": len(questions),
                "note_length": len(notes)
            }
        }

    return RunnableLambda(generate_notes)


@server.langchain_component(name="report_generator")
def create_report_generator():
    """Generate a comprehensive research report."""

    def generate_report(data):
        topic = data["topic"]
        questions = data["questions"]
        analysis = data["analysis"]
        notes = data["notes"]

        # Create comprehensive report
        report = {
            "title": f"Research Report: {topic}",
            "generated_at": datetime.now().isoformat(),
            "topic": topic,
            "executive_summary": {
                "overview": analysis.get("summary", ""),
                "key_findings": analysis.get("key_points", []),
                "statistics": analysis.get("statistics", {})
            },
            "research_questions": {
                "primary": questions.get("questions", [])[:5],
                "secondary": (
                    questions.get("questions", [])[5:10]
                    if len(questions.get("questions", [])) > 5
                    else []
                ),
                "total_count": len(questions.get("questions", []))
            },
            "research_notes": {
                "sections": notes.get("sections", {}),
                "metadata": notes.get("metadata", {})
            },
            "methodology": {
                "approach": (
                    "Mixed methods combining AI analysis and systematic investigation"
                ),
                "tools_used": ["Stepflow", "LangChain", "MCP"],
                "data_sources": [
                    "Initial context",
                    "Generated insights",
                    "Structured analysis"
                ]
            },
            "conclusions": {
                "summary": (
                    f"This research report provides a comprehensive framework "
                    f"for investigating {topic}."
                ),
                "recommendations": [
                    "Proceed with systematic investigation of research questions",
                    "Leverage AI tools for accelerated analysis",
                    "Maintain structured documentation throughout the research process",
                    "Engage with community for validation and feedback"
                ],
                "next_steps": [
                    "Review and prioritize research questions",
                    "Establish research timeline and milestones",
                    "Identify required resources and tools",
                    "Begin initial investigation phase"
                ]
            },
            "metadata": {
                "version": "1.0",
                "generator": "Stepflow Research Assistant",
                "workflow": "AI-powered research workflow",
                "components": {
                    "ai_processing": "LangChain",
                    "file_operations": "MCP",
                    "orchestration": "Stepflow"
                }
            }
        }

        # Convert report to formatted JSON string
        report_json = json.dumps(report, indent=2)

        # Generate summary
        num_questions = len(questions.get('questions', []))
        summary = f"""Research report generated for '{topic}' containing {num_questions} research questions,
comprehensive analysis, structured notes, and actionable recommendations.
The report provides a complete framework for conducting systematic research on the topic."""

        return {
            "report": report,
            "report_json": report_json,
            "summary": summary
        }

    return RunnableLambda(generate_report)



# Main entry point
if __name__ == "__main__":
    import asyncio
    # Create and run the HTTP server
    asyncio.run(server.run())


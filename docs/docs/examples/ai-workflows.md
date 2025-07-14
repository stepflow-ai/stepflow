---
sidebar_position: 3
---

# AI Workflows

StepFlow excels at orchestrating AI-powered workflows. These examples demonstrate how to integrate large language models, build multi-step AI reasoning, and create intelligent document processing pipelines.

## Simple AI Chat Workflow

A basic workflow that processes user questions through OpenAI's chat completion API.

```yaml
name: "Simple AI Assistant"
description: "Answer user questions using OpenAI with context and personalization"

input_schema:
  type: object
  properties:
    user_question:
      type: string
      description: "The user's question"
    user_context:
      type: object
      properties:
        name: { type: string }
        role: { type: string }
        expertise_level: { type: string, enum: ["beginner", "intermediate", "expert"] }
      description: "Context about the user for personalized responses"
    system_role:
      type: string
      default: "helpful assistant"
      description: "The AI's role and personality"
  required: ["user_question"]

steps:
  # Create personalized system instructions
  - id: create_system_prompt
    component: put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            role: { type: string }
            context: { type: object }
        code: |
          role = input['role']
          context = input.get('context', {})

          # Build personalized system prompt
          system_prompt = f"You are a {role}."

          if context.get('name'):
            system_prompt += f" You are speaking with {context['name']}."

          if context.get('role'):
            system_prompt += f" They work as a {context['role']}."

          expertise = context.get('expertise_level', 'intermediate')
          if expertise == 'beginner':
            system_prompt += " Explain concepts clearly and avoid jargon."
          elif expertise == 'expert':
            system_prompt += " You can use technical language and go into depth."
          else:
            system_prompt += " Balance technical accuracy with accessibility."

          system_prompt

  # Execute system prompt creation
  - id: execute_system_prompt
    component: /python/udf
    input:
      blob_id: { $from: { step: create_system_prompt }, path: "blob_id" }
      input:
        role: { $from: { workflow: input }, path: "system_role" }
        context:
          $from: { workflow: input }
          path: "user_context"
          $on_skip: "use_default"
          $default: {}

  # Create chat messages
  - id: create_messages
    component: create_messages
    input:
      system_instructions: { $from: { step: execute_system_prompt } }
      user_prompt: { $from: { workflow: input }, path: "user_question" }

  # Get AI response
  - id: get_ai_response
    component: openai
    input:
      messages: { $from: { step: create_messages }, path: "messages" }
      max_tokens: 500
      temperature: 0.7

  # Post-process response (add metadata, format, etc.)
  - id: format_response
    component: put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            response: { type: string }
            question: { type: string }
            context: { type: object }
        code: |
          import datetime

          {
            'answer': input['response'],
            'original_question': input['question'],
            'personalization_used': bool(input.get('context')),
            'response_length': len(input['response']),
            'timestamp': datetime.datetime.now().isoformat(),
            'confidence': 'high' if len(input['response']) > 100 else 'medium'
          }

  # Execute response formatting
  - id: execute_formatting
    component: /python/udf
    input:
      blob_id: { $from: { step: format_response }, path: "blob_id" }
      input:
        response: { $from: { step: get_ai_response }, path: "response" }
        question: { $from: { workflow: input }, path: "user_question" }
        context:
          $from: { workflow: input }
          path: "user_context"
          $on_skip: "use_default"
          $default: {}

output:
  result: { $from: { step: execute_formatting } }

test:
  cases:
  - name: basic_question
    description: Simple question without context
    input:
      user_question: "What is machine learning?"
      system_role: "helpful assistant"
    output:
      outcome: success
      result:
        answer: "Machine learning is a subset of artificial intelligence..."
        personalization_used: false

  - name: personalized_question
    description: Question with user context for personalization
    input:
      user_question: "How should I approach learning Python?"
      user_context:
        name: "Alice"
        role: "data scientist"
        expertise_level: "beginner"
      system_role: "programming mentor"
    output:
      outcome: success
      result:
        personalization_used: true
```

## Document Analysis Pipeline

A sophisticated workflow that analyzes documents using multiple AI operations.

```yaml
name: "Intelligent Document Analysis"
description: "Analyze documents with AI: extract key info, summarize, and generate insights"

input_schema:
  type: object
  properties:
    document_path:
      type: string
      description: "Path to document file to analyze"
    analysis_depth:
      type: string
      enum: ["quick", "standard", "comprehensive"]
      default: "standard"
      description: "How deep to analyze the document"
    focus_areas:
      type: array
      items:
        type: string
        enum: ["key_points", "sentiment", "entities", "recommendations", "risks"]
      default: ["key_points", "sentiment"]
      description: "Specific areas to focus analysis on"

steps:
  # Load the document
  - id: load_document
    component: load_file
    input:
      path: { $from: { workflow: input }, path: "document_path" }

  # Extract key information
  - id: extract_key_info
    component: create_messages
    input:
      system_instructions: |
        You are a document analysis expert. Extract the most important information from documents.
        Focus on: main topics, key decisions, important dates, people mentioned, and critical facts.
        Format your response as structured JSON with clear categories.
      user_prompt: |
        Please analyze this document and extract key information:

        {{ $from: { step: load_document }, path: "data" }}

  - id: get_key_info
    component: openai
    input:
      messages: { $from: { step: extract_key_info }, path: "messages" }
      max_tokens: 800
      temperature: 0.3

  # Generate summary (runs in parallel with key info extraction)
  - id: create_summary_prompt
    component: create_messages
    input:
      system_instructions: |
        You are a professional summarization expert. Create clear, concise summaries
        that capture the essence of documents. Adapt your summary length to the document size
        and complexity.
      user_prompt: |
        Please provide a comprehensive summary of this document:

        {{ $from: { step: load_document }, path: "data" }}

  - id: get_summary
    component: openai
    input:
      messages: { $from: { step: create_summary_prompt }, path: "messages" }
      max_tokens: 400
      temperature: 0.5

  # Perform sentiment analysis (if requested)
  - id: analyze_sentiment
    component: create_messages
    skip:
      # Skip if sentiment not in focus_areas
      # This would need custom logic to check array membership
    input:
      system_instructions: |
        You are a sentiment analysis expert. Analyze the overall tone and sentiment
        of documents. Provide both overall sentiment and specific emotional indicators.
      user_prompt: |
        Please analyze the sentiment and tone of this document:

        {{ $from: { step: load_document }, path: "data" }}

  - id: get_sentiment
    component: openai
    input:
      messages: { $from: { step: analyze_sentiment }, path: "messages" }
      max_tokens: 300
      temperature: 0.2

  # Generate recommendations (if comprehensive analysis requested)
  - id: generate_recommendations
    component: create_messages
    skip:
      # Skip if not comprehensive analysis
      # This would check if analysis_depth != "comprehensive"
    input:
      system_instructions: |
        You are a strategic advisor. Based on document analysis, provide actionable
        recommendations and next steps. Focus on practical, specific advice.
      user_prompt: |
        Based on this document analysis, what recommendations do you have?

        Key Information: {{ $from: { step: get_key_info }, path: "response" }}
        Summary: {{ $from: { step: get_summary }, path: "response" }}

        Original Document:
        {{ $from: { step: load_document }, path: "data" }}

  - id: get_recommendations
    component: openai
    input:
      messages: { $from: { step: generate_recommendations }, path: "messages" }
      max_tokens: 600
      temperature: 0.6

  # Combine all analysis results
  - id: combine_analysis
    component: put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            key_info: { type: string }
            summary: { type: string }
            sentiment: { type: string }
            recommendations: { type: string }
            metadata: { type: object }
            focus_areas: { type: array }
            analysis_depth: { type: string }
        code: |
          import datetime

          # Build comprehensive analysis report
          analysis = {
            'document_metadata': input['metadata'],
            'analysis_settings': {
              'depth': input['analysis_depth'],
              'focus_areas': input['focus_areas'],
              'timestamp': datetime.datetime.now().isoformat()
            },
            'key_information': input['key_info'],
            'summary': input['summary']
          }

          # Add optional analyses
          if input.get('sentiment'):
            analysis['sentiment_analysis'] = input['sentiment']

          if input.get('recommendations'):
            analysis['recommendations'] = input['recommendations']

          # Generate executive summary
          word_count = len(input['metadata'].get('resolved_path', '').split())
          analysis['executive_summary'] = {
            'document_size': f"{input['metadata'].get('size_bytes', 0)} bytes",
            'analysis_completeness': len([k for k in analysis.keys() if k.endswith('_analysis') or k in ['key_information', 'summary']]),
            'processing_time': 'estimated 30-60 seconds'
          }

          analysis

  # Execute analysis combination
  - id: execute_combination
    component: /python/udf
    input:
      blob_id: { $from: { step: combine_analysis }, path: "blob_id" }
      input:
        key_info: { $from: { step: get_key_info }, path: "response" }
        summary: { $from: { step: get_summary }, path: "response" }
        sentiment:
          $from: { step: get_sentiment }
          path: "response"
          $on_skip: "use_default"
          $default: null
        recommendations:
          $from: { step: get_recommendations }
          path: "response"
          $on_skip: "use_default"
          $default: null
        metadata: { $from: { step: load_document }, path: "metadata" }
        focus_areas: { $from: { workflow: input }, path: "focus_areas" }
        analysis_depth: { $from: { workflow: input }, path: "analysis_depth" }

output:
  analysis: { $from: { step: execute_combination } }

test:
  cases:
  - name: standard_analysis
    description: Standard document analysis with key points and sentiment
    input:
      document_path: "sample-report.txt"
      analysis_depth: "standard"
      focus_areas: ["key_points", "sentiment"]
```

## Multi-Step AI Reasoning

A complex workflow that demonstrates AI reasoning through multiple steps.

:::info Future Component
The `vector://search` component shown below represents a planned future component for vector similarity search. Currently, you can achieve similar functionality using custom Python components with libraries like `chromadb`, `pinecone`, or `weaviate`.
:::

```yaml
name: "Multi-Step AI Research Assistant"
description: "Research a topic using multiple AI reasoning steps and knowledge retrieval"

input_schema:
  type: object
  properties:
    research_question:
      type: string
      description: "The main research question to investigate"
    context_documents:
      type: array
      items:
        type: string
      description: "Paths to relevant documents for context"
    depth_level:
      type: integer
      minimum: 1
      maximum: 5
      default: 3
      description: "How many reasoning iterations to perform"

steps:
  # Step 1: Break down the research question
  - id: analyze_question
    component: create_messages
    input:
      system_instructions: |
        You are a research methodology expert. When given a research question,
        break it down into specific sub-questions and identify what information
        would be needed to answer it comprehensively.
      user_prompt: |
        Please analyze this research question and break it down:

        Question: {{ $from: { workflow: input }, path: "research_question" }}

        Provide:
        1. 3-5 specific sub-questions that need to be answered
        2. Types of information/evidence needed
        3. Potential challenges or limitations
        4. Suggested research approach

  - id: get_question_analysis
    component: openai
    input:
      messages: { $from: { step: analyze_question }, path: "messages" }
      max_tokens: 600
      temperature: 0.4

  # Step 2: Load and process context documents
  - id: load_context_docs
    component: put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            doc_paths: { type: array }
        code: |
          # This would be enhanced with actual file loading
          # For now, simulate loading multiple documents
          documents = []
          for i, path in enumerate(input['doc_paths']):
            documents.append({
              'id': f"doc_{i}",
              'path': path,
              'content': f"[Content of {path} would be loaded here]",
              'summary': f"Summary of document {i+1}"
            })

          {
            'documents': documents,
            'total_docs': len(documents),
            'status': 'loaded'
          }

  - id: execute_doc_loading
    component: /python/udf
    input:
      blob_id: { $from: { step: load_context_docs }, path: "blob_id" }
      input:
        doc_paths: { $from: { workflow: input }, path: "context_documents" }

  # Step 3: Search for relevant information in documents
  - id: search_documents
    component: /vector/search  # Future component
    on_error:
      action: continue
      default_output:
        results: []
        search_performed: false
    input:
      query: { $from: { workflow: input }, path: "research_question" }
      documents: { $from: { step: execute_doc_loading }, path: "documents" }
      max_results: 10
      similarity_threshold: 0.7

  # Step 4: First reasoning iteration
  - id: first_reasoning_step
    component: create_messages
    input:
      system_instructions: |
        You are a research analyst. Based on the research question breakdown and
        available context documents, provide your initial analysis and findings.
        Identify areas where you need more information.
      user_prompt: |
        Research Question: {{ $from: { workflow: input }, path: "research_question" }}

        Question Analysis: {{ $from: { step: get_question_analysis }, path: "response" }}

        Context Information: {{ $from: { step: search_documents }, path: "results" }}

        Please provide your initial analysis and identify what additional information is needed.

  - id: get_first_reasoning
    component: openai
    input:
      messages: { $from: { step: first_reasoning_step }, path: "messages" }
      max_tokens: 800
      temperature: 0.5

  # Step 5: Second reasoning iteration (deeper analysis)
  - id: second_reasoning_step
    component: create_messages
    input:
      system_instructions: |
        You are a senior research analyst. Building on previous analysis,
        provide deeper insights, identify patterns, and develop preliminary conclusions.
        Consider alternative perspectives and potential counterarguments.
      user_prompt: |
        Previous Analysis: {{ $from: { step: get_first_reasoning }, path: "response" }}

        Please build on this analysis with:
        1. Deeper insights and patterns
        2. Alternative perspectives
        3. Preliminary conclusions
        4. Areas of uncertainty

  - id: get_second_reasoning
    component: openai
    input:
      messages: { $from: { step: second_reasoning_step }, path: "messages" }
      max_tokens: 800
      temperature: 0.6

  # Step 6: Generate final research synthesis
  - id: final_synthesis
    component: create_messages
    input:
      system_instructions: |
        You are a research director. Synthesize all previous analysis into a
        comprehensive research report. Provide clear conclusions, acknowledge
        limitations, and suggest next steps.
      user_prompt: |
        Research Question: {{ $from: { workflow: input }, path: "research_question" }}

        Question Breakdown: {{ $from: { step: get_question_analysis }, path: "response" }}

        Initial Analysis: {{ $from: { step: get_first_reasoning }, path: "response" }}

        Deeper Analysis: {{ $from: { step: get_second_reasoning }, path: "response" }}

        Please provide a comprehensive synthesis including:
        1. Executive summary
        2. Key findings
        3. Conclusions and recommendations
        4. Limitations and uncertainties
        5. Suggested next research steps

  - id: get_final_synthesis
    component: openai
    input:
      messages: { $from: { step: final_synthesis }, path: "messages" }
      max_tokens: 1200
      temperature: 0.4

  # Step 7: Create structured research report
  - id: create_research_report
    component: put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            question: { type: string }
            question_analysis: { type: string }
            first_reasoning: { type: string }
            second_reasoning: { type: string }
            final_synthesis: { type: string }
            context_docs: { type: object }
            depth_level: { type: integer }
        code: |
          import datetime

          # Create comprehensive research report
          report = {
            'research_metadata': {
              'original_question': input['question'],
              'depth_level': input['depth_level'],
              'reasoning_steps': 3,
              'context_documents_used': input['context_docs'].get('total_docs', 0),
              'generated_at': datetime.datetime.now().isoformat()
            },
            'methodology': {
              'question_breakdown': input['question_analysis'],
              'approach': 'Multi-step AI reasoning with document context',
              'iterations': ['Initial analysis', 'Deeper insights', 'Final synthesis']
            },
            'analysis': {
              'initial_findings': input['first_reasoning'],
              'deeper_analysis': input['second_reasoning'],
              'final_synthesis': input['final_synthesis']
            },
            'supporting_information': {
              'context_documents': input['context_docs'],
              'search_results': 'Vector search performed' if input['context_docs'].get('status') == 'loaded' else 'No search performed'
            }
          }

          # Add research quality metrics
          total_length = sum(len(str(v)) for v in [
            input['question_analysis'],
            input['first_reasoning'],
            input['second_reasoning'],
            input['final_synthesis']
          ])

          report['quality_metrics'] = {
            'total_analysis_length': total_length,
            'reasoning_depth': 'comprehensive' if total_length > 2000 else 'standard',
            'evidence_base': 'strong' if input['context_docs'].get('total_docs', 0) > 2 else 'limited'
          }

          report

  # Execute report creation
  - id: execute_report_creation
    component: /python/udf
    input:
      blob_id: { $from: { step: create_research_report }, path: "blob_id" }
      input:
        question: { $from: { workflow: input }, path: "research_question" }
        question_analysis: { $from: { step: get_question_analysis }, path: "response" }
        first_reasoning: { $from: { step: get_first_reasoning }, path: "response" }
        second_reasoning: { $from: { step: get_second_reasoning }, path: "response" }
        final_synthesis: { $from: { step: get_final_synthesis }, path: "response" }
        context_docs: { $from: { step: execute_doc_loading } }
        depth_level: { $from: { workflow: input }, path: "depth_level" }

output:
  research_report: { $from: { step: execute_report_creation } }

test:
  cases:
  - name: climate_research
    description: Research climate change impacts on agriculture
    input:
      research_question: "How is climate change affecting agricultural productivity in developing countries?"
      context_documents: ["climate-report-2023.pdf", "agriculture-trends.txt"]
      depth_level: 3
```

:::tip Community Opportunity
Vector similarity search components would enable powerful document retrieval and knowledge base integration. Consider building these using the Python SDK with vector databases like ChromaDB, Pinecone, or local embedding models.
:::

## AI Content Generation Pipeline

Generate structured content using AI with multiple review and refinement steps.

```yaml
name: "AI Content Generation Pipeline"
description: "Generate, review, and refine content using multiple AI steps"

input_schema:
  type: object
  properties:
    content_brief:
      type: object
      properties:
        topic: { type: string }
        target_audience: { type: string }
        content_type: { type: string, enum: ["blog_post", "documentation", "marketing_copy", "technical_guide"] }
        tone: { type: string, enum: ["professional", "casual", "technical", "friendly"] }
        length: { type: string, enum: ["short", "medium", "long"] }
        key_points: { type: array, items: { type: string } }
      required: ["topic", "target_audience", "content_type"]
    review_criteria:
      type: object
      properties:
        check_accuracy: { type: boolean, default: true }
        check_tone: { type: boolean, default: true }
        check_structure: { type: boolean, default: true }
        check_engagement: { type: boolean, default: false }

steps:
  # Generate initial content outline
  - id: create_outline
    component: create_messages
    input:
      system_instructions: |
        You are a content strategist and expert writer. Create detailed outlines
        for various types of content, ensuring logical flow and comprehensive coverage.
      user_prompt: |
        Create a detailed outline for:

        Topic: {{ $from: { workflow: input }, path: "content_brief.topic" }}
        Type: {{ $from: { workflow: input }, path: "content_brief.content_type" }}
        Audience: {{ $from: { workflow: input }, path: "content_brief.target_audience" }}
        Tone: {{ $from: { workflow: input }, path: "content_brief.tone" }}
        Length: {{ $from: { workflow: input }, path: "content_brief.length" }}

        Key points to cover: {{ $from: { workflow: input }, path: "content_brief.key_points" }}

        Provide a structured outline with main sections, subsections, and brief descriptions.

  - id: get_outline
    component: openai
    input:
      messages: { $from: { step: create_outline }, path: "messages" }
      max_tokens: 600
      temperature: 0.6

  # Generate full content based on outline
  - id: generate_content
    component: create_messages
    input:
      system_instructions: |
        You are an expert content writer. Write engaging, well-structured content
        based on provided outlines. Adapt your writing style to match the specified
        tone and target audience.
      user_prompt: |
        Write full content based on this outline:

        {{ $from: { step: get_outline }, path: "response" }}

        Requirements:
        - Target audience: {{ $from: { workflow: input }, path: "content_brief.target_audience" }}
        - Tone: {{ $from: { workflow: input }, path: "content_brief.tone" }}
        - Type: {{ $from: { workflow: input }, path: "content_brief.content_type" }}

        Make it engaging, informative, and well-structured.

  - id: get_initial_content
    component: openai
    input:
      messages: { $from: { step: generate_content }, path: "messages" }
      max_tokens: 1500
      temperature: 0.7

  # Review content for accuracy and structure
  - id: review_content
    component: create_messages
    input:
      system_instructions: |
        You are a content editor and quality assurance expert. Review content
        for accuracy, structure, tone consistency, and overall quality.
        Provide specific, actionable feedback.
      user_prompt: |
        Please review this content for quality and provide feedback:

        {{ $from: { step: get_initial_content }, path: "response" }}

        Review criteria:
        - Accuracy and factual correctness
        - Structure and logical flow
        - Tone consistency (should be {{ $from: { workflow: input }, path: "content_brief.tone" }})
        - Appropriateness for {{ $from: { workflow: input }, path: "content_brief.target_audience" }}

        Provide specific suggestions for improvement.

  - id: get_content_review
    component: openai
    input:
      messages: { $from: { step: review_content }, path: "messages" }
      max_tokens: 800
      temperature: 0.4

  # Refine content based on review
  - id: refine_content
    component: create_messages
    input:
      system_instructions: |
        You are a senior content editor. Refine and improve content based on
        review feedback while maintaining the original intent and key messages.
      user_prompt: |
        Please refine this content based on the review feedback:

        Original Content:
        {{ $from: { step: get_initial_content }, path: "response" }}

        Review Feedback:
        {{ $from: { step: get_content_review }, path: "response" }}

        Provide the improved version of the content.

  - id: get_refined_content
    component: openai
    input:
      messages: { $from: { step: refine_content }, path: "messages" }
      max_tokens: 1500
      temperature: 0.5

  # Generate final content package
  - id: create_content_package
    component: put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            brief: { type: object }
            outline: { type: string }
            initial_content: { type: string }
            review_feedback: { type: string }
            final_content: { type: string }
        code: |
          import datetime

          # Create comprehensive content package
          package = {
            'content_metadata': {
              'topic': input['brief']['topic'],
              'content_type': input['brief']['content_type'],
              'target_audience': input['brief']['target_audience'],
              'tone': input['brief'].get('tone', 'professional'),
              'length_category': input['brief'].get('length', 'medium'),
              'generated_at': datetime.datetime.now().isoformat(),
              'generation_process': 'outline -> draft -> review -> refinement'
            },
            'content_development': {
              'outline': input['outline'],
              'initial_draft': input['initial_content'],
              'review_feedback': input['review_feedback'],
              'final_content': input['final_content']
            },
            'content_analytics': {
              'outline_length': len(input['outline']),
              'initial_draft_length': len(input['initial_content']),
              'final_content_length': len(input['final_content']),
              'improvement_ratio': len(input['final_content']) / len(input['initial_content']) if input['initial_content'] else 1,
              'review_feedback_length': len(input['review_feedback'])
            }
          }

          # Add quality indicators
          package['quality_indicators'] = {
            'went_through_review': True,
            'content_refined': len(input['final_content']) != len(input['initial_content']),
            'comprehensive_outline': len(input['outline']) > 200,
            'detailed_review': len(input['review_feedback']) > 100
          }

          package

  # Execute package creation
  - id: execute_package_creation
    component: /python/udf
    input:
      blob_id: { $from: { step: create_content_package }, path: "blob_id" }
      input:
        brief: { $from: { workflow: input }, path: "content_brief" }
        outline: { $from: { step: get_outline }, path: "response" }
        initial_content: { $from: { step: get_initial_content }, path: "response" }
        review_feedback: { $from: { step: get_content_review }, path: "response" }
        final_content: { $from: { step: get_refined_content }, path: "response" }

output:
  content_package: { $from: { step: execute_package_creation } }

test:
  cases:
  - name: blog_post_generation
    description: Generate a technical blog post
    input:
      content_brief:
        topic: "Introduction to Container Orchestration"
        target_audience: "software developers"
        content_type: "blog_post"
        tone: "technical"
        length: "medium"
        key_points: ["What is orchestration", "Benefits", "Popular tools", "Getting started"]
      review_criteria:
        check_accuracy: true
        check_tone: true
        check_structure: true
```

## Next Steps

These AI workflow examples demonstrate the power of orchestrating multiple AI operations:

- **Sequential Reasoning**: Building complex logic through multiple AI steps
- **Parallel Processing**: Running multiple AI analyses simultaneously
- **Quality Control**: Using AI to review and improve AI-generated content
- **Structured Outputs**: Converting AI responses into structured data

Explore more advanced patterns:
- **[Data Processing](./data-processing.md)** - Combine AI with data transformation pipelines
- **[Custom Components](./custom-components.md)** - Build specialized AI components

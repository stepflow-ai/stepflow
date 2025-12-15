---
date: 2025-10-03
title: "Langflow on Stepflow POC"
description: "Demonstrates the use of Stepflow to execute Langflow flows."
slug: langflow-poc
authors:
  - benchambers
  - natemccall
tags: [announcements]
draft: false
---

> **Note**: This post uses the legacy `$from` syntax. As of December 2025, Stepflow uses the new syntax with `$step`, `$input`, and `$variable`. See the [latest documentation](https://stepflow.org) for current examples.

# Langflow on Stepflow POC

Stepflow was designed to provide a common runtime for GenAI workflow frameworks.
We've been working on a proof-of-concept converting flows from Langflow to Stepflow and executing them using Stepflow.
This post demonstrates the integration and provides implementation details.

<!-- truncate -->

## Demo

For this demo, we created a Langflow flow using the Blog Content generation template.
We configured it to fetch content from `stepflow.org/docs`, and saved it as [flow.json](https://github.com/stepflow-ai/stepflow/blob/main/docs/static/files/2025-09-langflow-poc-flow.json).

![Blog Content Flow](https://github.com/stepflow-ai/stepflow/blob/main/docs/static/img/2025-09-langflow-poc-flow.png?raw=true)

The following command converts the Langflow file to Stepflow and executes it. As this uses the OpenAI API, you will need to set the `OPENAI_API_KEY` environment variable before running the following. If you dont have an OpenAI API key, you can get one [here](https://platform.openai.com/api-keys).

```sh
cd integrations/langflow
uv run stepflow-langflow execute ../../docs/static/files/2025-09-langflow-poc-flow.json '{}' \
    --tweaks '{ "LanguageModelComponent-icleF": { "api_key": "$OPENAI_API_KEY" } }'
```

The Langflow workflow fetches documentation from [stepflow.org](https://stepflow.org/docs/introduction), parses it, and uses GPT-4 to generate a blog post based on the provided instructions.


:::note[Tweaks]
As with Langflow, [tweaks](https://docs.langflow.org/concepts-publish#input-schema) are overlays that allow changing the inputs to specific 'components' prior to execution.
To run other flows you will likely need to specify different component names for the tweaks. You can run `stepflow-langflow analyze flow.json` to discover component IDs in your workflow.
:::

Refer to the [integration README](https://github.com/stepflow-ai/stepflow/blob/main/integrations/langflow/README.md) for more details.

## Details

### Translation Architecture

The Langflow integration demonstrates several key features of Stepflow:

1. **Protocol flexibility**: The Stepflow protocol and Python SDK make it easy to implement custom component logic.
2. **Workflow portability**: The Stepflow workflow system provides a clean target for GenAI workflow translation.

The translation process walks the Langflow graph and produces equivalent Stepflow steps. Each Langflow component becomes:
- A **blob storage step** containing the component's Python code and configuration
- An **executor step** that loads and runs the code using the custom UDF executor

This approach preserves the original Langflow component implementations while adapting them to Stepflow's execution model.

For non-serializable components, we use the following strategies:

- **Embeddings**: the base class isn't serialiazble, but all of the embeddings we have used extend Pydantic models, so we can serialize them to JSON.
- **Components as tools**: rather than executing the component and returning it, we return the code for the component and pass that into the agent, which detects and it instantiates the component-as-tool for execution.

### Transformation Example

Here's how a Langflow component translates to Stepflow:

**Langflow (simplified):**
```json
{
  "nodes": [{
    "id": "TextInput-abc123",
    "data": {
      "type": "TextInput",
      "node": {
        "template": {
          "input_value": {
            "value": "Use the references above for style..."
          }
        }
      }
    }
  }]
}
```

**Stepflow (generated):**
```yaml
steps:
  - id: langflow_textinput-abc123_blob
    component: /builtin/put_blob
    input:
      data:
        code: |
          class TextInputComponent(TextComponent):
            # ... Langflow component code
        template:
          input_value:
            value: "Use the references above for style..."
        component_type: TextInput

  - id: langflow_textinput-abc123
    component: /langflow/udf_executor
    input:
      blob_id:
        $from: { step: langflow_textinput-abc123_blob }
        path: blob_id
```

The UDF executor loads the component code from the blob, instantiates it with Langflow's runtime, and executes it with proper input/output adaptation.

### Benefits

Executing Langflow workflows on Stepflow provides:

1. **Automatic parallelization**: Stepflow executes independent steps concurrently, improving performance for complex workflows.
2. **Distributed execution**: Components can run [remotely](/docs/configuration#routing-configuration) via Stepflow's routing system.
3. **Future scalability**: Support for serverless environments (AWS Lambda, etc.) will come naturally through Stepflow's plugin architecture.

## Next Steps

We plan to continue working on the Langflow integration.
We are especially excited to bring the benefits of Stepflow to executing Langflow flows -- isolation, sharing, distributed execution, etc.

We're also excited that this confirms Stepflow is a good target for GenAI workflows.
We are interested in supporting other workflow systems -- if you are interested in executing flows directly on Stepflow or connecting an existing workflow library or UI to Stepflow, please let us know!
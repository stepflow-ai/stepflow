---
date: 2025-09-12
title: "Langflow on Stepflow POC"
description: "Demonstrates the use of Stepflow to execute Langflow flows."
slug: langflow-poc
authors:
  - benchambers
tags: [announcements]
draft: true
---

# Langflow on Stepflow POC

Stepflow was designed to provide a common runtime for GenAI workflow frameworks.
We've been working on a proof-of-concept converting flows from Langflow to Stepflow and executing them using Stepflow.
This post demonstrates the integration and discusses provides some implementation details.

<!-- truncate -->

## Demo

For this demo, we created a Langflow flow using the Blog Content generation template.
We configured it to fetch content from `stepflow.org/docs`, and saved it as [`flow.json`](/files/2025-09-langflow-poc-flow.json).

![Blog Content Flow](/img/2025-09-langflow-poc-flow.png)

The following command converts the Langflow file to Stepflow and executes it.
You should set the `OPENAI_API_KEY` environment variable before running the following.

```sh
uv run stepflow-langflow execute flow.json \
    --tweaks "{ \"LanguageModelComponent-icleF\": { \"api_key\": \"$OPENAI_API_KEY\" } }"
```


:::note[Tweaks]
As with Langflow, [tweaks](https://docs.langflow.org/concepts-publish#input-schema) allow changing the inputs to specific nodes prior to execution.
To run other flows you will likely need to specify different component names for the tweaks.
:::

Refer to the [docs for more details](https://github.com/stepflow-ai/stepflow/blob/main/integrations/langflow/README.md).

## Details

The Langflow integration was relatively straightforward and demonstrates several key features of Stepflow:

1. The Stepflow protocol and Python SDK make it easy to implement custom component logic.
2. The Stepflow workflow system makes it easy to describe GenAI workflows.

As a result, the bulk of translation consists of walking the Langflow graph and producing equivalent Stepflow nodes.
Currently, every step in a Langflow flow includes custom code -- often a copy of the component definition.
The resulting Stepflow flow stores the custom code for each step as separate blobs and then uses a custom UDF executor to load the code and execute it.
This custom executor adapts the Stepflow inputs to the Langflow signature and then uses code from Langflow to instantiate the component.

The Stepflow execution of a Langflow pipeline already has some benefits:
1. Stepflow executes multiple steps concurrently, allowing additional parallelism the workflow.
2. Stepflow allows running components [remotely](/docs/configuration#routing-configuration), allowing large flows to be distributed.
3. In the future, Stepflow will allow running components in serverless environments like AWS Lambda.

## Next Steps

We plan to continue working on the Langflow integration.
We are especially excited to bring the benefits of Stepflow to executing Langflow flows -- isolation, sharing, distributed execution, etc.

We're also excited that this confirms Stepflow is a good target for GenAI workflows.
We are interested in supporting other workflow systems -- if you are interested in executing flows directly on Stepflow or connecting an existing workflow library or UI to Stepflow, please let us know!
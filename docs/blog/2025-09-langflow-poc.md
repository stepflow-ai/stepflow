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
We've been working on a proof-of-concept converting Langflow flows Stepflow and executing them.
This post demonstrates the integration and discusse some of the steps that were necessary.

<!-- truncate -->

## Demo

For this demo, we created a Langflow flow using the Blog Content generation template.
We configured it to fetch content from `stepflow.org/docs`, and saved it as [`flow.json`](/files/2025-09-langflow-poc-flow.json).

![Blog Content Flow](/img/2025-09-langflow-poc-flow.png)

The following command converts the Langflow file to Stepflow and executes it.
You should set `OPENAI_API_KEY` following environment variables (or manually replace it).

```sh
uv run stepflow-langflow execute flow.json \
    --tweaks "{ \"LanguageModelComponent-icleF\": { \"api_key\": \"$OPENAI_API_KEY\" } }"
```

:::note[Using other flows]
If you use other flows, you will likely need to specify different component names for the tweaks.
:::

Refer to the [docs for more details](https://github.com/stepflow-ai/stepflow/blob/main/integrations/langflow/README.md).

## Details

:::note[TODO]
Add details on the integration
:::

## Next Steps

We plan to continue working on the Langflow integration.
We are especially excited to bring the benefits of Stepflow to executing Langflow flows -- isolation, sharing, distributed execution, etc.

We're also excited that this confirms Stepflow is a good target for GenAI workflows.
We are interested in supporting other workflow systems -- if you are interested in executing flows directly on Stepflow or connecting an existing workflow library or UI to Stepflow, please let us know!
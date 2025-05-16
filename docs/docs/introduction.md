---
sidebar_position: 1
---

# StepFlow Introduction

StepFlow is an open standard for creating and running for Agentic AI workflows.

Workflows are written in JSON or YAML.

## Steps

A workflow is a sequence of *steps*.
Each step has a unique identifier and executes a specific component.
The input to a step can be created from inputs to the flow, the outputs of earlier steps, and literal values.

## Components

Components provide the logic executed by steps.
Each step specifies a specific component to execute.

Components are provided by plugins.
The most common component plugins are described below.

* Built-in components are provided by StepFlow and provide useful building blocks such as evaluation of sub-flows, looping, and conditional execution.
* The component protocol is a JSON-RPC protocol for a component server to provide components.
* Tools provided by MCP (Model Context Protocol) servers can also be used as components.
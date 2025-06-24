<a id="readme-top"></a>

<!-- PROJECT SHIELDS -->
[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![Apache License][license-shield]][license-url]

<!-- PROJECT LOGO -->
<br />
<div align="center">
  <a href="https://github.com/riptano/stepflow">
    <img src="images/logo.png" alt="Logo" width="80" height="80">
  </a>

  <h3 align="center">StepFlow</h3>

  <p align="center">
    Open protocol and runtime for building GenAI workflows
    <br />
    <a href="https://fuzzy-journey-4j3y1we.pages.github.io/"><strong>Explore the docs »</strong></a>
    <br />
    <br />
    <a href="https://github.com/riptano/stepflow">View Demo</a>
    &middot;
    <a href="https://github.com/riptano/stepflow/issues/new?labels=bug&template=bug-report---.md">Report Bug</a>
    &middot;
    <a href="https://github.com/riptano/stepflow/issues/new?labels=enhancement&template=feature-request---.md">Request Feature</a>
  </p>
</div>

<!-- TABLE OF CONTENTS -->
<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#installation">Installation</a></li>
      </ul>
    </li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#roadmap">Roadmap</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
  </ol>
</details>

<!-- ABOUT THE PROJECT -->
## About The Project

StepFlow is an open protocol and runtime for building, executing, and scaling GenAI workflows across local and cloud environments. Its modular architecture ensures secure, isolated execution of components—whether running locally or deployed to production. With durability, fault-tolerance, and an open specification, StepFlow empowers anyone to create, share, and run AI workflows across platforms and tools.

### Key Features

- **⚙️ Reliable, Scalable Workflow Execution**
   Run workflows locally with confidence they'll scale. StepFlow provides built-in durability and fault tolerance—ready for seamless transition to production-scale deployments.
- **🔐 Secure, Isolated Components**
   Each workflow step runs in a sandboxed process or container with strict resource and environment controls. StepFlow's design prioritizes security, reproducibility, and platform independence.
- **🌐 Open, Portable Workflow Standard**
   Build once, run anywhere. The StepFlow protocol is open and extensible, enabling workflow portability across different environments and platforms.

### What StepFlow Enables

- Define AI workflows using YAML or JSON
- Execute workflows with built-in support for parallel execution
- Extend functionality through step services
- Handle errors at both flow and step levels
- Use as both a library and a service

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- GETTING STARTED -->
## Getting Started
To get a local copy up and running quickly follow these simple steps.
### Prerequisites

- Rust 1.70+ (for building from source)
- Python 3.8+ (for Python SDK examples)

### Installation

1. Clone the repository
   ```sh
   git clone https://github.com/riptano/stepflow.git
   cd stepflow
   ```

2. Build the project
   ```sh
   cargo build --release
   ```

3. Run a sample workflow
   ```sh
   cargo run -- run --flow=examples/python/basic.yaml --input=examples/python/input1.json
   ```

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- USAGE EXAMPLES -->
## Usage

### Quick Start Example

Here's a simple workflow that demonstrates basic StepFlow usage:

**workflow.yaml:**
```yaml
input_schema:
  type: object
  properties:
    m:
      type: integer
    n:
      type: integer

steps:
  - id: add_numbers
    component: python://add
    args:
      a: { $from: $input, path: m }
      b: { $from: $input, path: n }

outputs:
  result: { $from: add_numbers, path: result }
```

**input.json:**
```json
{
  "m": 8,
  "n": 5
}
```

**Run the workflow:**
```sh
cargo run -- run --flow=workflow.yaml --input=input.json
```

### Configuration

Create a `stepflow-config.yaml` file to define available plugins:

```yaml
plugins:
  - name: builtin
    type: builtin
  - name: python
    type: stdio
    command: uv
    args: ["--project", "sdks/python", "run", "stepflow_sdk"]
```

_For more examples, please refer to the [Documentation](https://fuzzy-journey-4j3y1we.pages.github.io/)_

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- ROADMAP -->
## Roadmap

- [x] Workflow execution and debugging
- [x] JSON-RPC over stdio protocol for component servers
- [x] Initial Stepflow UI
- [x] SQL state store for durable execution
- [ ] MCP tools as components
- [ ] Container-based component servers
- [ ] JSON-RPC over http protocol for remote execution
- [ ] Improve Python SDK
- [ ] Enrich component libraries
- [ ] Distributed state stores for scalable execution
- [ ] Kubernetes and container based deployments

See the [open issues](https://github.com/riptano/stepflow/issues) for a full list of proposed features (and known issues).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CONTRIBUTING -->
## Contributing

Contributions are what make the open source community such an amazing place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

If you have a suggestion that would make this better, please fork the repo and create a pull request. You can also simply open an issue with the tag "enhancement".
Don't forget to give the project a star! Thanks again!

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

For detailed development instructions, see [CONTRIBUTING.md](CONTRIBUTING.md).

### Top contributors:

<a href="https://github.com/riptano/stepflow/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=riptano/stepflow" alt="contrib.rocks image" />
</a>

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- LICENSE -->
## License

Distributed under the Apache License. See `LICENSE.txt` for more information.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CONTACT -->
## Contact

Project Link: [https://github.com/riptano/stepflow](https://github.com/riptano/stepflow)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- MARKDOWN LINKS & IMAGES -->
<!-- https://www.markdownguide.org/basic-syntax/#reference-style-links -->
[contributors-shield]: https://img.shields.io/github/contributors/riptano/stepflow.svg?style=for-the-badge
[contributors-url]: https://github.com/riptano/stepflow/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/riptano/stepflow.svg?style=for-the-badge
[forks-url]: https://github.com/riptano/stepflow/network/members
[stars-shield]: https://img.shields.io/github/stars/riptano/stepflow.svg?style=for-the-badge
[stars-url]: https://github.com/riptano/stepflow/stargazers
[issues-shield]: https://img.shields.io/github/issues/riptano/stepflow.svg?style=for-the-badge
[issues-url]: https://github.com/riptano/stepflow/issues
[license-shield]: https://img.shields.io/github/license/riptano/stepflow.svg?style=for-the-badge
[license-url]: https://github.com/riptano/stepflow/blob/master/LICENSE.txt

use error_stack::ResultExt as _;
use serde::de::DeserializeOwned;
use stepflow_core::workflow::Flow;
use stepflow_execution::StepFlowExecutor;
use url::Url;

use crate::{
    MainError, error::Result, run::run, serve::serve, stepflow_config::StepflowConfig,
    submit::submit, test::TestOptions,
};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

/// StepFlow command line application.
///
/// Allows running a flow directly (with run)
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Run a workflow directly.
    Run {
        /// Path to the workflow file to execute.
        #[arg(long="flow", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        flow_path: PathBuf,

        /// The path to the stepflow config file.
        ///
        /// If not specified, will look for `stepflow-config.yml` in the directory containing `flow`.
        /// If that isn't found, will also look in the current directory.
        #[arg(long="config", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        config_path: Option<PathBuf>,

        /// The path to the input file to execute the workflow with.
        ///
        /// Should be JSON or YAML. Format is inferred from file extension.
        #[arg(long = "input", value_name = "FILE", value_hint = clap::ValueHint::FilePath, 
              conflicts_with_all = ["input_json", "input_yaml", "format"])]
        input: Option<PathBuf>,

        /// The input value as a JSON string.
        #[arg(long = "input-json", value_name = "JSON", 
              conflicts_with_all = ["input", "input_yaml", "format"])]
        input_json: Option<String>,

        /// The input value as a YAML string.
        #[arg(long = "input-yaml", value_name = "YAML", 
              conflicts_with_all = ["input", "input_json", "format"])]
        input_yaml: Option<String>,

        /// The format for stdin input (json or yaml).
        ///
        /// Only used when reading from stdin (no other input options specified).
        #[arg(long = "format", value_name = "FORMAT", default_value = "json",
              conflicts_with_all = ["input", "input_json", "input_yaml"])]
        format: InputFormat,

        /// Path to write the output workflow to.
        ///
        /// If not set, will write to stdout.
        #[arg(long = "output", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        output_path: Option<PathBuf>,
    },
    /// Start a StepFlow service.
    Serve {
        /// Port to run the service on.
        #[arg(long, value_name = "PORT", default_value = "8080")]
        port: u16,

        /// The path to the stepflow config file.
        ///
        /// If not specified, will look for `stepflow-config.yml` in the current directory.
        #[arg(long="config", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        config_path: Option<PathBuf>,
    },
    /// Submit a workflow to a StepFlow service.
    Submit {
        /// The URL of the StepFlow service to submit the workflow to.
        #[arg(long, value_name = "URL", default_value = "http://localhost:8080", value_hint = clap::ValueHint::Url)]
        url: Url,

        /// Path to the workflow file to submit.
        #[arg(long="flow", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        flow_path: PathBuf,

        /// The path to the input file to execute the workflow with.
        ///
        /// Should be JSON or YAML. Format is inferred from file extension.
        #[arg(long = "input", value_name = "FILE", value_hint = clap::ValueHint::FilePath, 
              conflicts_with_all = ["input_json", "input_yaml", "format"])]
        input: Option<PathBuf>,

        /// The input value as a JSON string.
        #[arg(long = "input-json", value_name = "JSON", 
              conflicts_with_all = ["input", "input_yaml", "format"])]
        input_json: Option<String>,

        /// The input value as a YAML string.
        #[arg(long = "input-yaml", value_name = "YAML", 
              conflicts_with_all = ["input", "input_json", "format"])]
        input_yaml: Option<String>,

        /// The format for stdin input (json or yaml).
        ///
        /// Only used when reading from stdin (no other input options specified).
        #[arg(long = "format", value_name = "FORMAT", default_value = "json",
              conflicts_with_all = ["input", "input_json", "input_yaml"])]
        format: InputFormat,

        /// Path to write the output workflow to.
        ///
        /// If not set, will write to stdout.
        #[arg(long = "output", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        output_path: Option<PathBuf>,
    },
    /// Run tests defined in a workflow file or directory.
    Test {
        /// Path to the workflow file or directory containing tests.
        #[arg(value_name = "PATH", value_hint = clap::ValueHint::AnyPath)]
        path: PathBuf,

        /// The path to the stepflow config file for tests.
        ///
        /// If not specified, will use hierarchical resolution:
        /// 1. stepflow_config in test section of this workflow
        /// 2. stepflow_config in test section of enclosing workflow (if any)
        /// 3. stepflow-config.test.yml in workflow directory
        /// 4. stepflow-config.yml in workflow directory
        /// 5. stepflow-config.yml in current directory
        #[arg(long="config", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        config_path: Option<PathBuf>,

        #[command(flatten)]
        test_options: TestOptions,
    },
    /// List all available components from a stepflow config.
    ListComponents {
        /// The path to the stepflow config file.
        ///
        /// If not specified, will look for stepflow-config.yml in the current directory.
        #[arg(long="config", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        config_path: Option<PathBuf>,

        /// Output format for the component list.
        #[arg(long = "format", value_name = "FORMAT", default_value = "pretty")]
        format: OutputFormat,

        /// Include component schemas in output.
        ///
        /// Defaults to false for pretty format, true for json/yaml formats.
        #[arg(long = "schemas")]
        schemas: Option<bool>,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum InputFormat {
    Json,
    Yaml,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Pretty,
    Json,
    Yaml,
}

enum Format {
    Yaml,
    Json,
}

impl Format {
    fn from_path(path: &Path) -> Result<Self> {
        let extension = path
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();
        match extension {
            "yml" | "yaml" => Ok(Self::Yaml),
            "json" => Ok(Self::Json),
            _ => Err(MainError::UnrecognizedFileExtension(path.to_owned()).into()),
        }
    }
}

pub fn load<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let rdr = File::open(path).change_context_lazy(|| MainError::MissingFile(path.to_owned()))?;
    let value = match Format::from_path(path)? {
        Format::Json => serde_json::from_reader(rdr)
            .change_context_lazy(|| MainError::InvalidFile(path.to_owned()))?,
        Format::Yaml => serde_yaml_ng::from_reader(rdr)
            .change_context_lazy(|| MainError::InvalidFile(path.to_owned()))?,
    };
    Ok(value)
}

// Look for any of these file names.
const FILE_NAMES: &[&str] = &[
    "stepflow-config.yml",
    "stepflow-config.yaml",
    "stepflow_config.yml",
    "stepflow_config.yaml",
];

fn locate_config(directory: Option<&Path>) -> Result<Option<PathBuf>> {
    // First look for any of the file names in the `directory`.
    if let Some(directory) = directory {
        let mut file_names = FILE_NAMES
            .iter()
            .map(|name| directory.join(name))
            .filter(|path| path.is_file());
        if let Some(path) = file_names.next() {
            // If there are multiple, it is ambiguous so report an error.
            error_stack::ensure!(
                file_names.next().is_none(),
                MainError::MultipleStepflowConfigs(directory.to_owned())
            );
            return Ok(Some(path.to_owned()));
        }
    }

    // Then, look for any of the file names in the current directory.
    let mut file_names = FILE_NAMES
        .iter()
        .map(PathBuf::from)
        .filter(|path| path.is_file());
    if let Some(path) = file_names.next() {
        // If there are multiple, it is ambiguous so report an error.
        error_stack::ensure!(
            file_names.next().is_none(),
            MainError::MultipleStepflowConfigs(std::env::current_dir().unwrap()),
        );
        return Ok(Some(path.to_owned()));
    }

    Ok(None)
}

/// Attempt to load a config file from `config_path`.
///
/// If that is not set, look either in the directory containing `flow_path` or the current directory.
pub fn load_config(
    flow_path: Option<&Path>,
    mut config_path: Option<PathBuf>,
) -> Result<StepflowConfig> {
    if config_path.is_none() {
        let flow_dir = flow_path.and_then(|p| p.parent());
        config_path = locate_config(flow_dir)?;
    }

    tracing::info!("Loading config from {:?}", config_path);

    let config_path = config_path.ok_or(MainError::StepflowConfigNotFound)?;
    let mut config: StepflowConfig = load(&config_path)?;

    if config.working_directory.is_none() {
        let config_dir = config_path
            .parent()
            .expect("config_path should have a parent directory");
        config.working_directory = Some(config_dir.to_owned());
    }
    Ok(config)
}

fn load_input(
    input: Option<PathBuf>,
    input_json: Option<String>,
    input_yaml: Option<String>,
    format: InputFormat,
) -> Result<stepflow_core::workflow::ValueRef> {
    match (input, input_json, input_yaml) {
        (Some(path), None, None) => {
            // Load from file - format is inferred from extension
            load(&path)
        }
        (None, Some(value), None) => {
            // Parse JSON string
            serde_json::from_str(&value)
                .change_context_lazy(|| MainError::InvalidFile(PathBuf::from("input-json")))
        }
        (None, None, Some(value)) => {
            // Parse YAML string
            serde_yaml_ng::from_str(&value)
                .change_context_lazy(|| MainError::InvalidFile(PathBuf::from("input-yaml")))
        }
        (None, None, None) => {
            // Read from stdin using specified format
            let stdin = std::io::stdin();
            let input = match format {
                InputFormat::Json => serde_json::from_reader(stdin)
                    .change_context_lazy(|| MainError::InvalidFile(PathBuf::from("stdin")))?,
                InputFormat::Yaml => serde_yaml_ng::from_reader(stdin)
                    .change_context_lazy(|| MainError::InvalidFile(PathBuf::from("stdin")))?,
            };
            Ok(input)
        }
        _ => {
            // This should be prevented by clap conflicts_with_all, but just in case
            unreachable!("input options are mutually exclusive")
        }
    }
}

pub fn write_output(path: Option<PathBuf>, output: impl serde::Serialize) -> Result<()> {
    match path {
        Some(path) => {
            let format = Format::from_path(&path)?;
            let wtr = File::create(&path)
                .change_context_lazy(|| MainError::CreateOutput(path.clone()))?;
            match format {
                Format::Json => serde_json::to_writer(wtr, &output)
                    .change_context_lazy(|| MainError::WriteOutput(path.clone()))?,
                Format::Yaml => serde_yaml_ng::to_writer(wtr, &output)
                    .change_context_lazy(|| MainError::WriteOutput(path.clone()))?,
            };
            Ok(())
        }
        None => {
            serde_json::to_writer(std::io::stdout(), &output)
                .change_context_lazy(|| MainError::WriteOutput(PathBuf::from("stdout")))?;
            Ok(())
        }
    }
}

pub async fn create_executor(config: StepflowConfig) -> Result<Arc<StepFlowExecutor>> {
    // TODO: Allow other state backends.
    let executor = StepFlowExecutor::new_in_memory();

    let working_directory = config
        .working_directory
        .as_ref()
        .expect("working_directory");

    for plugin_config in config.plugins {
        let (protocol, plugin) = plugin_config.instantiate(working_directory).await?;
        executor
            .register_plugin(protocol, plugin)
            .await
            .change_context(MainError::RegisterPlugin)?;
    }
    Ok(executor)
}

impl Cli {
    pub async fn execute(self) -> Result<()> {
        tracing::debug!("Executing command: {:?}", self);
        match self.command {
            Command::Run {
                flow_path,
                config_path,
                input,
                input_json,
                input_yaml,
                format,
                output_path,
            } => {
                let flow: Arc<Flow> = load(&flow_path)?;
                let config = load_config(Some(&flow_path), config_path)?;
                let executor = create_executor(config).await?;

                let input = load_input(input, input_json, input_yaml, format)?;

                let output = run(executor, flow, input).await?;
                write_output(output_path, output)?;
            }
            Command::Serve { port, config_path } => {
                let config = load_config(None, config_path)?;
                let executor = create_executor(config).await?;

                serve(executor, port).await?;
            }
            Command::Submit {
                url,
                flow_path,
                input,
                input_json,
                input_yaml,
                format,
                output_path,
            } => {
                let flow: Flow = load(&flow_path)?;
                let input = load_input(input, input_json, input_yaml, format)?;

                let output = submit(url, flow, input).await?;
                write_output(output_path, output)?;
            }
            Command::Test {
                path,
                config_path,
                test_options,
            } => {
                let failures = crate::test::run_tests(&path, config_path, test_options).await?;
                if failures {
                    std::process::exit(1);
                }
            }
            Command::ListComponents {
                config_path,
                format,
                schemas,
            } => {
                crate::list_components::list_components(config_path, format, schemas).await?;
            }
        };

        Ok(())
    }
}

use error_stack::ResultExt as _;
use serde::de::DeserializeOwned;
use stepflow_workflow::Flow;
use url::Url;

use crate::{
    MainError, error::Result, run::run, serve::serve, stepflow_config::StepflowConfig,
    submit::submit,
};
use std::{
    fs::File,
    path::{Path, PathBuf},
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
        #[arg(long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        flow: PathBuf,

        /// The path to the stepflow config file.
        ///
        /// If not specified, will look for `stepflow-config.yml` in the directory containing `flow`.
        /// If that isn't found, will also look in the current directory.
        #[arg(long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        config: Option<PathBuf>,

        /// The path to the input file to execute the workflow with.
        ///
        /// Should be JSON or YAML.
        ///
        /// If not set, will read from stdin.
        #[arg(long = "input", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        input_path: Option<PathBuf>,

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
        #[arg(long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Submit a workflow to a StepFlow service.
    Submit {
        /// The URL of the StepFlow service to submit the workflow to.
        #[arg(long, value_name = "URL", default_value = "http://localhost:8080", value_hint = clap::ValueHint::Url)]
        url: Url,

        /// Path to the workflow file to submit.
        #[arg(long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        flow: PathBuf,

        /// The path to the input file to execute the workflow with.
        ///
        /// Should be JSON or YAML.
        ///
        /// If not set, will read from stdin.
        #[arg(long = "input", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        input_path: Option<PathBuf>,

        /// Path to write the output workflow to.
        ///
        /// If not set, will write to stdout.
        #[arg(long = "output", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        output_path: Option<PathBuf>,
    },
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

fn load<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let rdr = File::open(path).change_context_lazy(|| MainError::MissingFile(path.to_owned()))?;
    let value = match Format::from_path(path)? {
        Format::Json => serde_json::from_reader(rdr)
            .change_context_lazy(|| MainError::InvalidFile(path.to_owned()))?,
        Format::Yaml => serde_yml::from_reader(rdr)
            .change_context_lazy(|| MainError::InvalidFile(path.to_owned()))?,
    };
    Ok(value)
}

/// Attempt to load a config file from `config_path`.
///
/// If that is not set, look either in the directory containing `flow_path` or the current directory.
fn load_config(
    flow_path: Option<&PathBuf>,
    mut config_path: Option<PathBuf>,
) -> Result<StepflowConfig> {
    // If config_path is not set, look for `stepflow-config.yml`in the directory containing
    // `flow_path`, then the current directory.
    tracing::info!("Loading config from {:?}", config_path);

    if config_path.is_none() {
        if let Some(flow_path) = flow_path {
            let flow_path = flow_path
                .canonicalize()
                .change_context_lazy(|| MainError::MissingFile(flow_path.clone()))?;
            if let Some(flow_dir) = flow_path.parent() {
                config_path = Some(flow_dir.join("stepflow-config.yml"));
            }
        }
    }

    let config_path = config_path.unwrap_or_else(|| PathBuf::from("stepflow-config.yml"));
    let mut config: StepflowConfig = load(&config_path)?;

    if config.working_directory.is_none() {
        let config_dir = config_path
            .parent()
            .expect("config_path should have a parent directory");
        config.working_directory = Some(config_dir.to_owned());
    }
    Ok(config)
}

fn load_input(path: Option<PathBuf>) -> Result<stepflow_workflow::Value> {
    match path {
        Some(path) => load(&path),
        None => {
            let input = serde_json::from_reader(std::io::stdin())
                .change_context_lazy(|| MainError::InvalidFile(PathBuf::from("stdin")))?;

            Ok(input)
        }
    }
}

fn write_output(path: Option<PathBuf>, output: stepflow_workflow::Value) -> Result<()> {
    match path {
        Some(path) => {
            let format = Format::from_path(&path)?;
            let wtr = File::create(&path)
                .change_context_lazy(|| MainError::CreateOutput(path.clone()))?;
            match format {
                Format::Json => serde_json::to_writer(wtr, &output)
                    .change_context_lazy(|| MainError::WriteOutput(path.clone()))?,
                Format::Yaml => serde_yml::to_writer(wtr, &output)
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

impl Cli {
    pub async fn execute(self) -> Result<()> {
        tracing::debug!("Executing command: {:?}", self);
        match self.command {
            Command::Run {
                flow,
                config,
                input_path,
                output_path,
            } => {
                let mut config = load_config(Some(&flow), config)?;
                let plugins = config.create_plugins().await?;
                let flow: Flow = load(&flow)?;
                let input = load_input(input_path)?;

                let output = run(&plugins, flow, input).await?;
                write_output(output_path, output)?;
            }
            Command::Serve { port, config } => {
                let mut config = load_config(None, config)?;
                let plugins = config.create_plugins().await?;

                serve(&plugins, port).await?;
            }
            Command::Submit {
                url,
                flow,
                input_path,
                output_path,
            } => {
                let flow: Flow = load(&flow)?;
                let input = load_input(input_path)?;

                let output = submit(url, flow, input).await?;
                write_output(output_path, output)?;
            }
        };

        Ok(())
    }
}

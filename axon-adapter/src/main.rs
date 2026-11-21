use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use clap::{Parser, Subcommand, ValueEnum};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Parser, Debug)]
#[command(name = "s2c-axon")]
#[command(about = "Headless Rust adapter for screenshot-to-code")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run screenshot-to-code's Docker services.
    Up {
        #[arg(long, default_value = ".")]
        project_dir: PathBuf,
        #[arg(long)]
        no_build: bool,
    },
    /// Stop screenshot-to-code's Docker services.
    Down {
        #[arg(long, default_value = ".")]
        project_dir: PathBuf,
    },
    /// Generate code from one screenshot through the existing backend WebSocket.
    Generate {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value = "out/s2c")]
        output: PathBuf,
        #[arg(long, default_value = "ws://127.0.0.1:7001/generate-code")]
        backend: String,
        #[arg(long, value_enum, default_value_t = Stack::ReactTailwind)]
        stack: Stack,
        #[arg(long, default_value = "Reconstruct this screenshot faithfully.")]
        prompt: String,
        #[arg(long)]
        design_system: Option<PathBuf>,
        /// Exact screenshot-to-code model display value. Repeat for parallel variants.
        #[arg(long = "model")]
        models: Vec<String>,
        #[arg(long, default_value_t = true)]
        extract_assets: bool,
        #[arg(long, default_value_t = false)]
        generate_images: bool,
    },
}

#[derive(Clone, Debug, ValueEnum)]
enum Stack {
    HtmlCss,
    HtmlTailwind,
    ReactTailwind,
    Bootstrap,
    IonicTailwind,
    VueTailwind,
}

impl Stack {
    fn wire_value(&self) -> &'static str {
        match self {
            Self::HtmlCss => "html_css",
            Self::HtmlTailwind => "html_tailwind",
            Self::ReactTailwind => "react_tailwind",
            Self::Bootstrap => "bootstrap",
            Self::IonicTailwind => "ionic_tailwind",
            Self::VueTailwind => "vue_tailwind",
        }
    }

    fn extension(&self) -> &'static str {
        match self {
            Self::HtmlCss | Self::HtmlTailwind | Self::Bootstrap | Self::IonicTailwind => "html",
            Self::ReactTailwind => "tsx",
            Self::VueTailwind => "vue",
        }
    }
}

#[derive(Debug, Serialize)]
struct RunManifest {
    input: String,
    backend: String,
    stack: String,
    requested_models: Vec<String>,
    reported_models: Vec<String>,
    variant_count: Option<usize>,
    completed_variants: Vec<usize>,
    failed_variants: BTreeMap<usize, String>,
    output_files: BTreeMap<usize, String>,
}

fn data_url(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read input {}", path.display()))?;
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned();
    Ok(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
}

fn run_docker(project_dir: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("docker")
        .arg("compose")
        .args(args)
        .current_dir(project_dir)
        .status()
        .with_context(|| format!("run docker compose in {}", project_dir.display()))?;
    if !status.success() {
        bail!("docker compose exited with {status}");
    }
    Ok(())
}

async fn generate(
    input: PathBuf,
    output: PathBuf,
    backend: String,
    stack: Stack,
    prompt: String,
    design_system: Option<PathBuf>,
    models: Vec<String>,
    extract_assets: bool,
    generate_images: bool,
) -> Result<()> {
    if !input.is_file() {
        bail!("input does not exist or is not a file: {}", input.display());
    }

    let image = data_url(&input)?;
    let design_system_text = match design_system {
        Some(path) => Some(
            fs::read_to_string(&path)
                .with_context(|| format!("read design system {}", path.display()))?,
        ),
        None => None,
    };

    fs::create_dir_all(&output)
        .with_context(|| format!("create output directory {}", output.display()))?;

    let mut payload = json!({
        "generationType": "create",
        "inputMode": "image",
        "prompt": {
            "text": prompt,
            "images": [image],
            "videos": []
        },
        "history": [],
        "generatedCodeConfig": stack.wire_value(),
        "openAiApiKey": null,
        "openAiBaseURL": null,
        "anthropicApiKey": null,
        "geminiApiKey": null,
        "replicateApiKey": null,
        "screenshotOneApiKey": null,
        "isImageGenerationEnabled": generate_images,
        "isAssetExtractionEnabled": extract_assets,
        "editorTheme": "cobalt",
        "codeGenerationModel": "",
        "selectedDesignSystemId": null,
        "isTermOfServiceAccepted": true,
        "designSystem": design_system_text
    });

    if !models.is_empty() {
        payload["variantModels"] = json!(models);
    }

    let (mut socket, _) = connect_async(&backend)
        .await
        .with_context(|| format!("connect to screenshot-to-code backend {backend}"))?;
    socket
        .send(Message::Text(payload.to_string().into()))
        .await
        .context("send generation request")?;

    let mut codes: BTreeMap<usize, String> = BTreeMap::new();
    let mut completed = BTreeSet::new();
    let mut failed: BTreeMap<usize, String> = BTreeMap::new();
    let mut variant_count: Option<usize> = None;
    let mut reported_models = Vec::new();

    while let Some(message) = socket.next().await {
        let message = message.context("read backend websocket message")?;
        match message {
            Message::Text(text) => {
                let event: Value = match serde_json::from_str(&text) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
                let index = event
                    .get("variantIndex")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;

                match kind {
                    "variantCount" => {
                        variant_count = event
                            .get("value")
                            .and_then(Value::as_str)
                            .and_then(|v| v.parse::<usize>().ok());
                    }
                    "variantModels" => {
                        reported_models = event
                            .pointer("/data/models")
                            .and_then(Value::as_array)
                            .map(|items| {
                                items
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(ToOwned::to_owned)
                                    .collect()
                            })
                            .unwrap_or_default();
                    }
                    "setCode" => {
                        if let Some(code) = event.get("value").and_then(Value::as_str) {
                            codes.insert(index, code.to_owned());
                        }
                    }
                    "variantComplete" => {
                        completed.insert(index);
                    }
                    "variantError" => {
                        let reason = event
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or("variant failed")
                            .to_owned();
                        failed.insert(index, reason);
                        completed.insert(index);
                    }
                    "error" => {
                        let reason = event
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or("generation failed");
                        bail!("screenshot-to-code backend error: {reason}");
                    }
                    _ => {}
                }

                if let Some(expected) = variant_count {
                    if completed.len() >= expected {
                        let _ = socket.close(None).await;
                        break;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    if codes.is_empty() {
        bail!("no generated code was returned");
    }

    let mut output_files = BTreeMap::new();
    for (index, code) in &codes {
        let filename = format!("variant-{:02}.{}", index + 1, stack.extension());
        let path = output.join(&filename);
        fs::write(&path, code).with_context(|| format!("write {}", path.display()))?;
        output_files.insert(*index, filename);
    }

    let manifest = RunManifest {
        input: input.display().to_string(),
        backend,
        stack: stack.wire_value().to_string(),
        requested_models: models,
        reported_models,
        variant_count,
        completed_variants: completed.into_iter().collect(),
        failed_variants: failed,
        output_files,
    };
    fs::write(
        output.join("run.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .context("write run manifest")?;

    println!("{}", output.display());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Up {
            project_dir,
            no_build,
        } => {
            if no_build {
                run_docker(&project_dir, &["up", "-d"])?;
            } else {
                run_docker(&project_dir, &["up", "-d", "--build"])?;
            }
        }
        Commands::Down { project_dir } => {
            run_docker(&project_dir, &["down"])?;
        }
        Commands::Generate {
            input,
            output,
            backend,
            stack,
            prompt,
            design_system,
            models,
            extract_assets,
            generate_images,
        } => {
            generate(
                input,
                output,
                backend,
                stack,
                prompt,
                design_system,
                models,
                extract_assets,
                generate_images,
            )
            .await?;
        }
    }
    Ok(())
}

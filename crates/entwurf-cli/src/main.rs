use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use entwurf_core::compiler::{compile, CompileOptions};
use entwurf_core::ingest::{ingest_paths, IngestOptions};
use entwurf_core::project::{Project, VersionReason};
use entwurf_core::prompt::build_prompt;
use entwurf_core::storage::{save_ir, save_project, ProjectPaths};
use entwurf_core::StyleSpec;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("entwurf: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("compile") => compile_command(&args[1..]),
        Some("prompt") => prompt_command(&args[1..]),
        Some("--version") | Some("-V") => {
            println!("entwurf {}", entwurf_core::VERSION);
            Ok(())
        }
        _ => {
            usage();
            Ok(())
        }
    }
}

fn compile_command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut inputs = Vec::new();
    let mut output = PathBuf::from("entwurf-output");
    let mut title = "Knowledge sheet".to_string();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" => {
                index += 1;
                output = PathBuf::from(args.get(index).ok_or("--out needs a path")?);
            }
            "--title" | "-t" => {
                index += 1;
                title = args.get(index).ok_or("--title needs text")?.clone();
            }
            path => inputs.push(PathBuf::from(path)),
        }
        index += 1;
    }
    if inputs.is_empty() {
        return Err("compile needs at least one source file".into());
    }
    let project = Project::new(title.clone());
    let report = ingest_paths(&inputs, &IngestOptions::standard())?;
    let mut ir = report.into_ir(&project.id, "cli-task", &title);
    let style = StyleSpec::default();
    let compile_options = CompileOptions::new(&output);
    let artifacts = compile(&ir, &style, &compile_options)?;
    save_ir(&ProjectPaths::new(&output), &ir.task_id, &ir)?;
    let prompt = build_prompt(&ir, &style, 1, "")?;
    std::fs::write(output.join("task-envelope.md"), prompt.markdown)?;
    let mut project = project;
    let task_id = project.add_task(title, None).id.clone();
    ir.task_id = task_id.clone();
    let version = project.save_version(&task_id, ir, VersionReason::Ingested)?;
    let layout = entwurf_core::layout::layout(
        project.current_ir(&task_id).ok_or("missing IR")?,
        &style,
        &entwurf_core::layout::LayoutOptions::from_style(&style),
    )?;
    let validation = entwurf_core::validate::validate_ir(
        project.current_ir(&task_id).ok_or("missing IR")?,
        &entwurf_core::validate::ValidationOptions::default(),
    )?;
    project.attach_version_outputs(
        &task_id,
        &version,
        layout,
        validation,
        entwurf_core::project::ArtifactPaths {
            svg: artifacts
                .svg_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            png: artifacts
                .png_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            pdf: artifacts.pdf_path.display().to_string(),
            manifest: artifacts
                .manifest_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            ir: artifacts.ir_path.display().to_string(),
        },
    )?;
    save_project(&ProjectPaths::new(&output), &project)?;
    println!(
        "compiled {} -> {} ({} pages, SVG/PNG/PDF/manifest)",
        inputs.len(),
        output.display(),
        artifacts.page_count
    );
    Ok(())
}

fn prompt_command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = args.first().ok_or("prompt needs an IR JSON file")?;
    let ir = entwurf_core::storage::load_ir(std::path::Path::new(path))?;
    let bundle = build_prompt(&ir, &StyleSpec::default(), 1, "")?;
    println!("{}", bundle.markdown);
    Ok(())
}

fn usage() {
    println!(
        "entwurf {}\n\n\
         Usage:\n  entwurf compile <source...> [--out DIR] [--title TEXT]\n  entwurf prompt <knowledge.ir.json>\n  entwurf --version\n\n\
         This CLI is a local acceptance harness; the desktop UI remains the primary product surface.",
        entwurf_core::VERSION
    );
}

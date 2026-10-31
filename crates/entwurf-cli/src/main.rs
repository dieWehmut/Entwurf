use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use entwurf_core::compiler::{compile, CompileOptions};
use entwurf_core::ingest::{ingest_paths, IngestOptions};
use entwurf_core::project::{Project, VersionReason};
use entwurf_core::prompt::build_prompt;
use entwurf_core::storage::{load_ir, save_ir, save_project, ProjectPaths};
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
    let mut title = None;
    let mut reviewed_ir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" => {
                index += 1;
                output = PathBuf::from(args.get(index).ok_or("--out needs a path")?);
            }
            "--title" | "-t" => {
                index += 1;
                title = Some(args.get(index).ok_or("--title needs text")?.clone());
            }
            "--ir" => {
                index += 1;
                if reviewed_ir.is_some() {
                    return Err("--ir accepts one snapshot".into());
                }
                reviewed_ir = Some(PathBuf::from(args.get(index).ok_or("--ir needs a path")?));
            }
            flag if flag.starts_with('-') => return Err(format!("unknown option: {flag}").into()),
            path => inputs.push(PathBuf::from(path)),
        }
        index += 1;
    }
    if reviewed_ir.is_some() && !inputs.is_empty() {
        return Err("choose either --ir or source files".into());
    }
    if reviewed_ir.is_none() && inputs.is_empty() {
        return Err("compile needs source files or --ir".into());
    }
    // The internal acceptance harness can consume an already reviewed snapshot.
    // Loading never marks nodes verified or changes the unresolved review queue.
    let imported = reviewed_ir.as_ref().map(|path| load_ir(path)).transpose()?;
    let title = title.unwrap_or_else(|| {
        imported
            .as_ref()
            .map(|ir| ir.title.clone())
            .unwrap_or_else(|| "Knowledge sheet".into())
    });
    let mut project = Project::new(title.clone());
    let task_id = project.add_task(title.clone(), None).id.clone();
    let (ir, reason) = if let Some(mut ir) = imported {
        // This is a new local project/task import. Bind identity once before
        // compile so exports, manifests, prompt and persisted snapshots agree.
        ir.id = entwurf_core::new_id();
        ir.project_id = project.id.clone();
        ir.task_id = task_id.clone();
        ir.title = title;
        ir.touch();
        (ir, VersionReason::ManualSnapshot)
    } else {
        let report = ingest_paths(&inputs, &IngestOptions::standard())?;
        (
            report.into_ir(&project.id, &task_id, &title),
            VersionReason::Ingested,
        )
    };
    let style = StyleSpec::default();
    let compile_options = CompileOptions::new(&output);
    let artifacts = compile(&ir, &style, &compile_options)?;
    save_ir(&ProjectPaths::new(&output), &ir.task_id, &ir)?;
    let prompt = build_prompt(&ir, &style, 1, "")?;
    std::fs::write(output.join("task-envelope.md"), prompt.markdown)?;
    let version = project.save_version(&task_id, ir, reason)?;
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
        if reviewed_ir.is_some() {
            1
        } else {
            inputs.len()
        },
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
         Usage:\n  entwurf compile <source...> [--out DIR] [--title TEXT]\n  entwurf compile --ir <knowledge.ir.json> [--out DIR] [--title TEXT]\n  entwurf prompt <knowledge.ir.json>\n  entwurf --version\n\n\
         This CLI is a local acceptance harness; the desktop UI remains the primary product surface.",
        entwurf_core::VERSION
    );
}

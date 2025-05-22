use chrono::{DateTime, Utc};
use clap::Parser;
use git2::Repository;
use lopdf::Document;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

static STUB: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/stub.html"));

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the main LaTeX file
    #[arg(short, long)]
    main_tex_file: PathBuf,

    /// Path to the local Git repository
    #[arg(short, long)]
    repo_path: PathBuf,

    /// Path to the output file for history
    #[arg(short, long)]
    output_dir: PathBuf,

    /// Optional arguments for latexmk
    #[arg(long, value_delimiter = ' ')]
    latexmk_args: Vec<String>,
}

impl Args {
    pub fn cache_dir(&self) -> PathBuf {
        self.output_dir.join("cache")
    }
    pub fn output_file(&self) -> PathBuf {
        self.output_dir.join("textory_data.csv")
    }
    pub fn report_file(&self) -> PathBuf {
        self.output_dir.join("textory_report.html")
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let args = Args::parse();

    fs::create_dir_all(args.cache_dir())?;

    let temp_dir = TempDir::new()?;
    let temp_repo_path = temp_dir.path().join("repo");
    // let temp_repo_path = PathBuf::from(&args.cache_dir).join("repo");

    let repo = Repository::open(&args.repo_path)?;
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(git2::Sort::TIME | git2::Sort::REVERSE)?;

    let output_file = args.output_file();
    if std::fs::exists(&output_file)? {
        let ans = inquire::Confirm::new(&format!(
            "Output data file {} exists. Overwrite?",
            output_file.display()
        ))
        .with_default(false)
        .prompt();
        if !ans.unwrap_or(false) {
            log::error!("Cannot produce output without overwriting. Aborting.");
            std::process::exit(1);
        }
    }
    let mut output = fs::File::create(args.output_file())?;
    writeln!(&mut output, "Timestamp,Commit Hash,Page Count")?;

    for (id, oid_result) in revwalk.enumerate() {
        let oid = oid_result?;
        match repo.find_commit(oid) {
            Ok(commit) => {
                println!("Processing commit: {}", commit.id());
                let short_hash = commit.id().to_string()[0..7].to_string();
                let cache_pdf_path = args
                    .cache_dir()
                    .join(format!("thesis_{id}-{}.pdf", short_hash));

                if cache_pdf_path.exists() {
                    println!("Using cached PDF for commit {}", short_hash);
                } else {
                    println!("Compiling...");
                    // Copy the repository to the temporary directory, excluding fsmonitor--daemon.ipc
                    if fs::exists(&temp_repo_path)? {
                        fs::remove_dir_all(&temp_repo_path)?;
                    }
                    copy_repo(&args.repo_path, &temp_repo_path)?;
                    checkout_commit(&temp_repo_path, &commit.id().to_string())?;
                    let pdf_path =
                        compile_latex(&args.main_tex_file, &temp_repo_path, &args.latexmk_args)?;
                    fs::copy(&pdf_path, &cache_pdf_path)?;
                    fs::remove_file(&pdf_path)?;
                }

                let page_count = extract_page_count(&cache_pdf_path)?;
                let commit_time = DateTime::<Utc>::from(
                    std::time::SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_secs(commit.time().seconds() as u64),
                );
                writeln!(
                    &mut output,
                    "{},{},{}",
                    commit_time.to_rfc3339(),
                    commit.id(),
                    page_count
                )?;
            }
            Err(e) => {
                eprintln!("Error finding commit {}: {}", oid, e);
                continue;
            }
        }
    }

    log::info!("History written to {}", args.output_file().display());

    let report_filename = write_report(&args)?;
    log::info!("Report written to {}", report_filename.display());

    Ok(())
}

fn copy_repo(source: &PathBuf, destination: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = destination.join(entry.file_name());

        if path.is_dir() {
            fs::create_dir_all(&dest_path)?;
            copy_repo(&path, &dest_path)?;
        } else if let Some(filename) = path.file_name() {
            if filename != "fsmonitor--daemon.ipc" {
                fs::copy(&path, &dest_path)?;
            }
        }
    }
    Ok(())
}

fn checkout_commit(repo_path: &Path, commit_hash: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Checking out commit: {}", commit_hash);
    let output = Command::new("git")
        .current_dir(repo_path)
        .arg("checkout")
        .arg(commit_hash)
        .output()?;

    if !output.status.success() {
        eprintln!("Error checking out commit {}:", commit_hash);
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        return Err("Git checkout failed".into());
    }

    Ok(())
}

fn compile_latex(
    main_tex_file: &PathBuf,
    repo_path: &PathBuf,
    latexmk_args: &Vec<String>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let output = Command::new("latexmk")
        .current_dir(repo_path)
        .arg("-pdf")
        .arg(main_tex_file)
        .args(latexmk_args)
        .output()?;

    if !output.status.success() {
        eprintln!("Error compiling LaTeX:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        return Err("LaTeX compilation failed".into());
    }

    let pdf_file = main_tex_file.with_extension("pdf");
    println!("Successfully compiled latex to {}", pdf_file.display());
    Ok(repo_path.join(pdf_file))
}

fn extract_page_count(pdf_path: &PathBuf) -> Result<u32, Box<dyn std::error::Error>> {
    let document = Document::load(pdf_path)?;
    Ok(document.get_pages().len() as u32)
}

fn write_report(args: &Args) -> anyhow::Result<PathBuf> {
    let output = std::fs::read_to_string(args.output_file())?;
    let html = STUB.replace("%%DATA%%", &output);
    let filename = args.report_file();
    if std::fs::exists(&filename)? {
        let ans = inquire::Confirm::new(&format!(
            "Report file {} already exists. Overwrite?",
            filename.display()
        ))
        .with_default(false)
        .prompt();

        if !ans.unwrap_or(false) {
            anyhow::bail!(
                "Cannot create report, output file {} exists",
                filename.display()
            );
        }
    }
    std::fs::write(&filename, html)?;
    Ok(filename)
}

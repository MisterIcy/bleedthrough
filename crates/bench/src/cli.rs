//! The command line shared by the adapter's benchmark examples.
//!
//! `bench_parse` (wall time) and `bench_alloc` (allocation counters) are separate programs because
//! the allocation counter replaces the global allocator, which would distort timing. Both bind this
//! module to a parser backend; neither is part of the product.

use std::path::{Path, PathBuf};

use bleedthrough_syntax::ParserBackend;

use crate::analysis_stage::{AnalysisSetup, AnalysisStage};
use crate::baseline::{Baselines, Entries, with_allocation};
use crate::parse_stage::{ParseStage, SourceFile, describe};
use crate::realworld::{Budgets, List, Observed, is_fetched, tree_dir};
use crate::result::{Document, Environment, StageResult};
use crate::stage::{Allocation, EndToEnd, Registry, Stage};
use crate::synth::{Generator, Params};
use crate::timing::Config;

/// The reference input of the committed baseline: small, so the allocation check is fast.
pub const REFERENCE_INPUT: &str =
    "synthetic:seed=1,files=200,avg_bytes=2048,class_pm=600,depth=3,include_pm=100,vuln_pm=0";

/// The input of the analysis allocation baseline: smaller than the parse reference, with planted
/// vulnerable patterns so the finding count is not zero.
pub const ANALYSIS_INPUT: &str =
    "synthetic:seed=1,files=60,avg_bytes=2048,class_pm=600,depth=3,include_pm=100,vuln_pm=200";

/// Runs a pass under an allocation counter and returns what it allocated.
pub type Counter = dyn Fn(&mut dyn FnMut()) -> Allocation;

/// Parsed command line.
#[derive(Debug, Default)]
struct Args {
    input: Option<String>,
    name: Option<String>,
    iterations: Option<u32>,
    warmups: Option<u32>,
    out: Option<PathBuf>,
    commit: Option<String>,
    binary: Option<PathBuf>,
    tree: Option<PathBuf>,
    baseline: Option<PathBuf>,
    update: bool,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |flag: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        let number = |flag: &str, v: String| {
            v.parse::<u32>()
                .map_err(|_| format!("{flag} needs a number"))
        };
        match arg.as_str() {
            "--input" => parsed.input = Some(value("--input")?),
            "--name" => parsed.name = Some(value("--name")?),
            "--iterations" => {
                parsed.iterations = Some(number("--iterations", value("--iterations")?)?)
            }
            "--warmups" => parsed.warmups = Some(number("--warmups", value("--warmups")?)?),
            "--out" => parsed.out = Some(value("--out")?.into()),
            "--commit" => parsed.commit = Some(value("--commit")?),
            "--binary" => parsed.binary = Some(value("--binary")?.into()),
            "--tree" => parsed.tree = Some(value("--tree")?.into()),
            "--baseline" => parsed.baseline = Some(value("--baseline")?.into()),
            "--update" => parsed.update = true,
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    Ok(parsed)
}

/// A loaded input: its name, its files in memory and, when it exists on disk, its directory.
struct Loaded {
    name: String,
    files: Vec<SourceFile>,
    dir: Option<PathBuf>,
}

fn read_dir_files(root: &Path, dir: &Path, out: &mut Vec<SourceFile>) -> Result<(), String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            read_dir_files(root, &path, out)?;
        } else if path.extension().is_some_and(|e| e == "php") {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            out.push(SourceFile {
                path: relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
                bytes: std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
            });
        }
    }
    Ok(())
}

/// Loads `synthetic:<parameters>` (generated in memory, written to `--tree` when given) or
/// `dir:<path>` (every `.php` file below it, named by `--name`, which must start with `real:`).
fn load(args: &Args) -> Result<Loaded, String> {
    let spec = args
        .input
        .clone()
        .unwrap_or_else(|| REFERENCE_INPUT.to_string());
    if let Some(parameters) = spec.strip_prefix("synthetic:") {
        let params = Params::parse(parameters)?;
        let generator = Generator::new(params.clone());
        let mut files = Vec::new();
        generator.walk(&mut |f, _| {
            files.push(SourceFile {
                path: f.path.clone(),
                bytes: f.content.clone().into_bytes(),
            })
        });
        if let Some(dir) = &args.tree {
            generator.write(dir)?;
        }
        return Ok(Loaded {
            name: format!("synthetic:{}", params.name()),
            files,
            dir: args.tree.clone(),
        });
    }
    let path = spec
        .strip_prefix("dir:")
        .ok_or("--input is `synthetic:<parameters>` or `dir:<path>`")?;
    let name = args
        .name
        .clone()
        .filter(|n| n.starts_with("real:"))
        .ok_or("a `dir:` input needs --name real:<project id>")?;
    let root = PathBuf::from(path);
    let mut files = Vec::new();
    read_dir_files(&root, &root, &mut files)?;
    Ok(Loaded {
        name,
        files,
        dir: Some(root),
    })
}

fn write_out(out: Option<&Path>, text: &str) -> Result<(), String> {
    match out {
        Some(path) => std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display())),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

/// The wall-time program: measures the parse stage (and the end-to-end stage when `--binary` is
/// given) and writes a result document.
pub fn time_main<B: ParserBackend>(
    backend: &B,
    args: &[String],
    analysis: &AnalysisSetup<'_>,
) -> Result<(), String> {
    let args = parse_args(args)?;
    let defaults = Config::default();
    let config = Config {
        warmups: args.warmups.unwrap_or(defaults.warmups),
        iterations: args.iterations.unwrap_or(defaults.iterations),
    };
    config.validate()?;
    let loaded = load(&args)?;
    let input = describe(&loaded.name, &loaded.files);
    let mut registry = Registry::new();
    registry.register(Box::new(ParseStage::new(backend, &loaded.files)));
    registry.register(Box::new(AnalysisStage::new(
        backend,
        &loaded.files,
        analysis,
    )));
    if let Some(binary) = &args.binary {
        let dir = loaded
            .dir
            .clone()
            .ok_or("--binary needs an on-disk input: pass --tree DIR with a synthetic input")?;
        registry.register(Box::new(EndToEnd::new(binary.clone(), dir)));
    }
    let results = registry
        .run(&config)?
        .into_iter()
        .map(|(stage, outcome)| StageResult {
            stage,
            input: input.name.clone(),
            outcome,
        })
        .collect();
    let document = Document {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        commit: args.commit,
        environment: Environment::current(),
        inputs: vec![input],
        results,
    };
    write_out(args.out.as_deref(), &document.to_json())
}

/// The allocation program: counts what one parse pass and one analysis pass allocate through `measure` (which the example
/// implements with its allocator) and checks or updates the committed baseline.
///
/// With `--baseline FILE` the counters are compared with the file (exit status 1 on any difference
/// beyond the tolerance); with `--update` the file is rewritten and the diff printed. Without
/// `--baseline` the counters are printed. Returns the process exit status.
pub fn alloc_main<B: ParserBackend>(
    backend: &B,
    args: &[String],
    measure: &Counter,
    analysis: &AnalysisSetup<'_>,
) -> Result<i32, String> {
    let args = parse_args(args)?;
    let loaded = load(&args)?;
    let input = describe(&loaded.name, &loaded.files);
    let mut stage = ParseStage::new(backend, &loaded.files);
    let counters = stage.counters();
    let allocation = measure(&mut || stage.pass());
    let mut now = Entries::new();
    now.insert(
        ("parse".to_string(), input.name.clone()),
        with_allocation(&counters, allocation),
    );
    let analysed = load(&Args {
        input: Some(ANALYSIS_INPUT.to_string()),
        ..Args::default()
    })?;
    let analysed_input = describe(&analysed.name, &analysed.files);
    let mut stage = AnalysisStage::new(backend, &analysed.files, analysis);
    let counters = stage.counters();
    let allocation = measure(&mut || stage.pass());
    now.insert(
        ("analysis".to_string(), analysed_input.name.clone()),
        with_allocation(&counters, allocation),
    );
    let Some(path) = &args.baseline else {
        let rendered = Baselines {
            tolerance_pct: crate::baseline::DEFAULT_TOLERANCE_PCT,
            entries: now,
        }
        .render();
        return write_out(args.out.as_deref(), &rendered).map(|()| 0);
    };
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let baseline = Baselines::parse(&text)?;
    if args.update {
        let new = baseline.updated(now);
        let diff = baseline.diff(&new);
        if diff.is_empty() {
            println!("baselines: unchanged");
        } else {
            diff.iter().for_each(|l| println!("baselines: {l}"));
            std::fs::write(path, new.render()).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        return Ok(0);
    }
    let problems = baseline.check(&now);
    problems.iter().for_each(|p| eprintln!("error: {p}"));
    if problems.is_empty() {
        println!("bench-counters: ok ({})", input.name);
        Ok(0)
    } else {
        eprintln!("bench-counters: FAILED ({} problem(s))", problems.len());
        Ok(1)
    }
}

/// `php -n -l` on a file: `Some(verdict)` when real PHP rejects it, `None` when it accepts it or PHP
/// is not installed.
fn php_rejects(path: &Path) -> Option<String> {
    let output = std::process::Command::new("php")
        .args(["-n", "-l"])
        .arg(path)
        .output()
        .ok()?;
    if output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    let line = text.lines().map(str::trim).find(|l| l.contains("rror"))?;
    let shown = path.display().to_string();
    Some(format!(
        "php -l: {}",
        line.replace(&shown, "<file>")
            .replace("PHP Parse error:  ", "")
            .replace("Parse error: ", "Parse error ")
    ))
}

/// Arguments of the real-world commands.
struct CorpusArgs {
    list: PathBuf,
    budgets: PathBuf,
    cache: PathBuf,
    project: Option<String>,
    update: bool,
}

fn corpus_args(args: &[String]) -> Result<CorpusArgs, String> {
    let mut parsed = CorpusArgs {
        list: "corpora/real-world.toml".into(),
        budgets: "corpora/parse-budget.toml".into(),
        cache: crate::realworld::cache_dir(Path::new(".")),
        project: None,
        update: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |flag: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match arg.as_str() {
            "--list" => parsed.list = value("--list")?.into(),
            "--budgets" => parsed.budgets = value("--budgets")?.into(),
            "--cache" => parsed.cache = value("--cache")?.into(),
            "--project" => parsed.project = Some(value("--project")?),
            "--update" => parsed.update = true,
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    Ok(parsed)
}

/// Exit status of a check that could not run: nothing was evaluated, which is not a pass.
pub const NOT_EVALUATED: i32 = 3;

/// Classifies every file of `dir` with the backend.
fn observe<B: ParserBackend>(backend: &B, dir: &Path) -> Result<(Observed, Vec<String>), String> {
    use bleedthrough_syntax::{ParseOptions, ParseQuality};
    let files = crate::realworld::php_files(dir)?;
    let mut observed = Observed::default();
    for file in &files {
        let bytes = std::fs::read(dir.join(file)).map_err(|e| format!("{file}: {e}"))?;
        match backend
            .parse(file, &bytes, &ParseOptions::default())
            .quality()
        {
            ParseQuality::Clean => {}
            ParseQuality::Recovered => observed.recovered.push(file.clone()),
            // `Failed`, and any quality added later, counts against the budget as a failure.
            _ => observed.failed.push(file.clone()),
        }
    }
    Ok((observed, files))
}

/// The parse-failure budget over the fetched real-world projects.
///
/// Projects that are not fetched are reported as unavailable, never as passed; when none could be
/// checked the status is [`NOT_EVALUATED`]. With `--update` the budget is re-derived (consulting
/// `php -l` for files the parser rejects) and the diff printed.
pub fn budget_main<B: ParserBackend>(backend: &B, args: &[String]) -> Result<i32, String> {
    let args = corpus_args(args)?;
    let list = List::parse(
        &std::fs::read_to_string(&args.list)
            .map_err(|e| format!("{}: {e}", args.list.display()))?,
    )?;
    let old = std::fs::read_to_string(&args.budgets)
        .map_or_else(|_| Ok(Budgets::default()), |t| Budgets::parse(&t))?;
    let mut new = old.clone();
    let (mut checked, mut failures) = (0, 0);
    for project in list
        .projects
        .iter()
        .filter(|p| args.project.as_ref().is_none_or(|id| *id == p.id))
    {
        if !is_fetched(&args.cache, project) {
            println!(
                "parse-budget: `{}` unavailable: not fetched (run `cargo xtask fetch-corpus`)",
                project.id
            );
            continue;
        }
        let dir = tree_dir(&args.cache, project);
        let (observed, _) = observe(backend, &dir)?;
        checked += 1;
        if args.update {
            let mut budget = old.projects.get(&project.id).cloned().unwrap_or_default();
            budget.excluded.retain(|file, _| {
                observed.recovered.contains(file) || observed.failed.contains(file)
            });
            for file in observed.recovered.iter().chain(&observed.failed) {
                if !budget.excluded.contains_key(file)
                    && let Some(verdict) = php_rejects(&dir.join(file))
                {
                    budget.excluded.insert(file.clone(), verdict);
                }
            }
            let kept = |files: &[String]| {
                files
                    .iter()
                    .filter(|f| !budget.excluded.contains_key(*f))
                    .count() as u64
            };
            budget.recovered = kept(&observed.recovered);
            budget.failed = kept(&observed.failed);
            new.projects.insert(project.id.clone(), budget);
            continue;
        }
        let problems = old.check(&project.id, &observed);
        failures += problems.len();
        problems.iter().for_each(|p| eprintln!("error: {p}"));
        if problems.is_empty() {
            println!("parse-budget: `{}` ok", project.id);
        }
    }
    if args.update {
        let diff = budget_diff(&old, &new);
        if diff.is_empty() {
            println!("parse-budget: unchanged");
        } else {
            diff.iter().for_each(|l| println!("parse-budget: {l}"));
            std::fs::write(&args.budgets, new.render())
                .map_err(|e| format!("{}: {e}", args.budgets.display()))?;
        }
        return Ok(0);
    }
    Ok(if failures > 0 {
        1
    } else if checked == 0 {
        println!("parse-budget: not evaluated: no real-world project is fetched");
        NOT_EVALUATED
    } else {
        0
    })
}

fn budget_diff(old: &Budgets, new: &Budgets) -> Vec<String> {
    let mut lines = Vec::new();
    for (id, budget) in &new.projects {
        let before = old.projects.get(id);
        if before != Some(budget) {
            lines.push(format!(
                "`{id}` recovered {} -> {}, failed {} -> {}, excluded {} -> {}",
                before.map_or(0, |b| b.recovered),
                budget.recovered,
                before.map_or(0, |b| b.failed),
                budget.failed,
                before.map_or(0, |b| b.excluded.len()),
                budget.excluded.len()
            ));
        }
    }
    lines
}

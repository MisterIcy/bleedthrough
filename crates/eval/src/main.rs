//! The evaluator's command line; `cargo xtask check-corpus`, `eval` and `coverage` run it.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use bleedthrough_eval::corpus::Corpus;
use bleedthrough_eval::coverage;
use bleedthrough_eval::expectations::Expectations;
use bleedthrough_eval::result::{Evaluation, Verdict, table, to_json};
use bleedthrough_eval::run::{Obtained, ci_exit_code, evaluate, evaluate_metrics_only, run_scan};

const USAGE: &str = "\
usage: bleedthrough-eval <command> [options]

commands:
  check-corpus   completeness and label checks of the corpus (offline, no scan needed)
  coverage       the shape-coverage report per weakness class
  eval           score a scan's json report against the labels and the expectations

options:
  --root DIR       the corpus (default fixtures/vuln)
  --binary PATH    the bleedthrough binary to scan with (eval)
  --report FILE    use this json report instead of running the binary (eval)
  --json FILE      also write the machine-readable result (eval)
  --update         rewrite expectations.toml from this result and print the diff (eval)
  --ci             exit 0 on `not evaluated` while the scan switch is false (eval)
  --labels         score a generated tree: read <root>/labels.toml, allow unlisted files, no
                   expectations (metrics only) (eval)

exit status of eval: 0 pass, 1 fail, 3 not evaluated";

struct Options {
    root: PathBuf,
    binary: Option<PathBuf>,
    report: Option<PathBuf>,
    json: Option<PathBuf>,
    update: bool,
    ci: bool,
    labels: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        root: PathBuf::from("fixtures/vuln"),
        binary: None,
        report: None,
        json: None,
        update: false,
        ci: false,
        labels: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |flag: &str| {
            iter.next()
                .map(PathBuf::from)
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match arg.as_str() {
            "--root" => options.root = value("--root")?,
            "--binary" => options.binary = Some(value("--binary")?),
            "--report" => options.report = Some(value("--report")?),
            "--json" => options.json = Some(value("--json")?),
            "--update" => options.update = true,
            "--ci" => options.ci = true,
            "--labels" => options.labels = true,
            other => return Err(format!("unknown option `{other}`\n{USAGE}")),
        }
    }
    Ok(options)
}

fn check_corpus(options: &Options) -> Result<u8, String> {
    let corpus = Corpus::load(&options.root)?;
    let errors = corpus.check();
    errors.iter().for_each(|e| eprintln!("error: {e}"));
    if errors.is_empty() {
        println!("check-corpus: ok ({} cases)", corpus.cases.len());
        Ok(0)
    } else {
        eprintln!("check-corpus: FAILED ({} problem(s))", errors.len());
        Ok(1)
    }
}

fn show_coverage(options: &Options) -> Result<u8, String> {
    let corpus = Corpus::load(&options.root)?;
    print!(
        "{}",
        coverage::render(&coverage::coverage(&corpus), &corpus.shapes)
    );
    Ok(0)
}

fn eval_labels(options: &Options) -> Result<u8, String> {
    let corpus = Corpus::load_named(&options.root, "labels.toml")?;
    let problems = corpus.check_labels();
    if !problems.is_empty() {
        problems.iter().for_each(|e| eprintln!("error: {e}"));
        return Err("the label file is invalid".to_string());
    }
    let report = options
        .report
        .as_ref()
        .ok_or("--labels needs --report FILE")?;
    let text = fs::read_to_string(report).map_err(|e| format!("{}: {e}", report.display()))?;
    let evaluation = evaluate_metrics_only(&corpus, &text);
    print!("{}", table(&evaluation));
    if let Some(file) = &options.json {
        fs::write(file, to_json(&evaluation, &corpus))
            .map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(evaluation.verdict.exit_code())
}

fn eval(options: &Options) -> Result<u8, String> {
    if options.labels {
        return eval_labels(options);
    }
    let corpus = Corpus::load(&options.root)?;
    let problems = corpus.check();
    if !problems.is_empty() {
        problems.iter().for_each(|e| eprintln!("error: {e}"));
        return Err("the corpus is invalid; run check-corpus".to_string());
    }
    let path = options.root.join("expectations.toml");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let expectations = Expectations::parse(&text)?;
    let obtained = match (&options.report, &options.binary) {
        (Some(file), _) => Obtained::Report(
            fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?,
        ),
        (None, Some(binary)) => run_scan(binary, &options.root),
        (None, None) => return Err("give --binary or --report".to_string()),
    };
    let evaluation = evaluate(&corpus, &expectations, &obtained);
    print!("{}", table(&evaluation));
    if let Some(file) = &options.json {
        fs::write(file, to_json(&evaluation, &corpus))
            .map_err(|e| format!("{}: {e}", file.display()))?;
    }
    if options.update {
        return update(options, &corpus, &expectations, &evaluation, &path);
    }
    Ok(if options.ci {
        ci_exit_code(evaluation.verdict, &expectations)
    } else {
        evaluation.verdict.exit_code()
    })
}

fn update(
    _options: &Options,
    corpus: &Corpus,
    old: &Expectations,
    evaluation: &Evaluation,
    path: &std::path::Path,
) -> Result<u8, String> {
    let Some(scored) = &evaluation.scored else {
        eprintln!(
            "error: nothing to record: the evaluation has no metrics ({})",
            evaluation.verdict.as_str()
        );
        return Ok(if evaluation.verdict == Verdict::NotEvaluated {
            3
        } else {
            1
        });
    };
    let new = Expectations::recorded(corpus, scored, old.scan_switch);
    let diff = old.diff(&new);
    if diff.is_empty() {
        println!("expectations: unchanged");
    } else {
        diff.iter().for_each(|l| println!("expectations: {l}"));
        fs::write(path, new.render()).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(0)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let result = parse(rest).and_then(|options| match command.as_str() {
        "check-corpus" => check_corpus(&options),
        "coverage" => show_coverage(&options),
        "eval" => eval(&options),
        other => Err(format!("unknown command `{other}`\n{USAGE}")),
    });
    match result {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

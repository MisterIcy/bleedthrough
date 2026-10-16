//! The seeded generator of synthetic PHP trees.
//!
//! Every file is a pure function of the seed, the parameters and its index, so a tree is reproducible
//! byte for byte and can be hashed without touching the disk. The constructs are a small subset the
//! parser is known to accept (a test parses a generated tree with the Mago backend). Trees are never
//! committed.
//!
//! Randomness is a SplitMix64 generator written here: no random-number dependency.

use std::fmt::Write as _;
use std::path::Path;

use sha2::{Digest, Sha256};

/// Called for each generated file, with the label of the pattern planted in it, if any.
pub type Visitor<'a> = dyn FnMut(&File, Option<&Label>) + 'a;

/// A rendered file body and the line range of the planted pattern in it, if any.
type Rendered = (String, Option<(u32, u32)>);

/// Files per directory.
const PER_DIR: u64 = 100;

/// The weakness classes planted, in rotation.
const PLANTED: [Planted; 6] = [
    Planted {
        cwe: "CWE-89",
        sink: "$pdo->query('SELECT * FROM t WHERE id = ' . $value);",
        safe: "$pdo->prepare('SELECT * FROM t WHERE id = ?')->execute([$value]);",
        pre: "$pdo = new \\PDO('sqlite::memory:');",
    },
    Planted {
        cwe: "CWE-78",
        sink: "system('ls -l ' . $value);",
        safe: "system('ls -l ' . escapeshellarg($value));",
        pre: "",
    },
    Planted {
        cwe: "CWE-22",
        sink: "readfile('/var/www/files/' . $value);",
        safe: "readfile('/var/www/files/' . basename($value));",
        pre: "",
    },
    Planted {
        cwe: "CWE-918",
        sink: "$body = file_get_contents($value);",
        safe: "$body = in_array(parse_url($value, PHP_URL_HOST), ['api.example.com'], true) ? file_get_contents($value) : '';",
        pre: "",
    },
    Planted {
        cwe: "CWE-601",
        sink: "header('Location: ' . $value);",
        safe: "header('Location: ' . (str_starts_with($value, '/') ? $value : '/'));",
        pre: "",
    },
    Planted {
        cwe: "CWE-502",
        sink: "$object = unserialize($value);",
        safe: "$object = unserialize($value, ['allowed_classes' => false]);",
        pre: "",
    },
];

struct Planted {
    cwe: &'static str,
    sink: &'static str,
    safe: &'static str,
    pre: &'static str,
}

/// What a tree looks like. All proportions are integers per thousand so equal parameters mean equal
/// bytes on every platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    /// The seed.
    pub seed: u64,
    /// Number of ordinary files (planted twins come on top).
    pub files: u64,
    /// Average file size in bytes (each file is within half of it, either way).
    pub avg_bytes: u32,
    /// Files that are classes, per thousand; the rest hold functions.
    pub class_pm: u32,
    /// Maximum nesting of control structures.
    pub depth: u32,
    /// Files that include an earlier file, per thousand.
    pub include_pm: u32,
    /// Files with a planted vulnerable pattern (and a safe twin file), per thousand.
    pub vuln_pm: u32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            seed: 1,
            files: 200,
            avg_bytes: 2048,
            class_pm: 600,
            depth: 3,
            include_pm: 100,
            vuln_pm: 0,
        }
    }
}

impl Params {
    /// The canonical name of the input, `seed=1,files=200,...`; [`Params::parse`] reads it back.
    pub fn name(&self) -> String {
        format!(
            "seed={},files={},avg_bytes={},class_pm={},depth={},include_pm={},vuln_pm={}",
            self.seed,
            self.files,
            self.avg_bytes,
            self.class_pm,
            self.depth,
            self.include_pm,
            self.vuln_pm
        )
    }

    /// Parses `key=value,...`; missing keys keep their defaults.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut params = Self::default();
        for pair in text.split(',').filter(|p| !p.is_empty()) {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| format!("`{pair}` is not key=value"))?;
            let number = |v: &str| {
                v.parse::<u64>()
                    .map_err(|_| format!("`{key}` needs a number, got `{v}`"))
            };
            let narrow = |n: u64| u32::try_from(n).map_err(|_| format!("`{key}` is too large"));
            match key {
                "seed" => params.seed = number(value)?,
                "files" => params.files = number(value)?,
                "avg_bytes" => params.avg_bytes = narrow(number(value)?)?,
                "class_pm" => params.class_pm = narrow(number(value)?)?.min(1000),
                "depth" => params.depth = narrow(number(value)?)?,
                "include_pm" => params.include_pm = narrow(number(value)?)?.min(1000),
                "vuln_pm" => params.vuln_pm = narrow(number(value)?)?.min(1000),
                other => return Err(format!("unknown parameter `{other}`")),
            }
        }
        Ok(params)
    }
}

/// SplitMix64.
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64, index: u64) -> Self {
        let mut rng = Self(seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        rng.next();
        Self(rng.next())
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }

    fn permille(&mut self, per_thousand: u32) -> bool {
        self.below(1000) < u64::from(per_thousand)
    }
}

/// A planted pattern: the vulnerable file, its safe twin file and the sink's lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// The weakness class.
    pub cwe: &'static str,
    /// The vulnerable file, relative to the tree root.
    pub file: String,
    /// The safe twin file.
    pub twin: String,
    /// First line of the sink in the vulnerable file.
    pub start: u32,
    /// Last line of the sink in the vulnerable file.
    pub end: u32,
}

/// One generated file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Path relative to the tree root, `/`-separated.
    pub path: String,
    /// The content.
    pub content: String,
}

fn path_of(index: u64, suffix: &str) -> String {
    format!("d{:05}/f{index:06}{suffix}.php", index / PER_DIR)
}

fn statements(rng: &mut Rng, depth: u32, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    for _ in 0..1 + rng.below(3) {
        match rng.below(6) {
            0 => {
                let _ = writeln!(out, "{pad}$total = $total + {} * $a;", 1 + rng.below(9));
            }
            1 => {
                let _ = writeln!(out, "{pad}$text = sprintf('%s-%d', $b, $total);");
            }
            2 => {
                let _ = writeln!(out, "{pad}$list[] = strtoupper($b) . (string) $total;");
            }
            3 if depth > 0 => {
                let _ = writeln!(out, "{pad}if ($total > {}) {{", rng.below(50));
                statements(rng, depth - 1, indent + 4, out);
                let _ = writeln!(out, "{pad}}} else {{");
                statements(rng, depth - 1, indent + 4, out);
                let _ = writeln!(out, "{pad}}}");
            }
            4 if depth > 0 => {
                let _ = writeln!(out, "{pad}foreach ($list as $key => $item) {{");
                statements(rng, depth - 1, indent + 4, out);
                let _ = writeln!(out, "{pad}}}");
            }
            _ => {
                let _ = writeln!(out, "{pad}$total += strlen($text);");
            }
        }
    }
}

fn routine_body(rng: &mut Rng, depth: u32, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    let _ = writeln!(out, "{pad}$total = $a;");
    let _ = writeln!(out, "{pad}$text = $b;");
    let _ = writeln!(out, "{pad}$list = [];");
    statements(rng, depth, indent, out);
    let _ = writeln!(out, "{pad}return $total + count($list);");
}

fn sink_function(planted: &Planted, indent: usize, safe: bool, out: &mut String) -> (u32, u32) {
    let pad = " ".repeat(indent);
    if !planted.pre.is_empty() {
        let _ = writeln!(out, "{pad}{}", planted.pre);
    }
    let first = u32::try_from(out.lines().count()).unwrap_or(u32::MAX) + 1;
    let sink = if safe { planted.safe } else { planted.sink };
    let _ = writeln!(out, "{pad}{sink}");
    (first, first)
}

/// The generator for one set of parameters.
#[derive(Debug, Clone)]
pub struct Generator {
    params: Params,
}

impl Generator {
    /// A generator for `params`.
    pub fn new(params: Params) -> Self {
        Self { params }
    }

    /// The parameters.
    pub fn params(&self) -> &Params {
        &self.params
    }

    fn planted_for(&self, index: u64) -> Option<&'static Planted> {
        let mut rng = Rng::new(self.params.seed ^ 0xA5A5, index);
        rng.permille(self.params.vuln_pm)
            .then(|| &PLANTED[usize::try_from(rng.below(PLANTED.len() as u64)).unwrap_or(0)])
    }

    /// The files generated for index `index`: the ordinary file, then the safe twin when one is planted.
    /// The order of the returned files, over ascending indices, is ascending by path.
    #[allow(
        clippy::too_many_lines,
        reason = "one straight-line builder; splitting it would only pass state around"
    )]
    pub fn files_at(&self, index: u64) -> (Vec<File>, Option<Label>) {
        let params = &self.params;
        let mut rng = Rng::new(params.seed, index);
        let is_class = rng.permille(params.class_pm);
        let target =
            u64::from(params.avg_bytes) / 2 + rng.below(u64::from(params.avg_bytes).max(1));
        let mut head = String::from("<?php\n\ndeclare(strict_types=1);\n\n");
        let _ = writeln!(head, "namespace App\\G{:05};\n", index / PER_DIR);
        if index > 0 && rng.permille(params.include_pm) {
            let earlier = rng.below(index);
            let _ = writeln!(
                head,
                "require_once dirname(__DIR__) . '/{}';\n",
                path_of(earlier, "")
            );
        }
        let planted = self.planted_for(index);
        let build = |safe: bool| -> Rendered {
            let mut rng = Rng::new(params.seed, index);
            let mut body = String::new();
            let mut lines = None;
            let mut n = 0u64;
            let (open, close, indent) = if is_class {
                body.push_str(&format!("final class Unit{index}\n{{\n"));
                ("    public function", "}\n", 8)
            } else {
                ("function", "", 4)
            };
            while (body.len() as u64) < target || n == 0 {
                let name = if is_class {
                    format!("m{n}")
                } else {
                    format!("fn{index}_{n}")
                };
                let _ = writeln!(body, "{open} {name}(int $a, string $b): int");
                let pad = if is_class { "    " } else { "" };
                let _ = writeln!(body, "{pad}{{");
                routine_body(&mut rng, params.depth, indent, &mut body);
                let _ = writeln!(body, "{pad}}}\n");
                n += 1;
            }
            if let Some(planted) = planted {
                let pad = if is_class { "    " } else { "" };
                let name = if is_class {
                    "plant".to_string()
                } else {
                    format!("plant{index}")
                };
                let _ = writeln!(body, "{open} {name}(): void");
                let _ = writeln!(body, "{pad}{{");
                let _ = writeln!(body, "{}$value = $_GET['x'] ?? '';", " ".repeat(indent));
                let before = head.lines().count() + body.lines().count();
                let mut inner = String::new();
                let (a, _) = sink_function(planted, indent, safe, &mut inner);
                // `a` counts lines within `inner`; the sink starts after any setup line in it.
                let offset = u32::try_from(before).unwrap_or(0);
                lines = Some((offset + a, offset + a));
                body.push_str(&inner);
                let _ = writeln!(body, "{pad}}}\n");
            }
            body.push_str(close);
            (body, lines)
        };
        let (vulnerable, lines) = build(false);
        let ordinary = File {
            path: path_of(index, ""),
            content: format!("{head}{vulnerable}"),
        };
        let (Some(planted), Some((start, end))) = (planted, lines) else {
            return (vec![ordinary], None);
        };
        let (safe_body, _) = build(true);
        let twin = File {
            path: path_of(index, "_safe"),
            content: format!("{head}{safe_body}"),
        };
        let label = Label {
            cwe: planted.cwe,
            file: ordinary.path.clone(),
            twin: twin.path.clone(),
            start,
            end,
        };
        (vec![ordinary, twin], Some(label))
    }

    /// Calls `visit` for every file in path order, then returns the number of files and bytes and
    /// the tree hash (SHA-256 over each path and content in path order).
    pub fn walk(&self, visit: &mut Visitor<'_>) -> TreeStats {
        let mut hasher = Sha256::new();
        let (mut files, mut bytes) = (0u64, 0u64);
        for index in 0..self.params.files {
            let (generated, label) = self.files_at(index);
            for (n, file) in generated.iter().enumerate() {
                hasher.update(file.path.as_bytes());
                hasher.update([0]);
                hasher.update(file.content.as_bytes());
                hasher.update([0]);
                files += 1;
                bytes += file.content.len() as u64;
                visit(file, if n == 0 { label.as_ref() } else { None });
            }
        }
        let digest = hasher.finalize();
        let hash = digest.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        });
        TreeStats { files, bytes, hash }
    }

    /// The tree hash without keeping any file.
    pub fn tree_hash(&self) -> TreeStats {
        self.walk(&mut |_, _| {})
    }

    /// Writes the tree under `dir` (created if needed) and, when patterns are planted, `labels.toml` in
    /// the corpus manifest format. Returns the statistics.
    pub fn write(&self, dir: &Path) -> Result<TreeStats, String> {
        let mut labels = Vec::new();
        let mut error = None;
        let stats = self.walk(&mut |file, label| {
            if error.is_some() {
                return;
            }
            let target = dir.join(&file.path);
            let result = target
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&target, &file.content));
            if let Err(e) = result {
                error = Some(format!("{}: {e}", target.display()));
            }
            if let Some(label) = label {
                labels.push(label.clone());
            }
        });
        if let Some(e) = error {
            return Err(e);
        }
        if self.params.vuln_pm > 0 {
            std::fs::write(dir.join("labels.toml"), labels_toml(&labels))
                .map_err(|e| format!("labels.toml: {e}"))?;
        }
        Ok(stats)
    }

    /// The label file for every planted pattern, in the corpus manifest format.
    pub fn labels(&self) -> String {
        let mut labels = Vec::new();
        self.walk(&mut |_, label| labels.extend(label.cloned()));
        labels_toml(&labels)
    }
}

/// Files, bytes and hash of a generated tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeStats {
    /// Number of files.
    pub files: u64,
    /// Total bytes.
    pub bytes: u64,
    /// SHA-256 over every path and content in path order, lower-case hex.
    pub hash: String,
}

/// Renders labels as cases of the vulnerability corpus manifest: each planted pattern is a vulnerable
/// case with its safe twin. `shapes` carries the one vocabulary entry the planted flow has.
pub fn labels_toml(labels: &[Label]) -> String {
    let mut out = String::from(
        "# Generated labels for planted patterns (the vulnerability corpus manifest format).\nshapes = [\"direct\"]\n",
    );
    for (n, label) in labels.iter().enumerate() {
        let id = format!("planted-{n:05}");
        let _ = write!(
            out,
            "\n[[case]]\nid = \"{id}\"\ncwe = \"{cwe}\"\nlabel = \"vulnerable\"\ndescription = \"planted by the synthetic generator\"\nphp = \"8.0+\"\nshapes = [\"direct\"]\ntwin = \"{id}-safe\"\nfix = \"the safe twin file\"\nprovenance = \"generated\"\nlicence = \"MIT OR Apache-2.0\"\nfiles = [\"{file}\"]\n\n[[case.expect]]\nfile = \"{file}\"\nlines = [{start}, {end}]\nmin_confidence = \"medium\"\n\n[[case]]\nid = \"{id}-safe\"\ncwe = \"{cwe}\"\nlabel = \"safe\"\ndescription = \"safe twin of {id}\"\nphp = \"8.0+\"\nshapes = [\"direct\"]\ntwin = \"{id}\"\nprovenance = \"generated\"\nlicence = \"MIT OR Apache-2.0\"\nfiles = [\"{twin}\"]\n",
            cwe = label.cwe,
            file = label.file,
            twin = label.twin,
            start = label.start,
            end = label.end,
        );
    }
    out
}

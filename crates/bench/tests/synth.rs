//! The synthetic generator: determinism, order, parameters and labels.

use bleedthrough_bench::synth::{Generator, Params};

fn params(seed: u64, files: u64, vuln_pm: u32) -> Params {
    Params {
        seed,
        files,
        vuln_pm,
        ..Params::default()
    }
}

#[test]
fn one_seed_gives_identical_trees() {
    let a = Generator::new(params(7, 300, 100)).tree_hash();
    let b = Generator::new(params(7, 300, 100)).tree_hash();
    assert_eq!(a, b);
    assert!(a.files >= 300 && a.bytes > 0);
    assert_eq!(a.hash.len(), 64);
}

#[test]
fn other_seeds_and_parameters_give_other_trees() {
    let base = Generator::new(params(7, 100, 0)).tree_hash().hash;
    assert_ne!(base, Generator::new(params(8, 100, 0)).tree_hash().hash);
    assert_ne!(base, Generator::new(params(7, 101, 0)).tree_hash().hash);
    let mut bigger = params(7, 100, 0);
    bigger.avg_bytes = 4096;
    assert_ne!(base, Generator::new(bigger).tree_hash().hash);
}

#[test]
fn files_come_in_ascending_path_order_so_the_hash_is_a_sorted_hash() {
    let mut paths = Vec::new();
    Generator::new(params(3, 450, 300)).walk(&mut |f, _| paths.push(f.path.clone()));
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
    assert!(paths.iter().any(|p| p.ends_with("_safe.php")));
}

#[test]
fn a_file_depends_only_on_its_index() {
    let g = Generator::new(params(5, 50, 0));
    assert_eq!(g.files_at(17), g.files_at(17));
    let mut whole = Vec::new();
    g.walk(&mut |f, _| whole.push(f.clone()));
    assert_eq!(whole[17], g.files_at(17).0[0]);
}

#[test]
fn parameters_round_trip_through_their_name() {
    let p = Params {
        seed: 9,
        files: 12,
        avg_bytes: 700,
        class_pm: 250,
        depth: 2,
        include_pm: 50,
        vuln_pm: 10,
    };
    assert_eq!(Params::parse(&p.name()), Ok(p));
    assert_eq!(Params::parse("files=5").map(|p| p.files), Ok(5));
    assert!(Params::parse("files=x").is_err());
    assert!(Params::parse("bogus=1").is_err());
}

#[test]
fn average_size_is_respected() {
    let stats = Generator::new(Params {
        avg_bytes: 3000,
        ..params(1, 200, 0)
    })
    .tree_hash();
    let average = stats.bytes / stats.files;
    assert!((1500..6000).contains(&average), "average {average}");
}

#[test]
fn no_vulnerabilities_are_planted_at_density_zero() {
    let g = Generator::new(params(1, 200, 0));
    assert!(!g.labels().contains("[[case]]"));
}

#[test]
fn every_planted_pattern_is_labelled_at_its_sink_line() {
    let g = Generator::new(params(11, 400, 150));
    let mut checked = 0;
    g.walk(&mut |file, label| {
        let Some(label) = label else { return };
        let line = file
            .content
            .lines()
            .nth(label.start as usize - 1)
            .expect("line");
        assert!(
            line.contains("$value") && !line.contains("$_GET"),
            "{} line {}: {line}",
            file.path,
            label.start
        );
        assert_eq!(label.start, label.end);
        checked += 1;
    });
    assert!(checked > 20, "{checked} planted");
    assert_eq!(
        g.labels().matches("label = \"vulnerable\"").count(),
        checked
    );
}

#[test]
fn the_tree_is_written_with_its_labels() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("synth-write");
    let _ = std::fs::remove_dir_all(&dir);
    let g = Generator::new(params(2, 120, 200));
    let stats = g.write(&dir).expect("writes");
    assert!(dir.join("labels.toml").is_file());
    assert!(dir.join("d00000/f000000.php").is_file());
    let mut on_disk = 0;
    for entry in std::fs::read_dir(dir.join("d00000")).expect("dir") {
        let _ = entry.expect("entry");
        on_disk += 1;
    }
    assert!(on_disk >= 100 && stats.files >= 120);
    assert_eq!(
        std::fs::read_to_string(dir.join("labels.toml")).expect("labels"),
        g.labels()
    );
}

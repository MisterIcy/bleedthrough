//! Statement-level flow: branches, loops, switches, try, jumps and declarations.
#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::type_complexity,
    reason = "tests are long tables of cases and the test parser is a dispatch over PHP syntax"
)]

mod common;
use common::*;

/// spec: type-inference/Flow-sensitivity with joins and bounded loops
#[test]
fn control_flow_joins_and_loops() {
    let cases: &[(&str, &str, &str)] = &[
        ("$x = 1;", "x", "1"),
        ("$x = 1; while ($c) { $x = 's'; }", "x", "1|'s'"),
        ("$x = 1; while ($c) { $x = 's'; break; }", "x", "1|'s'"),
        (
            "$x = 1; while ($c) { if ($d) { $x = 's'; continue; } $x = 2.5; }",
            "x",
            "1|float|'s'",
        ),
        (
            "$x = 1; while ($c) { $x = 's'; continue; $x = 3; }",
            "x",
            "1|'s'",
        ),
        ("$x = 1; while (true) { $x = 's'; break; }", "x", "1|'s'"),
        (
            "$x = 1; while ($c) { while ($d) { $x = 's'; break 2; } $x = 2.5; }",
            "x",
            "1|float|'s'",
        ),
        (
            "$x = 1; while ($c) { while ($d) { $x = 's'; continue 2; } $x = 2.5; }",
            "x",
            "1|float|'s'",
        ),
        (
            "$x = 1; while ($c) { while ($d) { $x = 's'; break 1; } $x = 2.5; }",
            "x",
            "1|float",
        ),
        (
            "$x = 1; while ($c) { while ($d) { $x = 's'; break 0; } $x = 2.5; }",
            "x",
            "1|float",
        ),
        (
            "$x = 1; while ($c) { while ($d) { $x = 's'; break $c; } $x = 2.5; }",
            "x",
            "1|float",
        ),
        ("$x = 1; do { $x = 's'; } while ($c);", "x", "'s'"),
        (
            "$x = 1; do { $x = 's'; break; $x = 2.5; } while ($c);",
            "x",
            "'s'",
        ),
        (
            "$x = 1; do { if ($d) { $x = 's'; continue; } $x = 2.5; } while ($c);",
            "x",
            "float|'s'",
        ),
        ("$x = null; do { $x = 1; } while ($x === null);", "x", "1"),
        ("$x = null; while ($x === null) { $x = 1; }", "x", "1"),
        (
            "$x = 0; for ($i = 0; $i < 3; $i++) { $x = 's'; }",
            "x",
            "0|'s'",
        ),
        (
            "$x = 0; for ($i = 0; $i < 3; $i++) { $x = 's'; }",
            "i",
            "int",
        ),
        (
            "$x = 0; for ($i = 0, $j = 1; $i < 3, $j < 4; $i++, $j++) { $x = 's'; }",
            "j",
            "int",
        ),
        ("$x = 0; for (;;) { $x = 's'; break; }", "x", "'s'"),
        ("$x = 0; for (;;) { $x = 's'; }", "x", "unknown"),
        (
            "$x = 0; for ($i = 0; $i < 3; $i++) { if ($c) { continue; } $x = 's'; }",
            "x",
            "0|'s'",
        ),
        (
            "$x = 0; for ($i = 0; $i < 3; $i++) { if ($c) { $x = 's'; continue; } $x = 2.5; }",
            "x",
            "0|float|'s'",
        ),
        (
            "$x = 0; foreach ($xs as $k => $v) { $x = 's'; }",
            "x",
            "0|'s'",
        ),
        (
            "$x = 0; foreach ($xs as $v) { $x = 's'; break; }",
            "x",
            "0|'s'",
        ),
        (
            "$x = 0; foreach ($xs as $v) { if ($c) { $x = 's'; continue; } $x = 2.5; }",
            "x",
            "0|float|'s'",
        ),
        (
            "$x = 0; foreach ($xs as $v) { foreach ($xs as $w) { $x = 's'; break 2; } $x = 2.5; }",
            "x",
            "0|float|'s'",
        ),
        (
            "$x = 0; switch ($c) { case 1: $x = 1; break; case 2: $x = 's'; break; default: $x = 2.5; }",
            "x",
            "1|float|'s'",
        ),
        (
            "$x = 0; switch ($c) { case 1: $x = 1; break; case 2: $x = 's'; break; }",
            "x",
            "0|1|'s'",
        ),
        (
            "$x = 0; switch ($c) { case 1: $x = 1; case 2: $x = 's'; break; }",
            "x",
            "0|'s'",
        ),
        (
            "$x = 0; switch ($c) { case 1: $x = 1; case 2: $x = 's'; }",
            "x",
            "0|'s'",
        ),
        (
            "$x = 0; switch ($c) { default: $x = 1; case 2: $x = 's'; }",
            "x",
            "'s'",
        ),
        (
            "$x = 0; switch ($c) { case 1: if ($d) { $x = 1; break; } $x = 's'; break; default: $x = 2.5; }",
            "x",
            "1|float|'s'",
        ),
        (
            "$x = 0; switch ($c) { case 1: $x = 1; continue; default: $x = 2.5; }",
            "x",
            "1|float",
        ),
        (
            "$x = 0; while ($c) { switch ($d) { case 1: $x = 1; continue 2; case 2: $x = 's'; break; } $x = 2.5; }",
            "x",
            "0|1|float",
        ),
        (
            "$x = 0; while ($c) { switch ($d) { case 1: $x = 1; break 2; case 2: $x = 's'; break; } $x = 2.5; }",
            "x",
            "0|1|float",
        ),
        (
            "$x = 0; switch ($c) { case 1: $x = 1; return; default: $x = 2.5; }",
            "x",
            "float",
        ),
        (
            "$x = 1; try { $x = 's'; } catch (Exception $ex) { $x = 2.5; }",
            "x",
            "float|'s'",
        ),
        (
            "$x = 1; try { $x = 's'; } catch (Exception $ex) { }",
            "x",
            "1|'s'",
        ),
        (
            "$x = 1; try { $x = 's'; $x = 2.5; } catch (Exception $ex) { $y = $x; }",
            "y",
            "unknown|1|float|'s'",
        ),
        (
            "$x = 1; try { $x = 's'; } finally { $y = $x; }",
            "y",
            "1|'s'",
        ),
        (
            "$x = 1; try { $x = 's'; } finally { $x = 2.5; }",
            "x",
            "float",
        ),
        (
            "$x = 1; try { return; } finally { $x = 2.5; }",
            "x",
            "float",
        ),
        (
            "$x = 1; try { $x = 's'; } catch (Exception $ex) { return; }",
            "x",
            "'s'",
        ),
        (
            "$x = 1; try { $x = 's'; } catch (Exception $ex) { return; } finally { $y = $x; }",
            "y",
            "1|'s'",
        ),
        (
            "$x = 1; try { $x = 's'; } catch (Exception $ex) { }",
            "ex",
            "unknown|Exception",
        ),
        (
            "$x = 1; try { $x = 's'; } catch (A|B $ex) { $y = $ex; }",
            "y",
            "unknown|A|B",
        ),
        (
            "$x = 1; try { $x = 's'; } catch (Exception) { $y = 1; }",
            "y",
            "unknown|1",
        ),
        (
            "$x = 1; try { throw new Exception(); } catch (Exception $ex) { $x = 's'; }",
            "x",
            "'s'",
        ),
        ("global $g;", "g", "unknown"),
        ("global $g, $h; $y = $g;", "y", "unknown"),
        ("static $n = 0;", "n", "0"),
        ("static $n;", "n", "null"),
        ("static $a = 1, $b = 's';", "b", "'s'"),
        ("$x = 1; unset($x);", "x", "unknown"),
        ("$x = 1; $y = 2; unset($x, $y);", "y", "unknown"),
        ("$x = [1]; unset($x[0]);", "x", "non-empty-list<1>"),
        ("$x = 1; echo $x, 's';", "x", "1"),
        ("echo $undefined;", "undefined", "unknown"),
        ("const K = 5; $y = K;", "y", "unknown"),
        ("declare(strict_types=1); $x = 1;", "x", "1"),
        ("declare(ticks=1) { $x = 1; }", "x", "1"),
        ("{ $x = 1; }", "x", "1"),
        (
            "$x = 1; if ($c) { $x = 's'; } elseif ($d) { $x = 2.5; } else { $x = null; }",
            "x",
            "float|'s'|null",
        ),
        (
            "$x = 1; if ($c) { $x = 's'; } elseif ($d) { $x = 2.5; }",
            "x",
            "1|float|'s'",
        ),
        ("$x = 1; if ($c) { $x = 's'; } else { return; }", "x", "'s'"),
        (
            "$x = 1; if ($c) { return; } elseif ($d) { $x = 2.5; } else { return; }",
            "x",
            "float",
        ),
        ("if ($c) { $x = 1; }", "x", "unknown|1"),
        ("if ($c) { $x = 1; } else { $x = 2; }", "x", "1|2"),
        ("$x = $c ? 1 : 's';", "x", "1|'s'"),
    ];
    for (body, var, expected) in cases {
        assert_eq!(fl(body, var), *expected, "{body} |> {var}");
    }
}

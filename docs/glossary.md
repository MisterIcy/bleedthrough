# Glossary

The ubiquitous language of Bleedthrough. Code, rules, output and documentation use these terms
with these meanings. Every `## ` heading is one term and appears exactly once.

Public types in domain crates name the term they represent with a doc line `Term: <name>`, or
`Term: exempt` when they represent no domain concept (errors, helpers). `cargo xtask check-glossary`
enforces this. Adding, renaming or removing a term is an explicit edit to this file.

## Source

A place where untrusted data enters the program, such as a request parameter or a file read.

## Sink

A place where data becomes dangerous if it is attacker-controlled, such as a database query or a command execution.

## Sanitizer

An operation that makes tainted data safe for a given class of sink, removing the taint for that class.

## Propagator

An operation that carries taint from its inputs to its output without cleaning it, such as string concatenation.

## Taint

The property of a value that derives from a source and has not been sanitized for the sink under consideration.

## Entry point

A place where execution begins from outside the analysed code, such as a controller action or a CLI command, from which analysis starts.

## Rule

A declarative description of what to detect: which sources, sinks, sanitizers and propagators apply, with an identifier and a message.

## Pack

A bundle of knowledge about one library or framework, expressed as rules and plain data, so that support for a library is added without changing the engine.

## Finding

A reported instance of a rule matching: a tainted flow from a source to a sink, with its location and path.

## Fingerprint

A stable identity of a finding that survives unrelated edits, used to track and baseline findings across runs.

## Severity

How serious a finding is if it is real, independent of how sure the analysis is.

## Confidence

How sure the analysis is that a finding is real, independent of how serious it would be.

## Coverage

What the analysis could and could not examine in a run, so that a clean result is never mistaken for full assurance.

## Summary

A compact description of what a function does to taint, computed once and reused at its call sites.

## Baseline

A recorded set of accepted findings, so that a run reports only findings that are new.

## Parse quality

How completely a source file was parsed, from fully understood to recovered with errors or skipped.

## Fix

A suggested patch that would resolve a finding.

## Suppression

A baseline entry that silences a finding, recorded with a reason.

## Report

The complete, immutable result of one run: its findings in a fixed order, the coverage, the suppressed counts and the run options, ready to be written in any output format.

## Trace

The ordered steps of a flow from a source to a sink that explain why a finding was reported.

## Weakness

A kind of software flaw identified by a CWE number, such as CWE-89 for SQL injection. Only weaknesses in the registry can be used.

## Category

An OWASP Top 10 risk category, always qualified by its edition, such as `OWASP:2021:A03`.

## Taxonomy

The mandatory classification of a rule or finding: at least one weakness and at least one category.

## Registry

The curated, compiled-in list of supported weaknesses and categories, with their official names and the mapping between them. A tag outside the registry is rejected.

## Severity metrics

The eight CVSS 3.1 base metrics a rule declares, from which the score, the vector string and the severity level are derived by one fixed formula.

## Built-in data

What is known about PHP's own functions, classes, interfaces, traits, enums and constants for each supported version: names, signatures, by-reference and optional flags, type text, deprecations and the extension that provides them. It is generated from PHP's stub files and checked against real interpreters, never written by hand, and holds facts only.

## Version mask

The set of PHP versions (8.0 to 8.5) for which a built-in record applies. A symbol whose signature changes between versions has one record per form, each with its own mask.

## Oracle

A real PHP interpreter's Reflection output, kept as checked-in fixtures, used to fill what the stubs lack and to verify the generated built-in data. It is never needed to run the product.

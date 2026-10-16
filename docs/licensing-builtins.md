# Licensing of the built-in data

**Status: unconfirmed.** This page records what is known about the licences that touch the
built-in data, what the project does about them, and the decision the owner must record before
the first public release. It is not legal advice.

## What the data is derived from

The built-in data (`stubs/builtin.data`) is generated from the `*.stub.php` files of the php-src
releases listed in `stubs/MANIFEST.toml`, and checked against, and in part completed from,
Reflection output of the official PHP images (`stubs/reflection/`). The stub files are vendored
unmodified under `third_party/php-src/`.

## Findings

- Every php-src tag from 8.0 to 8.5 carries the **PHP License 3.01** at the repository root and
  the **Zend Engine License 2.00** under `Zend/`. `sapi/fpm/` carries a BSD-style licence of its
  own, and `ext/oci8` a copy of the PHP License. The exact texts, which vary only in copyright
  years, are in `NOTICE`, generated from the vendored files.
- Only the development branches (`master`, `PHP-8.6`) have moved to BSD-3-Clause; no 8.0 to 8.5
  tag has.
- The stub files are source files that are part of php-src. They hold declarations (names,
  parameter and return types, flags) and doc comments.
- Other tools that redistribute PHP built-in signature data: phpstorm-stubs, Phan, PHPStan, Psalm
  and the Mago prelude. They ship their stubs without PHP or Zend licence notices in their own
  distribution. None of them was shown to derive directly from the php-src stub files, and no
  PHP Group statement on reusing signature data was found.

## What the project does

- `NOTICE` carries the full licence texts with their copyright lines, names the data as derived
  from the PHP source files, and ships in release archives and the Docker image
  (`cargo xtask check-builtins` enforces this).
- The data stores facts only: names, types, flags, version availability, default expression text
  and constant values. It never stores documentation prose, comments or examples. The format
  rejects free-form text in its fields.
- Nothing here concludes that the PHP License 3.01 binds data derived from the stubs. The
  `NOTICE` is the conservative choice.

## Decision (required before the first public release)

The project owner, after reviewing the PHP License 3.01, the Zend Engine License 2.00 and the
findings above (with counsel if wished), records the decision here by replacing the status line
and filling the three fields. `cargo xtask check-release` fails until this is done.

Status: unconfirmed
Decided by:
Date:
Decision:

<?php

$factor = 10;
$scale = fn($n) => $n * $factor;
$nested = fn($x) => fn($y) => $x + $y;
$typed = fn(int $x): int => $x;

<?php

class Handlers
{
    public const FORMAT = static function (string $s): string {
        return $s;
    };
    public const LEN = strlen(...);
}

function f($cb = static function (int $x): int { return $x; }) {}

<?php
namespace Docs;

class Box
{
    /**
     * @param int $a
     * @param string|int $b
     * @param array<array-key, Foo> $c
     * @param class-string<\Docs\Box> $d
     * @return int|string
     */
    public function mixup(string $a, string $b, array $c, string $d): int {}

    /**
     * @param array{id: int $broken
     */
    public function bad(array $x) {}
}

<?php

class Immutable
{
    public readonly int $id;

    public function __construct(public readonly string $name = 'x')
    {
        $this->id = 1;
    }
}

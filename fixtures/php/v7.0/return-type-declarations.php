<?php

function names(): array
{
    return ['a', 'b'];
}

class Fluent
{
    public function self(): self
    {
        return $this;
    }
}

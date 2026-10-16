<?php

final class Handler
{
    public function handle(string $value): void
    {
        $object = unserialize($value);
    }
}

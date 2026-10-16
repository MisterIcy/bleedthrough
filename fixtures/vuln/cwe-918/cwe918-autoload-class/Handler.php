<?php

final class Handler
{
    public function handle(string $value): void
    {
        $body = file_get_contents($value);
    }
}

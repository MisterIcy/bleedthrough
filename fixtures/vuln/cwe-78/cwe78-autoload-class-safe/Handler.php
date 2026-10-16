<?php

final class Handler
{
    public function handle(string $value): void
    {
        system('ls -l ' . escapeshellarg($value));
    }
}

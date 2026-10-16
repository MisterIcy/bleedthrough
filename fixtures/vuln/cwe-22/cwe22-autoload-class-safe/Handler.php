<?php

final class Handler
{
    public function handle(string $value): void
    {
        readfile('/var/www/files/' . basename($value));
    }
}

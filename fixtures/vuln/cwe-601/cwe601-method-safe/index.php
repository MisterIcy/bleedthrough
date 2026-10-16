<?php

final class Handler
{
    public function handle(string $value): void
    {
        if (str_starts_with($value, '/') && !str_starts_with($value, '//')) {
            header('Location: ' . $value);
        }
    }
}

(new Handler())->handle($_GET['x']);

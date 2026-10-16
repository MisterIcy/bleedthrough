<?php

final class Handler
{
    public function handle(string $value): void
    {
        if (in_array(parse_url($value, PHP_URL_HOST), ['api.example.com'], true)) {
            $body = file_get_contents($value);
        }
    }
}

(new Handler())->handle($_GET['x']);

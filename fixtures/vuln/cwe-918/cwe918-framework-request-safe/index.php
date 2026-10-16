<?php

final class Request
{
    public function query(string $key): string
    {
        return $_GET[$key] ?? '';
    }
}

$request = new Request();
if (in_array(parse_url($request->query('x'), PHP_URL_HOST), ['api.example.com'], true)) {
    $body = file_get_contents($request->query('x'));
}

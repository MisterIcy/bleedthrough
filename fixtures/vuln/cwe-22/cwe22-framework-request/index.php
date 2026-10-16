<?php

final class Request
{
    public function query(string $key): string
    {
        return $_GET[$key] ?? '';
    }
}

$request = new Request();
readfile('/var/www/files/' . $request->query('x'));

<?php

final class Request
{
    public function query(string $key): string
    {
        return $_GET[$key] ?? '';
    }
}

$request = new Request();
if (str_starts_with($request->query('x'), '/') && !str_starts_with($request->query('x'), '//')) {
    header('Location: ' . $request->query('x'));
}

<?php

final class Request
{
    public function query(string $key): string
    {
        return $_GET[$key] ?? '';
    }
}

$request = new Request();
$object = unserialize($request->query('x'), ['allowed_classes' => false]);

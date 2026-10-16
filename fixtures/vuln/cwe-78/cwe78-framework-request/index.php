<?php

final class Request
{
    public function query(string $key): string
    {
        return $_GET[$key] ?? '';
    }
}

$request = new Request();
system('ls -l ' . $request->query('x'));
